/// CPAL audio engine that owns the native input/output streams.
/// Every input channel feeds its own mixer strip; the voice chain runs on the
/// `voice_chain` strip; the Main, Stream and Monitor buses go to the main,
/// broadcast and monitor devices (see `input_proc` and `routing`).
/// The streams stay on dedicated audio callbacks; Tauri only stores a Send-safe
/// controller that can stop the engine, report status, and request noise
/// profile capture from recent input audio.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use parking_lot::Mutex;
use ringbuf::traits::{Consumer, Split};
use std::cmp::{max, min};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_STREAM};
use crate::dsp::noise::compute_noise_profile;
use crate::dsp::{AudioDeviceInfo, DspChain, DspParams, MetersPayload};
use crate::history::HistoryRing;
use crate::mixer_control::MixerLink;
use crate::mixer_layout::StripLayout;
use crate::mixer_meters::MixerMeterSlots;
use crate::ndi::receive::receiver::NdiReceiver;
use crate::ndi::receive::{NdiInputHandle, NdiInputs};
use crate::ndi::sender::NdiBusSender;
use crate::ndi::{NdiEngineConfig, NdiOutputs};

mod input_proc;
use input_proc::{InputProcessor, OutputRoute};

/// Largest number of frames the input callback processes in one pass. Longer
/// device buffers are split into chunks of this size so the preallocated
/// per-channel and mixer buffers are never outgrown.
const MAX_CALLBACK_FRAMES: usize = 4096;

/// `mixer-meters` emit period (the meters thread wakes at least every
/// 20 ms, so the event arrives at roughly 18–20 Hz).
const MIXER_METERS_INTERVAL_MS: u64 = 50;

/// Which devices to open. Ids are the names/ids from `list_devices`;
/// `None` means the default device (monitor, input) or "not opened"
/// (broadcast, main).
pub struct DeviceSelection {
    pub input_id: Option<String>,
    pub monitor_output_id: Option<String>,
    pub broadcast_output_id: Option<String>,
    pub main_output_id: Option<String>,
}

/// Minimum history needed to compute a noise profile (one FFT frame).
const MIN_PROFILE_SAMPLES: usize = 1024;

/// How long `capture_noise_profile` waits for live audio / the callback.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct AudioEngine {
    _in_stream: cpal::Stream,
    _out_streams: Vec<cpal::Stream>,
    /// NDI senders. Declared after the streams so they stop after the input
    /// callback (the ring producers' owner) is gone.
    ndi_senders: Vec<NdiBusSender>,
    ndi_applied: NdiOutputs,
    /// NDI receivers; they stop after the input callback too.
    _ndi_receivers: Vec<NdiReceiver>,
    ndi_inputs: Vec<NdiInputHandle>,
    ndi_inputs_applied: NdiInputs,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub latency_ms: f32,
    pub input_device_name: String,
    pub monitor_output_name: String,
    pub broadcast_output_name: Option<String>,
    /// Main (PA) output, when one was selected and opened.
    pub main_output_name: Option<String>,
    /// RNNoise stage usable (engine at 48 kHz).
    pub neural_available: bool,
    /// Input channels of the open input device (one mixer strip each).
    pub input_channels: u32,
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
    /// Main (PA) output, when one was selected and opened.
    pub main_output_name: Option<String>,
    /// RNNoise stage usable (engine at 48 kHz).
    pub neural_available: bool,
    /// Input channels of the open input device (one mixer strip each).
    pub input_channels: u32,
    /// NDI source names that are sending.
    pub ndi_sending: Vec<String>,
    /// The NDI settings this engine was started with.
    pub ndi_applied: NdiOutputs,
    /// NDI inputs that are receiving, each feeding a strip after the
    /// hardware ones.
    pub ndi_inputs: Vec<NdiInputHandle>,
    /// The NDI input settings this engine was started with.
    pub ndi_inputs_applied: NdiInputs,
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
    main_output_name: Option<String>,
    neural_available: bool,
    input_channels: u32,
    ndi_sending: Vec<String>,
    ndi_applied: NdiOutputs,
    ndi_inputs: Vec<NdiInputHandle>,
    ndi_inputs_applied: NdiInputs,
}

