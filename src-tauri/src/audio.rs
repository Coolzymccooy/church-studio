/// CPAL audio engine that owns the native input/output streams.
/// The streams stay on dedicated audio callbacks; Tauri only stores a Send-safe
/// controller that can stop the engine, report status, and request noise
/// profile capture from recent input audio.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use parking_lot::Mutex;
use ringbuf::traits::{Consumer, Producer, Split};
use rustfft::{num_complex::Complex32, FftPlanner};
use std::cmp::{max, min};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::dsp::{AudioDeviceInfo, DspChain, DspParams, MetersPayload};

pub struct AudioEngine {
    _in_stream: cpal::Stream,
    _out_streams: Vec<cpal::Stream>,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub latency_ms: f32,
    pub input_device_name: String,
    pub monitor_output_name: String,
    pub broadcast_output_name: Option<String>,
}

pub type EngineState = std::sync::Mutex<Option<RunningEngine>>;

pub struct RunningEngine {
    stop_tx: mpsc::Sender<EngineCommand>,
    join_handle: Option<JoinHandle<()>>,
    shared: Arc<EngineShared>,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub latency_ms: f32,
    pub input_device_name: String,
    pub monitor_output_name: String,
    pub broadcast_output_name: Option<String>,
}

enum EngineCommand {
    Stop,
}

#[derive(Clone)]
struct EngineInfo {
    sample_rate: u32,
    buffer_frames: u32,
    latency_ms: f32,
    input_device_name: String,
    monitor_output_name: String,
    broadcast_output_name: Option<String>,
}

struct EngineShared {
    recent_input: Mutex<Vec<f32>>,
    recent_capacity: usize,
    capture_requested: AtomicBool,
    capture_reply: Mutex<Option<mpsc::SyncSender<Result<(), String>>>>,
    noise_profile_ready: AtomicBool,
    dropped_output_samples: AtomicU64,
    callback_count: AtomicU64,
    callback_total_ns: AtomicU64,
    callback_peak_ns: AtomicU64,
}

impl EngineShared {
    fn new(recent_capacity: usize) -> Self {
        Self {
            recent_input: Mutex::new(Vec::with_capacity(recent_capacity)),
            recent_capacity,
            capture_requested: AtomicBool::new(false),
            capture_reply: Mutex::new(None),
            noise_profile_ready: AtomicBool::new(false),
            dropped_output_samples: AtomicU64::new(0),
            callback_count: AtomicU64::new(0),
            callback_total_ns: AtomicU64::new(0),
            callback_peak_ns: AtomicU64::new(0),
        }
    }

    fn push_recent_input(&self, samples: &[f32]) {
        let mut recent = self.recent_input.lock();
        let overflow = recent
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.recent_capacity);

        if overflow > 0 {
            let drain_len = overflow.min(recent.len());
            recent.drain(0..drain_len);
        }

        recent.extend_from_slice(samples);
    }

    fn capture_recent_snapshot(&self) -> Vec<f32> {
        self.recent_input.lock().clone()
    }

    fn record_callback_time(&self, elapsed: Duration) {
        let elapsed_ns = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        self.callback_count.fetch_add(1, Ordering::Relaxed);
        self.callback_total_ns
            .fetch_add(elapsed_ns, Ordering::Relaxed);
        self.callback_peak_ns
            .fetch_max(elapsed_ns, Ordering::Relaxed);
    }
}

impl AudioEngine {
    fn info(&self) -> EngineInfo {
        EngineInfo {
            sample_rate: self.sample_rate,
            buffer_frames: self.buffer_frames,
            latency_ms: self.latency_ms,
            input_device_name: self.input_device_name.clone(),
            monitor_output_name: self.monitor_output_name.clone(),
            broadcast_output_name: self.broadcast_output_name.clone(),
        }
    }

