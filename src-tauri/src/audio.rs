/// CPAL audio engine — wraps input/output streams and the DSP chain.
/// Runs at native OS audio latency (WASAPI on Windows, CoreAudio on macOS,
/// ALSA/PipeWire on Linux). No Chromium, no Web Audio API.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, StreamConfig, BufferSize};
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use tauri::{AppHandle, Emitter};
use rustfft::{FftPlanner, num_complex::Complex32};

use crate::dsp::{AudioDeviceInfo, DspChain, DspParams, MetersPayload};

/// Holds the running audio streams.
/// When dropped, streams stop automatically.
pub struct AudioEngine {
    _in_stream:  cpal::Stream,
    _out_stream: cpal::Stream,
    pub params:  Arc<DspParams>,
    pub sample_rate: u32,
    pub latency_ms:  f32,
    pub device_name: String,
}

/// Global engine state (None = stopped, Some = running)
pub type EngineState = Mutex<Option<AudioEngine>>;

impl AudioEngine {
    pub fn start(
        app: AppHandle,
        params: Arc<DspParams>,
        input_name: Option<String>,
        output_name: Option<String>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();

        let in_dev = pick_device(
            host.input_devices().map_err(|e| e.to_string())?,
            &input_name,
            || host.default_input_device(),
            "input",
        )?;

        let out_dev = pick_device(
            host.output_devices().map_err(|e| e.to_string())?,
            &output_name,
            || host.default_output_device(),
            "output",
        )?;

        let in_cfg = in_dev.default_input_config().map_err(|e| e.to_string())?;
        let out_cfg = out_dev.default_output_config().map_err(|e| e.to_string())?;

        let sr = in_cfg.sample_rate().0;
        let in_channels = in_cfg.channels() as usize;
        let device_name = in_dev.name().unwrap_or_default();

        // Estimate latency (buffer size * 2 / sr)
        let buf_frames = 256usize;
        let latency_ms = (buf_frames as f32 * 2.0 / sr as f32) * 1000.0;

        // ── Ring buffer: processed mono → output ────────────────────────────
        let ring_cap = sr as usize / 10; // 100ms of headroom
        let (mut ring_prod, mut ring_cons) = ringbuf::HeapRb::<f32>::new(ring_cap).split();

        // ── Meter channel ────────────────────────────────────────────────────
        let (meters_tx, meters_rx) = mpsc::sync_channel::<MetersPayload>(32);

        // ── DSP chain (owned by input callback closure) ──────────────────────
        let mut dsp = DspChain::new(sr as f64);

        // ── Noise profile transfer: shared between command handler + callback
        let noise_capture: Arc<Mutex<Option<Vec<f32>>>> = Arc::new(Mutex::new(None));
        let nc_callback = noise_capture.clone();

        // ── FFT for spectrum visualizer ──────────────────────────────────────
        let mut spec_planner = FftPlanner::<f32>::new();
        let spec_fft = spec_planner.plan_fft_forward(256);
        let spec_win: Vec<f32> = (0..256)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / 255.0).cos()))
            .collect();
        let mut spec_buf: Vec<Complex32> = vec![Complex32::default(); 256];
        let mut spec_acc: Vec<f32> = vec![0.0; 128];
        let mut spec_count: usize = 0;

        let params_cb = params.clone();
        let out_channels = out_cfg.channels() as usize;

        // ── Build stream configs ─────────────────────────────────────────────
        let in_stream_cfg = StreamConfig {
            channels: in_cfg.channels(),
            sample_rate: in_cfg.sample_rate(),
            buffer_size: BufferSize::Fixed(buf_frames as u32),
        };
        let out_stream_cfg = StreamConfig {
            channels: out_cfg.channels(),
            sample_rate: out_cfg.sample_rate(),
            buffer_size: BufferSize::Fixed(buf_frames as u32),
        };

        // ── Input stream (core DSP loop) ─────────────────────────────────────
        let in_stream = match in_cfg.sample_format() {
            SampleFormat::F32 => in_dev.build_input_stream(
                &in_stream_cfg,
                move |data: &[f32], _| {
                    // Accept noise profile if one was captured
                    if let Ok(mut nc) = nc_callback.try_lock() {
                        if let Some(profile) = nc.take() {
                            dsp.noise.capture_profile(&profile);
                        }
                    }

                    // Down-mix to mono
                    let mono: Vec<f32> = if in_channels == 1 {
                        data.to_vec()
                    } else {
                        data.chunks(in_channels)
                            .map(|ch| ch.iter().sum::<f32>() / in_channels as f32)
                            .collect()
                    };

                    let mut block = mono;

                    // Sync params + process
                    dsp.sync_params(&params_cb);
                    let gate_enabled = params_cb.gate_enabled.load(Ordering::Relaxed);
                    let bypass       = params_cb.bypass.load(Ordering::Relaxed);
                    dsp.process_block(&mut block, gate_enabled, bypass);

                    // Apply monitor gain
                    let mon_gain = db_to_lin(params_cb.monitor_gain_db.load(Ordering::Relaxed));
                    for s in block.iter_mut() { *s *= mon_gain; }

                    // FFT spectrum (every 256 samples)
                    for (i, &s) in block.iter().enumerate().take(256) {
                        spec_buf[i] = Complex32::new(s * spec_win[i], 0.0);
                    }
                    spec_fft.process(&mut spec_buf);
                    for k in 0..128 {
                        spec_acc[k] += spec_buf[k].norm();
                    }
                    spec_count += 1;

                    // Write to output ring (up-mix mono → out_channels)
                    for &s in &block {
                        for _ in 0..out_channels {
                            let _ = ring_prod.push(s);
                        }
                    }

                    // Send meter snapshot
                    let spectrum = if spec_count >= 4 {
                        let bins: Vec<f32> = spec_acc.iter().map(|&v| {
                            let db = 20.0 * (v / spec_count as f32 / 128.0).max(1e-9).log10();
                            ((db + 90.0) / 90.0).clamp(0.0, 1.0)
                        }).collect();
                        spec_acc.fill(0.0);
                        spec_count = 0;
                        bins
                    } else {
                        vec![]
                    };

                    let _ = meters_tx.try_send(MetersPayload {
                        input_db:    dsp.last_input_db,
                        output_db:   dsp.last_output_db,
                        gate_gain:   dsp.last_gate_gain,
                        lufs_m:      dsp.last_lufs.momentary,
                        lufs_st:     dsp.last_lufs.short_term,
                        lufs_i:      dsp.last_lufs.integrated,
                        deess_gr_db: dsp.last_deess_gr,
                        auto_gain_db: dsp.last_auto_gain_db,
                        spectrum,
                    });
                },
                |e| log::error!("Input stream error: {e}"),
                None,
            ).map_err(|e| e.to_string())?,
            fmt => return Err(format!("Unsupported sample format: {fmt:?}")),
        };

        // ── Output stream ────────────────────────────────────────────────────
        let out_stream = out_dev.build_output_stream(
            &out_stream_cfg,
            move |data: &mut [f32], _| {
                for s in data.iter_mut() {
                    *s = ring_cons.pop().unwrap_or(0.0);
                }
            },
            |e| log::error!("Output stream error: {e}"),
            None,
        ).map_err(|e| e.to_string())?;

        in_stream.play().map_err(|e| e.to_string())?;
        out_stream.play().map_err(|e| e.to_string())?;

        // ── Meter → UI thread ────────────────────────────────────────────────
        std::thread::spawn(move || {
            let mut pending: Vec<MetersPayload> = Vec::with_capacity(8);
            let mut last_emit = std::time::Instant::now();

            loop {
                match meters_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                    Ok(m) => pending.push(m),
                    Err(_) => {}
                }

                if last_emit.elapsed().as_millis() >= 50 && !pending.is_empty() {
                    // Merge: use last snapshot, but average spectrum
                    let last = pending.last().unwrap().clone();
                    let spectrum = if pending.iter().any(|p| !p.spectrum.is_empty()) {
                        let non_empty: Vec<&Vec<f32>> = pending.iter()
                            .filter(|p| !p.spectrum.is_empty())
                            .map(|p| &p.spectrum)
                            .collect();
                        if non_empty.is_empty() {
                            last.spectrum.clone()
                        } else {
                            let n = non_empty[0].len();
                            (0..n).map(|k| {
                                non_empty.iter().map(|s| s[k]).sum::<f32>() / non_empty.len() as f32
                            }).collect()
                        }
                    } else {
                        last.spectrum.clone()
                    };

                    let payload = MetersPayload { spectrum, ..last };
                    let _ = app.emit("audio-meters", &payload);
                    pending.clear();
                    last_emit = std::time::Instant::now();
                }
            }
        });

        Ok(AudioEngine {
            _in_stream: in_stream,
            _out_stream: out_stream,
            params,
            sample_rate: sr,
            latency_ms,
            device_name,
        })
    }
}

/// List all input or output audio devices.
pub fn list_devices(input: bool) -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let default_name = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }.and_then(|d| d.name().ok()).unwrap_or_default();

    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    };

    devices.map(|iter| {
        iter.filter_map(|d| d.name().ok())
            .map(|name| AudioDeviceInfo {
                is_default: name == default_name,
                name,
            })
            .collect()
    }).unwrap_or_default()
}

fn pick_device(
    mut devices: impl Iterator<Item = Device>,
    name: &Option<String>,
    default: impl FnOnce() -> Option<Device>,
    kind: &str,
) -> Result<Device, String> {
    if let Some(n) = name {
        devices.find(|d| d.name().map(|dn| &dn == n).unwrap_or(false))
               .ok_or_else(|| format!("{kind} device '{n}' not found"))
    } else {
        default().ok_or_else(|| format!("No default {kind} device"))
    }
}

#[inline] fn db_to_lin(db: f32) -> f32 { 10f32.powf(db / 20.0) }