/// State shared between the audio callback and the control (Tauri) thread.
///
/// The callback never blocks on these mutexes: it only uses `try_lock` and
/// skips the work when the control thread happens to hold the lock.
struct EngineShared {
    /// Recent raw input (pre-DSP) of the voice-chain strip, for
    /// noise-profile capture.
    history: Mutex<HistoryRing>,
    /// Noise power spectrum computed by the control thread, waiting to be
    /// copied into the DSP chain by the callback. The Vec is never dropped or
    /// resized on the audio thread.
    profile_slot: Mutex<Vec<f32>>,
    profile_pending: AtomicBool,
    /// Incremented by the callback every time it consumes `profile_slot`.
    profile_applied: AtomicU64,
    capture_busy: AtomicBool,
    noise_profile_ready: AtomicBool,
    /// Current processing latency of the DSP chain (samples), published by
    /// the callback after every block.
    dsp_latency_samples: AtomicU64,
    /// Input-to-output processing latency (samples) of what is active now:
    /// the slowest strip (voice chain on its strip, plus the gate lookahead
    /// of any strip whose gate is on) plus the bus limiter. Published by the
    /// callback after every block.
    path_latency_samples: AtomicU64,
    dropped_output_samples: AtomicU64,
    /// Samples the NDI rings could not take (callback) or skipped to stay
    /// near real time (sender threads).
    ndi_dropped_samples: Arc<AtomicU64>,
    callback_count: AtomicU64,
    callback_total_ns: AtomicU64,
    callback_peak_ns: AtomicU64,
}

impl EngineShared {
    fn new(recent_capacity: usize) -> Self {
        Self {
            history: Mutex::new(HistoryRing::new(recent_capacity)),
            profile_slot: Mutex::new(Vec::new()),
            profile_pending: AtomicBool::new(false),
            profile_applied: AtomicU64::new(0),
            capture_busy: AtomicBool::new(false),
            dsp_latency_samples: AtomicU64::new(0),
            path_latency_samples: AtomicU64::new(0),
            noise_profile_ready: AtomicBool::new(false),
            dropped_output_samples: AtomicU64::new(0),
            ndi_dropped_samples: Arc::new(AtomicU64::new(0)),
            callback_count: AtomicU64::new(0),
            callback_total_ns: AtomicU64::new(0),
            callback_peak_ns: AtomicU64::new(0),
        }
    }

    /// Audio thread: append input to the history. Skips (never blocks) if the
    /// control thread is taking a snapshot at this moment.
    fn push_recent_input(&self, samples: &[f32]) {
        if let Some(mut history) = self.history.try_lock() {
            history.push(samples);
        }
    }