    fn start(
        app: AppHandle,
        params: Arc<DspParams>,
        shared: Arc<EngineShared>,
        input_id: Option<String>,
        monitor_output_id: Option<String>,
        broadcast_output_id: Option<String>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();

        let input_devices: Vec<Device> = host
            .input_devices()
            .map_err(|e| e.to_string())?
            .collect();
        let output_devices: Vec<Device> = host
            .output_devices()
            .map_err(|e| e.to_string())?
            .collect();

        let in_dev = pick_device(
            input_devices,
            &input_id,
            || host.default_input_device(),
            "input",
        )?;
        let monitor_out_dev = pick_device(
            output_devices,
            &monitor_output_id,
            || host.default_output_device(),
            "monitor output",
        )?;
        let monitor_output_name = monitor_out_dev
            .name()
            .unwrap_or_else(|_| "Unknown output".to_string());

        let broadcast_out_dev = if let Some(selection) = broadcast_output_id.as_ref() {
            if monitor_output_id.as_ref() == Some(selection) {
                None
            } else {
                let device = pick_device(
                    host.output_devices()
                        .map_err(|e| e.to_string())?
                        .collect(),
                    &broadcast_output_id,
                    || host.default_output_device(),
                    "broadcast output",
                )?;
                let same_device = device
                    .name()
                    .map(|name| name == monitor_output_name)
                    .unwrap_or(false);
                if same_device {
                    None
                } else {
                    Some(device)
                }
            }
        } else {
            None
        };

        let mut output_devices = vec![monitor_out_dev];
        if let Some(device) = broadcast_out_dev {
            output_devices.push(device);
        }

        let (in_cfg, out_cfgs) = negotiate_stream_configs(&in_dev, &output_devices)?;

        let sr = in_cfg.sample_rate().0;
        let out_sr = out_cfgs[0].sample_rate().0;
        let in_channels = in_cfg.channels() as usize;
        let input_device_name = in_dev
            .name()
            .unwrap_or_else(|_| "Unknown input".to_string());
        let broadcast_output_name = output_devices
            .get(1)
            .and_then(|device| device.name().ok());

        let buf_frames = 256u32;
        let latency_ms = (buf_frames as f32 * 2.0 / out_sr as f32) * 1000.0;

        let max_out_channels = out_cfgs
            .iter()
            .map(|cfg| cfg.channels() as usize)
            .max()
            .unwrap_or(2);
        let ring_cap = (sr as usize / 10).max(buf_frames as usize * max_out_channels * 4);
        let mut ring_producers = Vec::with_capacity(output_devices.len());
        let mut ring_consumers = Vec::with_capacity(output_devices.len());
        for _ in 0..output_devices.len() {
            let (producer, consumer) = ringbuf::HeapRb::<f32>::new(ring_cap).split();
            ring_producers.push(producer);
            ring_consumers.push(consumer);
        }

        let (meters_tx, meters_rx) = mpsc::sync_channel::<MetersPayload>(32);
        let mut dsp = DspChain::new(sr as f64);
        let shared_cb = shared.clone();

        let mut spec_planner = FftPlanner::<f32>::new();
        let spec_fft = spec_planner.plan_fft_forward(256);
        let spec_win: Vec<f32> = (0..256)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / 255.0).cos()))
            .collect();
        let mut spec_buf = vec![Complex32::default(); 256];
        let mut spec_acc = vec![0.0f32; 128];
        let mut spec_count = 0usize;

        let params_cb = params.clone();

        let in_stream_cfg = StreamConfig {
            channels: in_cfg.channels(),
            sample_rate: in_cfg.sample_rate(),
            buffer_size: BufferSize::Fixed(buf_frames),
        };
        let output_stream_cfgs: Vec<StreamConfig> = out_cfgs
            .iter()
            .map(|cfg| StreamConfig {
                channels: cfg.channels(),
                sample_rate: cfg.sample_rate(),
                buffer_size: BufferSize::Fixed(buf_frames),
            })
            .collect();

        let mut on_input = move |mono: Vec<f32>| {
            let callback_started = Instant::now();

            shared_cb.push_recent_input(&mono);

            if shared_cb.capture_requested.swap(false, Ordering::AcqRel) {
                let snapshot = shared_cb.capture_recent_snapshot();
                let result = if snapshot.len() < 1024 {
                    Err("Need at least a short burst of live audio before capturing a noise profile".to_string())
                } else {
                    dsp.capture_noise_profile(&snapshot);
                    shared_cb
                        .noise_profile_ready
                        .store(true, Ordering::Release);
                    Ok(())
                };

                if let Some(reply) = shared_cb.capture_reply.lock().take() {
                    let _ = reply.send(result);
                }
            }

            let mut block = mono;

            dsp.sync_params(&params_cb);
            let bypass = params_cb.bypass.load(Ordering::Relaxed);
            dsp.process_block(&mut block, bypass);

            let monitor_gain =
                db_to_lin(params_cb.monitor_gain_db.load(Ordering::Relaxed));
            for sample in &mut block {
                *sample *= monitor_gain;
            }

            for (i, &sample) in block.iter().enumerate().take(256) {
                spec_buf[i] = Complex32::new(sample * spec_win[i], 0.0);
            }
            spec_fft.process(&mut spec_buf);
            for (acc, bin) in spec_acc.iter_mut().zip(spec_buf.iter().take(128)) {
                *acc += bin.norm();
            }
            spec_count += 1;

            let mut dropped_this_callback = 0u64;
            for (producer, cfg) in ring_producers
                .iter_mut()
                .zip(output_stream_cfgs.iter())
            {
                let output_channels = cfg.channels as usize;
                for &sample in &block {
                    for _ in 0..output_channels {
                        if producer.try_push(sample).is_err() {
                            dropped_this_callback += 1;
                        }
                    }
                }
            }

            if dropped_this_callback > 0 {
                let total = shared_cb
                    .dropped_output_samples
                    .fetch_add(dropped_this_callback, Ordering::Relaxed)
                    + dropped_this_callback;
                if total % 2048 <= dropped_this_callback {
                    log::warn!(
                        "Dropping output samples because the playback ring buffer is full (total dropped: {total})"
                    );
                }
            }

            let spectrum = if spec_count >= 4 {
                let bins: Vec<f32> = spec_acc
                    .iter()
                    .map(|&value| {
                        let db =
                            20.0 * (value / spec_count as f32 / 128.0).max(1e-9).log10();
                        ((db + 90.0) / 90.0).clamp(0.0, 1.0)
                    })
                    .collect();
                spec_acc.fill(0.0);
                spec_count = 0;
                bins
            } else {
                vec![]
            };

            let _ = meters_tx.try_send(MetersPayload {
                input_db: dsp.last_input_db,
                output_db: dsp.last_output_db,
                gate_gain: dsp.last_gate_gain,
                lufs_m: dsp.last_lufs.momentary,
                lufs_st: dsp.last_lufs.short_term,
                lufs_i: dsp.last_lufs.integrated,
                deess_gr_db: dsp.last_deess_gr,
                auto_gain_db: dsp.last_auto_gain_db,
                spectrum,
            });

            shared_cb.record_callback_time(callback_started.elapsed());
        };

        let in_stream = match in_cfg.sample_format() {
            SampleFormat::F32 => in_dev
                .build_input_stream(
                    &in_stream_cfg,
                    move |data: &[f32], _| {
                        on_input(interleaved_to_mono_f32(data, in_channels, |sample| sample));
                    },
                    |e| log::error!("Input stream error: {e}"),
                    None,
                )
                .map_err(|e| e.to_string())?,
            SampleFormat::I16 => in_dev
                .build_input_stream(
                    &in_stream_cfg,
                    move |data: &[i16], _| {
                        on_input(interleaved_to_mono_f32(data, in_channels, i16_to_f32));
                    },
                    |e| log::error!("Input stream error: {e}"),
                    None,
                )
                .map_err(|e| e.to_string())?,
            SampleFormat::U16 => in_dev
                .build_input_stream(
                    &in_stream_cfg,
                    move |data: &[u16], _| {
                        on_input(interleaved_to_mono_f32(data, in_channels, u16_to_f32));
                    },
                    |e| log::error!("Input stream error: {e}"),
                    None,
                )
                .map_err(|e| e.to_string())?,
            fmt => return Err(format!("Unsupported input sample format: {fmt:?}")),
        };

        let mut out_streams = Vec::with_capacity(output_devices.len());
        for ((out_dev, out_cfg), mut ring_cons) in output_devices
            .into_iter()
            .zip(out_cfgs.into_iter())
            .zip(ring_consumers.into_iter())
        {
            let out_stream_cfg = StreamConfig {
                channels: out_cfg.channels(),
                sample_rate: out_cfg.sample_rate(),
                buffer_size: BufferSize::Fixed(buf_frames),
            };

            let out_stream = match out_cfg.sample_format() {
                SampleFormat::F32 => out_dev
                    .build_output_stream(
                        &out_stream_cfg,
                        move |data: &mut [f32], _| {
                            write_output_data(data, &mut ring_cons, |sample| sample);
                        },
                        |e| log::error!("Output stream error: {e}"),
                        None,
                    )
                    .map_err(|e| e.to_string())?,
                SampleFormat::I16 => out_dev
                    .build_output_stream(
                        &out_stream_cfg,
                        move |data: &mut [i16], _| {
                            write_output_data(data, &mut ring_cons, f32_to_i16);
                        },
                        |e| log::error!("Output stream error: {e}"),
                        None,
                    )
                    .map_err(|e| e.to_string())?,
                SampleFormat::U16 => out_dev
                    .build_output_stream(
                        &out_stream_cfg,
                        move |data: &mut [u16], _| {
                            write_output_data(data, &mut ring_cons, f32_to_u16);
                        },
                        |e| log::error!("Output stream error: {e}"),
                        None,
                    )
                    .map_err(|e| e.to_string())?,
                fmt => return Err(format!("Unsupported output sample format: {fmt:?}")),
            };

            out_stream.play().map_err(|e| e.to_string())?;
            out_streams.push(out_stream);
        }

        in_stream.play().map_err(|e| e.to_string())?;

        std::thread::spawn(move || {
            let mut pending: Vec<MetersPayload> = Vec::with_capacity(8);
            let mut last_emit = Instant::now();

            loop {
                match meters_rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(meters) => pending.push(meters),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                if last_emit.elapsed().as_millis() >= 50 && !pending.is_empty() {
                    let last = pending.last().cloned().unwrap_or(MetersPayload {
                        input_db: -96.0,
                        output_db: -96.0,
                        gate_gain: 0.0,
                        lufs_m: -70.0,
                        lufs_st: -70.0,
                        lufs_i: -70.0,
                        deess_gr_db: 0.0,
                        auto_gain_db: 0.0,
                        spectrum: vec![],
                    });

                    let spectrum = if pending.iter().any(|p| !p.spectrum.is_empty()) {
                        let non_empty: Vec<&Vec<f32>> = pending
                            .iter()
                            .filter(|p| !p.spectrum.is_empty())
                            .map(|p| &p.spectrum)
                            .collect();

                        if non_empty.is_empty() {
                            last.spectrum.clone()
                        } else {
                            let n = non_empty[0].len();
                            (0..n)
                                .map(|k| {
                                    non_empty.iter().map(|s| s[k]).sum::<f32>()
                                        / non_empty.len() as f32
                                })
                                .collect()
                        }
                    } else {
                        last.spectrum.clone()
                    };

                    let payload = MetersPayload { spectrum, ..last };
                    let _ = app.emit("audio-meters", &payload);
                    pending.clear();
                    last_emit = Instant::now();
                }
            }
        });

        Ok(Self {
            _in_stream: in_stream,
            _out_streams: out_streams,
            sample_rate: sr,
            buffer_frames: buf_frames,
            latency_ms,
            input_device_name,
            monitor_output_name,
            broadcast_output_name,
        })
    }
}

impl RunningEngine {
    pub fn spawn(
        app: AppHandle,
        params: Arc<DspParams>,
        input_id: Option<String>,
        monitor_output_id: Option<String>,
        broadcast_output_id: Option<String>,
    ) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<EngineInfo, String>>(1);
        let (stop_tx, stop_rx) = mpsc::channel::<EngineCommand>();
        let shared = Arc::new(EngineShared::new(48_000 * 2));
        let thread_shared = shared.clone();

        let join_handle = std::thread::spawn(move || match AudioEngine::start(
            app,
            params,
            thread_shared,
            input_id,
            monitor_output_id,
            broadcast_output_id,
        ) {
            Ok(engine) => {
                let info = engine.info();
                if ready_tx.send(Ok(info)).is_err() {
                    return;
                }

                let _engine = engine;
                while let Ok(command) = stop_rx.recv() {
                    match command {
                        EngineCommand::Stop => break,
                    }
                }
            }
            Err(err) => {
                let _ = ready_tx.send(Err(err));
            }
        });

        let info = ready_rx
            .recv()
            .map_err(|_| "Audio thread exited before initialization completed".to_string())??;

        Ok(Self {
            stop_tx,
            join_handle: Some(join_handle),
            shared,
            sample_rate: info.sample_rate,
            buffer_frames: info.buffer_frames,
            latency_ms: info.latency_ms,
            input_device_name: info.input_device_name,
            monitor_output_name: info.monitor_output_name,
            broadcast_output_name: info.broadcast_output_name,
        })
    }

    pub fn stop(mut self) {
        let _ = self.stop_tx.send(EngineCommand::Stop);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }

    pub fn capture_noise_profile(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        {
            let mut slot = self.shared.capture_reply.lock();
            if slot.is_some() {
                return Err("Noise profile capture is already in progress".to_string());
            }
            *slot = Some(reply_tx);
        }

        self.shared.capture_requested.store(true, Ordering::Release);

        match reply_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.shared.capture_reply.lock().take();
                Err("Timed out while waiting for live audio to capture the noise profile".to_string())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.shared.capture_reply.lock().take();
                Err("Audio engine stopped before the noise profile capture completed".to_string())
            }
        }
    }

    pub fn noise_profile_ready(&self) -> bool {
        self.shared.noise_profile_ready.load(Ordering::Acquire)
    }

    pub fn dropped_output_samples(&self) -> u64 {
        self.shared.dropped_output_samples.load(Ordering::Relaxed)
    }

    pub fn callback_avg_ms(&self) -> f32 {
        let count = self.shared.callback_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        let total_ns = self.shared.callback_total_ns.load(Ordering::Relaxed);
        (total_ns as f64 / count as f64 / 1_000_000.0) as f32
    }

    pub fn callback_peak_ms(&self) -> f32 {
        self.shared.callback_peak_ns.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }

    pub fn cpu_load_pct(&self) -> f32 {
        let block_ms = self.buffer_frames as f32 / self.sample_rate as f32 * 1000.0;
        if block_ms <= f32::EPSILON {
            return 0.0;
        }
        (self.callback_avg_ms() / block_ms * 100.0).clamp(0.0, 999.0)
    }
}