    /// Audio thread: copy a pending noise profile into the DSP chain.
    fn poll_noise_profile(&self, dsp: &mut DspChain) {
        if !self.profile_pending.load(Ordering::Acquire) {
            return;
        }
        let Some(slot) = self.profile_slot.try_lock() else {
            return; // control thread is writing; try again next callback
        };
        if dsp.set_noise_profile(&slot[..]) {
            self.noise_profile_ready.store(true, Ordering::Release);
        }
        self.profile_pending.store(false, Ordering::Release);
        self.profile_applied.fetch_add(1, Ordering::AcqRel);
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
            main_output_name: self.main_output_name.clone(),
            neural_available: self.neural_available,
            input_channels: self.input_channels,
            ndi_sending: self
                .ndi_senders
                .iter()
                .map(|sender| sender.name().to_string())
                .collect(),
            ndi_applied: self.ndi_applied.clone(),
            ndi_inputs: self.ndi_inputs.clone(),
            ndi_inputs_applied: self.ndi_inputs_applied.clone(),
        }
    }

    fn start(
        app: AppHandle,
        params: Arc<DspParams>,
        shared: Arc<EngineShared>,
        mixer: MixerLink,
        devices: DeviceSelection,
        ndi: NdiEngineConfig,
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
            &devices.input_id,
            || host.default_input_device(),
            "input",
        )?;
        let monitor_out_dev = pick_device(
            output_devices,
            &devices.monitor_output_id,
            || host.default_output_device(),
            "monitor output",
        )?;
        let monitor_output_name = monitor_out_dev
            .name()
            .unwrap_or_else(|_| "Unknown output".to_string());

        // Each device is opened once, for the first bus that claims it, in
        // the order Monitor → Stream (broadcast) → Main. A broadcast device
        // equal to the monitor is skipped (as before); a main device equal to
        // the monitor or the broadcast device is skipped too.
        let broadcast = pick_extra_output(
            &host,
            &devices.broadcast_output_id,
            "broadcast output",
            &[devices.monitor_output_id.as_ref()],
            &[monitor_output_name.clone()],
        )?;
        let broadcast_output_name = broadcast.as_ref().map(|(_, name)| name.clone());
        let mut taken_names = vec![monitor_output_name.clone()];
        if let Some(name) = broadcast_output_name.as_ref() {
            taken_names.push(name.clone());
        }
        let main = pick_extra_output(
            &host,
            &devices.main_output_id,
            "main output",
            &[
                devices.monitor_output_id.as_ref(),
                devices.broadcast_output_id.as_ref(),
            ],
            &taken_names,
        )?;
        let main_output_name = main.as_ref().map(|(_, name)| name.clone());

        let mut output_devices = vec![monitor_out_dev];
        let mut output_buses = vec![BUS_MONITOR];
        if let Some((device, _)) = broadcast {
            output_devices.push(device);
            output_buses.push(BUS_STREAM);
        }
        if let Some((device, _)) = main {
            output_devices.push(device);
            output_buses.push(BUS_MAIN);
        }

        let (in_cfg, out_cfgs) = negotiate_stream_configs(&in_dev, &output_devices)?;

        let sr = in_cfg.sample_rate().0;
        let out_sr = out_cfgs[0].sample_rate().0;
        let in_channels = (in_cfg.channels() as usize).max(1);
        let input_device_name = in_dev
            .name()
            .unwrap_or_else(|_| "Unknown input".to_string());

        let buf_frames = 256u32;
        let latency_ms = (buf_frames as f32 * 2.0 / out_sr as f32) * 1000.0;

        let max_out_channels = out_cfgs
            .iter()
            .map(|cfg| cfg.channels() as usize)
            .max()
            .unwrap_or(2);
        let ring_cap = (sr as usize / 10).max(buf_frames as usize * max_out_channels * 4);
        let mut routes = Vec::with_capacity(output_devices.len());
        let mut ring_consumers = Vec::with_capacity(output_devices.len());
        for (cfg, &bus) in out_cfgs.iter().zip(output_buses.iter()) {
            let (producer, consumer) = ringbuf::HeapRb::<f32>::new(ring_cap).split();
            routes.push(OutputRoute {
                producer,
                channels: (cfg.channels() as usize).max(1),
                bus,
            });
            ring_consumers.push(consumer);
        }

        // NDI: one ring + sender thread per enabled bus (never called from
        // the callback). A missing runtime only skips NDI.
        let (ndi_producers, ndi_senders) = crate::ndi::start_senders(
            &ndi.outputs,
            sr,
            &shared.ndi_dropped_samples,
            |capacity| ringbuf::HeapRb::<f32>::new(capacity).split(),
        );
        let ndi_routes = ndi_producers
            .into_iter()
            .map(|(bus, producer)| OutputRoute {
                producer,
                channels: 2,
                bus,
            })
            .collect();

        // NDI inputs: one receiver thread + ring per source, each read by
        // the callback as a strip after the hardware channels.
        let hardware_strips = StripLayout::new(in_channels, 0).hardware;
        let ndi_in = crate::ndi::receive::start_receivers(
            &ndi.inputs,
            sr,
            hardware_strips,
            StripLayout::ndi_room(in_channels),
        );
        let layout = StripLayout::new(in_channels, ndi_in.feeds.len());

        let (meters_tx, meters_rx) = mpsc::sync_channel::<MetersPayload>(32);
        let mut dsp = DspChain::new(sr as f64);
        dsp.sync_params(&params);
        shared
            .dsp_latency_samples
            .store(dsp.total_latency_samples() as u64, Ordering::Relaxed);
        let neural_available = dsp.neural_available();

        let in_stream_cfg = StreamConfig {
            channels: in_cfg.channels(),
            sample_rate: in_cfg.sample_rate(),
            buffer_size: BufferSize::Fixed(buf_frames),
        };

        let meter_slots = Arc::new(MixerMeterSlots::new(layout.total()));
        let mut processor = InputProcessor::new(
            sr,
            in_channels,
            dsp,
            params,
            shared.clone(),
            mixer,
            meter_slots.clone(),
            routes,
            ndi_routes,
            ndi_in.feeds,
            meters_tx,
        );

        let in_stream = match in_cfg.sample_format() {
            SampleFormat::F32 => in_dev
                .build_input_stream(
                    &in_stream_cfg,
                    move |data: &[f32], _| processor.process(data, |sample| sample),
                    |e| log::error!("Input stream error: {e}"),
                    None,
                )
                .map_err(|e| e.to_string())?,
            SampleFormat::I16 => in_dev
                .build_input_stream(
                    &in_stream_cfg,
                    move |data: &[i16], _| processor.process(data, i16_to_f32),
                    |e| log::error!("Input stream error: {e}"),
                    None,
                )
                .map_err(|e| e.to_string())?,
            SampleFormat::U16 => in_dev
                .build_input_stream(
                    &in_stream_cfg,
                    move |data: &[u16], _| processor.process(data, u16_to_f32),
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

        spawn_meters_thread(app, shared, meters_rx, meter_slots);

        Ok(Self {
            _in_stream: in_stream,
            _out_streams: out_streams,
            ndi_senders,
            ndi_applied: ndi.outputs,
            _ndi_receivers: ndi_in.receivers,
            ndi_inputs: ndi_in.handles,
            ndi_inputs_applied: ndi.inputs,
            sample_rate: sr,
            buffer_frames: buf_frames,
            latency_ms,
            input_device_name,
            monitor_output_name,
            broadcast_output_name,
            main_output_name,
            neural_available,
            input_channels: in_channels as u32,
        })
    }
}

/// Meters thread: logs dropped output samples, emits `audio-meters`
/// (~20 Hz, spectrum averaged over the pending payloads) and `mixer-meters`
/// (~20 Hz, read from the lock-free slots). Exits when the input callback
/// (the only sender) is dropped with the engine.
fn spawn_meters_thread(
    app: AppHandle,
    meters_shared: Arc<EngineShared>,
    meters_rx: mpsc::Receiver<MetersPayload>,
    meter_slots: Arc<MixerMeterSlots>,
) {
    std::thread::spawn(move || {
        let mut pending: Vec<MetersPayload> = Vec::with_capacity(8);
        let mut last_emit = Instant::now();
        let mut last_mixer_emit = Instant::now();
        let mut last_logged_drops = 0u64;

        loop {
            let drops = meters_shared.dropped_output_samples.load(Ordering::Relaxed);
            if drops != last_logged_drops {
                log::warn!(
                    "Dropping output samples because the playback ring buffer is full (total dropped: {drops})"
                );
                last_logged_drops = drops;
            }

            match meters_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(meters) => pending.push(meters),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if last_mixer_emit.elapsed() >= Duration::from_millis(MIXER_METERS_INTERVAL_MS) {
                let _ = app.emit("mixer-meters", &meter_slots.take_payload());
                last_mixer_emit = Instant::now();
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
                    neural_vad: 0.0,
                    spectrum: vec![],
                });

                let spectrum = average_spectrum(&pending).unwrap_or_else(|| last.spectrum.clone());
                let payload = MetersPayload { spectrum, ..last };
                let _ = app.emit("audio-meters", &payload);
                pending.clear();
                last_emit = Instant::now();
            }
        }
    });
}

/// Mean of the non-empty spectra in `pending` (None if there are none).
fn average_spectrum(pending: &[MetersPayload]) -> Option<Vec<f32>> {
    let non_empty: Vec<&Vec<f32>> = pending
        .iter()
        .filter(|p| !p.spectrum.is_empty())
        .map(|p| &p.spectrum)
        .collect();
    let first = non_empty.first()?;
    let n = first.len();
    Some(
        (0..n)
            .map(|k| {
                non_empty.iter().map(|s| s.get(k).copied().unwrap_or(0.0)).sum::<f32>()
                    / non_empty.len() as f32
            })
            .collect(),
    )
}

/// Resolve an optional extra output device. Returns `None` when nothing is
/// selected or when the selection (by id or by resolved name) is a device
/// already opened for another bus.
fn pick_extra_output(
    host: &cpal::Host,
    selection: &Option<String>,
    kind: &str,
    taken_ids: &[Option<&String>],
    taken_names: &[String],
) -> Result<Option<(Device, String)>, String> {
    let Some(selected) = selection.as_ref() else {
        return Ok(None);
    };
    if taken_ids.contains(&Some(selected)) {
        return Ok(None);
    }
    let devices: Vec<Device> = host
        .output_devices()
        .map_err(|e| e.to_string())?
        .collect();
    let device = pick_device(devices, selection, || host.default_output_device(), kind)?;
    let Ok(name) = device.name() else {
        return Ok(Some((device, format!("Unknown {kind}"))));
    };
    if taken_names.iter().any(|taken| *taken == name) {
        return Ok(None);
    }
    Ok(Some((device, name)))
}

impl RunningEngine {
    pub fn spawn(
        app: AppHandle,
        params: Arc<DspParams>,
        mixer: MixerLink,
        devices: DeviceSelection,
        ndi: NdiEngineConfig,
    ) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<EngineInfo, String>>(1);
        let (stop_tx, stop_rx) = mpsc::channel::<EngineCommand>();
        let shared = Arc::new(EngineShared::new(48_000 * 2));
        let thread_shared = shared.clone();