impl Drop for RunningEngine {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(EngineCommand::Stop);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

pub fn list_devices(input: bool) -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let default_name = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
    .and_then(|d| d.name().ok())
    .unwrap_or_default();

    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    };

    devices
        .map(|iter| {
            iter.enumerate()
                .filter_map(|(index, device)| {
                    device.name().ok().map(|name| AudioDeviceInfo {
                        id: format_device_id(index, &name),
                        is_default: name == default_name,
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn pick_device(
    devices: Vec<Device>,
    selection: &Option<String>,
    default: impl FnOnce() -> Option<Device>,
    kind: &str,
) -> Result<Device, String> {
    if let Some(selection) = selection {
        devices
            .into_iter()
            .enumerate()
            .find_map(|(index, device)| {
                let name = device.name().ok()?;
                let id = format_device_id(index, &name);
                if selection == &id || selection == &name {
                    Some(device)
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("{kind} device '{selection}' not found"))
    } else {
        default().ok_or_else(|| format!("No default {kind} device"))
    }
}

fn negotiate_stream_configs(
    in_dev: &Device,
    out_devs: &[Device],
) -> Result<(SupportedStreamConfig, Vec<SupportedStreamConfig>), String> {
    let in_default = in_dev.default_input_config().map_err(|e| e.to_string())?;
    let out_defaults: Vec<SupportedStreamConfig> = out_devs
        .iter()
        .map(|device| device.default_output_config().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;

    if out_defaults
        .iter()
        .all(|cfg| cfg.sample_rate() == in_default.sample_rate())
    {
        return Ok((in_default, out_defaults));
    }

    let in_ranges: Vec<SupportedStreamConfigRange> = in_dev
        .supported_input_configs()
        .map_err(|e| e.to_string())?
        .collect();
    let out_ranges_sets: Vec<Vec<SupportedStreamConfigRange>> = out_devs
        .iter()
        .map(|device| {
            device
                .supported_output_configs()
                .map_err(|e| e.to_string())
                .map(|iter| iter.collect::<Vec<_>>())
        })
        .collect::<Result<_, _>>()?;

    if in_ranges.is_empty() || out_ranges_sets.iter().any(|ranges| ranges.is_empty()) {
        return Err("Selected input and output devices do not expose any supported audio stream configuration".to_string());
    }

    let mut preferred_rates = vec![in_default.sample_rate().0, 48_000, 44_100];
    preferred_rates.extend(out_defaults.iter().map(|cfg| cfg.sample_rate().0));
    preferred_rates.sort_unstable();
    preferred_rates.dedup();

    for rate in preferred_rates {
        let Some(input_cfg) = find_config_for_rate(&in_ranges, rate) else {
            continue;
        };
        let mut output_cfgs = Vec::with_capacity(out_ranges_sets.len());
        let mut all_match = true;
        for ranges in &out_ranges_sets {
            let Some(output_cfg) = find_config_for_rate(ranges, rate) else {
                all_match = false;
                break;
            };
            output_cfgs.push(output_cfg);
        }

        if all_match {
            return Ok((input_cfg, output_cfgs));
        }
    }

    for input_cfg in &in_ranges {
        let min_rate = out_ranges_sets.iter().fold(input_cfg.min_sample_rate().0, |current, ranges| {
            max(
                current,
                ranges
                    .iter()
                    .map(|cfg| cfg.min_sample_rate().0)
                    .min()
                    .unwrap_or(current),
            )
        });
        let max_rate = out_ranges_sets.iter().fold(input_cfg.max_sample_rate().0, |current, ranges| {
            min(
                current,
                ranges
                    .iter()
                    .map(|cfg| cfg.max_sample_rate().0)
                    .max()
                    .unwrap_or(current),
            )
        });

        if min_rate <= max_rate {
            let target_rate = preferred_rate_in_range(min_rate, max_rate);
            let mut output_cfgs = Vec::with_capacity(out_ranges_sets.len());
            let mut all_match = true;
            for ranges in &out_ranges_sets {
                let Some(output_cfg) = find_config_for_rate(ranges, target_rate) else {
                    all_match = false;
                    break;
                };
                output_cfgs.push(output_cfg);
            }

            if all_match {
                return Ok((input_cfg.with_sample_rate(SampleRate(target_rate)), output_cfgs));
            }
        }
    }

    Err(
        "No common sample rate is available between the selected input, monitor output, and broadcast output devices"
            .to_string(),
    )
}

fn find_config_for_rate(
    ranges: &[SupportedStreamConfigRange],
    rate: u32,
) -> Option<SupportedStreamConfig> {
    ranges
        .iter()
        .find(|cfg| rate >= cfg.min_sample_rate().0 && rate <= cfg.max_sample_rate().0)
        .map(|cfg| cfg.with_sample_rate(SampleRate(rate)))
}

fn preferred_rate_in_range(min_rate: u32, max_rate: u32) -> u32 {
    if (min_rate..=max_rate).contains(&48_000) {
        48_000
    } else if (min_rate..=max_rate).contains(&44_100) {
        44_100
    } else {
        max_rate
    }
}

fn format_device_id(index: usize, name: &str) -> String {
    let _ = index;
    name.to_string()
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn interleaved_to_mono_f32<T>(
    data: &[T],
    channels: usize,
    mut convert: impl FnMut(T) -> f32,
) -> Vec<f32>
where
    T: Copy,
{
    if channels <= 1 {
        return data.iter().copied().map(&mut convert).collect();
    }

    data.chunks(channels)
        .map(|frame| {
            frame.iter().copied().map(&mut convert).sum::<f32>() / channels as f32
        })
        .collect()
}

fn write_output_data<T>(
    data: &mut [T],
    consumer: &mut impl Consumer<Item = f32>,
    mut convert: impl FnMut(f32) -> T,
) {
    for sample in data.iter_mut() {
        *sample = convert(consumer.try_pop().unwrap_or(0.0));
    }
}

#[inline]
fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / 32_768.0
}

#[inline]
fn u16_to_f32(sample: u16) -> f32 {
    sample as f32 / 65_535.0 * 2.0 - 1.0
}

#[inline]
fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[inline]
fn f32_to_u16(sample: f32) -> u16 {
    (((sample.clamp(-1.0, 1.0) + 1.0) * 0.5) * u16::MAX as f32).round() as u16
}