        let join_handle = std::thread::spawn(move || match AudioEngine::start(
            app,
            params,
            thread_shared,
            mixer,
            devices,
            ndi,
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
            main_output_name: info.main_output_name,
            neural_available: info.neural_available,
            input_channels: info.input_channels,
            ndi_sending: info.ndi_sending,
            ndi_applied: info.ndi_applied,
            ndi_inputs: info.ndi_inputs,
            ndi_inputs_applied: info.ndi_inputs_applied,
        })
    }

    pub fn stop(mut self) {
        let _ = self.stop_tx.send(EngineCommand::Stop);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }

    /// Capture a noise profile from the most recent input audio.
    ///
    /// Runs on the calling (control) thread: it snapshots the input history,
    /// computes the noise spectrum here, then hands it to the audio callback
    /// through `profile_slot` and waits for the callback to install it. The
    /// whole operation times out after 2 s, as before.
    pub fn capture_noise_profile(&self) -> Result<(), String> {
        if self.shared.capture_busy.swap(true, Ordering::AcqRel) {
            return Err("Noise profile capture is already in progress".to_string());
        }
        let result = self.capture_noise_profile_inner();
        self.shared.capture_busy.store(false, Ordering::Release);
        result
    }

    fn capture_noise_profile_inner(&self) -> Result<(), String> {
        let deadline = Instant::now() + CAPTURE_TIMEOUT;
        let callbacks_at_start = self.shared.callback_count.load(Ordering::Relaxed);

        // 1. Wait (briefly) until enough live audio has been recorded.
        let snapshot = loop {
            let history = self.shared.history.lock();
            if history.len() >= MIN_PROFILE_SAMPLES {
                let samples = history.snapshot();
                drop(history);
                break samples;
            }
            drop(history);
            if Instant::now() >= deadline {
                let callbacks_now = self.shared.callback_count.load(Ordering::Relaxed);
                let message = if callbacks_now == callbacks_at_start {
                    "Timed out while waiting for live audio to capture the noise profile"
                } else {
                    "Need at least a short burst of live audio before capturing a noise profile"
                };
                return Err(message.to_string());
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        // 2. Heavy lifting off the audio thread.
        let profile = compute_noise_profile(&snapshot).ok_or_else(|| {
            "Need at least a short burst of live audio before capturing a noise profile"
                .to_string()
        })?;

        // 3. Hand over to the callback and wait for it to be installed.
        let applied_before = self.shared.profile_applied.load(Ordering::Acquire);
        {
            let mut slot = self.shared.profile_slot.lock();
            *slot = profile;
        }
        self.shared.profile_pending.store(true, Ordering::Release);

        loop {
            if self.shared.profile_applied.load(Ordering::Acquire) != applied_before {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.shared.profile_pending.store(false, Ordering::Release);
                return Err(
                    "Timed out while waiting for live audio to capture the noise profile"
                        .to_string(),
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Current voice-chain latency in samples (0 when bypassed or when the
    /// voice chain is on no active strip).
    pub fn dsp_latency_samples(&self) -> u64 {
        self.shared.dsp_latency_samples.load(Ordering::Relaxed)
    }

    /// Buffer latency plus the processing latency of what is really active
    /// (voice chain, gated strips, bus limiter), in ms.
    pub fn total_latency_ms(&self) -> f32 {
        let path = self.shared.path_latency_samples.load(Ordering::Relaxed);
        let path_ms = if self.sample_rate > 0 {
            path as f32 * 1000.0 / self.sample_rate as f32
        } else {
            0.0
        };
        self.latency_ms + path_ms
    }

    pub fn noise_profile_ready(&self) -> bool {
        self.shared.noise_profile_ready.load(Ordering::Acquire)
    }

    pub fn dropped_output_samples(&self) -> u64 {
        self.shared.dropped_output_samples.load(Ordering::Relaxed)
    }

    /// Mixer strips: input channels, then NDI inputs.
    pub fn strip_layout(&self) -> StripLayout {
        StripLayout::new(self.input_channels as usize, self.ndi_inputs.len())
    }

    pub fn ndi_dropped_samples(&self) -> u64 {
        self.shared.ndi_dropped_samples.load(Ordering::Relaxed)
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
        "No common sample rate is available between the selected input and output (monitor, broadcast, main) devices"
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
