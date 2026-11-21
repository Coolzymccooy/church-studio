/// CPAL audio engine that owns the native input/output streams.
/// Every input channel feeds its own mixer strip; the voice chain runs on the
/// `voice_chain` strip; the Main, Stream, Monitor, Monitor 2 and Monitor 3
/// buses go to the main, broadcast, monitor, monitor 2 and monitor 3 devices
/// (see `input_proc` and `routing`).
/// The streams stay on dedicated audio callbacks; Tauri only stores a Send-safe
/// controller that can stop the engine, report status, and request noise
/// profile capture from recent input audio.
use cpal::traits::StreamTrait;
use parking_lot::Mutex;
use ringbuf::traits::Split;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Runtime};

use crate::computer_audio::{self, ComputerAudioCapture, ComputerAudioHandle, OwnOutputs};
use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, BUS_STREAM, NUM_BUSES};
use crate::dsp::noise::compute_noise_profile;
use crate::dsp::{DspChain, DspParams, MetersPayload};
use crate::history::HistoryRing;
use crate::mixer_control::{BusOutputs, MixerLink};
use crate::soundcheck::{playback_channel, PlaybackPort};
use crate::mixer_layout::StripLayout;
use crate::mixer_meters::MixerMeterSlots;
use crate::ndi::receive::receiver::NdiReceivers;
use crate::ndi::receive::{NdiInputHandle, NdiInputs};
use crate::ndi::sender::NdiBusSender;
use crate::ndi::{NdiEngineConfig, NdiOutputs};
use crate::netaudio::{NetAudioConfig, NetAudioInfo, NetAudioThreads};
use crate::recorder::engine::EngineRecorder;
use crate::recorder::RecordPort;

mod config;
mod device_names;
pub mod device_watch;
mod health;
mod input_proc;
mod links;
mod meters_thread;
mod open;
mod output_feed;
mod output_plan;
mod streams;
#[cfg(test)]
mod net_wiring_tests;
#[cfg(test)]
mod device_names_tests;
#[cfg(test)]
mod output_plan_tests;
#[cfg(test)]
mod output_feed_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod usb_tests;
#[cfg(test)]
mod usb_probe;
#[cfg(test)]
mod hotplug_scenario_tests;
pub use device_names::list_devices;
use health::LinkState;
use input_proc::{DeviceRoute, HardwareInput, InputProcessor, OutputRoute};
use links::{EngineLinks, InputLink, OutputLink};
use output_feed::{ClockMode, OutputFeed};
use streams::{SharedFeed, SharedProcessor};

/// Largest number of frames the input callback processes in one pass. Longer
/// device buffers are split into chunks of this size so the preallocated
/// per-channel and mixer buffers are never outgrown.
const MAX_CALLBACK_FRAMES: usize = 4096;

/// `mixer-meters` emit period (the meters thread wakes at least every
/// 20 ms, so the event arrives at roughly 18–20 Hz).
const MIXER_METERS_INTERVAL_MS: u64 = 50;

/// Which devices to open. Ids are the names/ids from `list_devices`;
/// `None` means the default device (monitor, input) or "not opened"
/// (broadcast, main, monitor 2, monitor 3).
pub struct DeviceSelection {
    pub input_id: Option<String>,
    pub monitor_output_id: Option<String>,
    pub broadcast_output_id: Option<String>,
    pub main_output_id: Option<String>,
    /// Mon 2 / mon 3. Unlike broadcast and main, a device another output
    /// already uses is refused with an error, not skipped.
    pub monitor2_output_id: Option<String>,
    pub monitor3_output_id: Option<String>,
    /// Output pair per bus (`BUS_*` order): 0 = 1-2, 1 = 3-4, …
    pub output_pairs: [usize; NUM_BUSES],
    /// Input channels that become strips: (first, count), 0-based; `None`
    /// = every channel.
    pub input_range: Option<(usize, Option<usize>)>,
}

/// How long the control side waits for the engine thread to reopen a
/// stream.
const REOPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// Every engine gets a new id, so the device watcher notices a restart.
static ENGINE_IDS: AtomicU64 = AtomicU64::new(1);

/// Minimum history needed to compute a noise profile (one FFT frame).
const MIN_PROFILE_SAMPLES: usize = 1024;

/// How long `capture_noise_profile` waits for live audio / the callback.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct AudioEngine {
    /// `None` while the device is lost (or failed to reopen).
    in_stream: Option<cpal::Stream>,
    /// One per planned output device; `None` while it is waiting or lost.
    out_streams: Vec<Option<cpal::Stream>>,
    /// What the streams play, kept so a stream can be rebuilt after a
    /// re-plug (`streams`).
    processor: SharedProcessor,
    feeds: Vec<SharedFeed>,
    links: Arc<EngineLinks>,
    /// NDI senders. Declared after the streams so they stop after the input
    /// callback (the ring producers' owner) is gone.
    ndi_senders: Vec<NdiBusSender>,
    ndi_applied: NdiOutputs,
    /// NDI receivers; they stop after the input callback too, all signalled
    /// before any is joined.
    _ndi_receivers: NdiReceivers,
    ndi_inputs: Vec<NdiInputHandle>,
    ndi_inputs_applied: NdiInputs,
    /// Network-audio threads; like NDI they stop after the input callback.
    _net_threads: NetAudioThreads,
    net_audio: NetAudioInfo,
    /// Computer-audio capture; on macOS dropping it removes the tap and
    /// unmutes the tapped apps.
    _computer_capture: Option<ComputerAudioCapture>,
    computer_audio: Option<ComputerAudioHandle>,
    computer_audio_applied: bool,
    /// Control side of the record ring; handed to `RunningEngine`.
    record_port: Option<RecordPort>,
    playback_port: Option<PlaybackPort>,
    /// Control side of the record-preview ring; handed to `RunningEngine`.
    preview_player: Option<crate::preview::PreviewPlayer>,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub latency_ms: f32,
    pub input_device_name: String,
    pub(crate) input_device_identity: String,
    pub monitor_output_name: String,
    pub broadcast_output_name: Option<String>,
    /// Main (PA) output, when one was selected and opened.
    pub main_output_name: Option<String>,
    /// Mon 2 / mon 3 outputs, when selected and opened.
    pub monitor2_output_name: Option<String>,
    pub monitor3_output_name: Option<String>,
    /// RNNoise stage usable (engine at 48 kHz).
    pub neural_available: bool,
    /// Input channels of the open input device (one mixer strip each).
    pub input_channels: u32,
    /// First device channel read by strip 0 (0-based).
    pub input_first_channel: u32,
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
    pub(crate) input_device_identity: String,
    pub monitor_output_name: String,
    pub broadcast_output_name: Option<String>,
    /// Main (PA) output, when one was selected and opened.
    pub main_output_name: Option<String>,
    /// Mon 2 / mon 3 outputs, when selected and opened.
    pub monitor2_output_name: Option<String>,
    pub monitor3_output_name: Option<String>,
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
    /// Network audio: the Remote strips right after the hardware ones, and
    /// the sender.
    pub net_audio: NetAudioInfo,
    /// Computer audio, when it runs: the pair of strips right after the
    /// hardware ones.
    pub computer_audio: Option<ComputerAudioHandle>,
    /// Whether computer audio was on when this engine started.
    pub computer_audio_applied: bool,
    /// Multitrack recording; stopped (and finalized) before the engine.
    pub recorder: EngineRecorder,
    /// First device channel read by strip 0 (0-based).
    pub input_first_channel: u32,
    /// Every stream's device and state.
    pub links: Arc<EngineLinks>,
    /// Unique per start, for the device watcher.
    pub id: u64,
    pub playback: PlaybackPort,
    /// Plays the last recording into the Monitor output.
    pub preview: crate::preview::PreviewPlayer,
}

enum EngineCommand {
    Stop,
    /// Reopen stream `index` (0 = input, i + 1 = output device i) on the
    /// device the watcher matched.
    Reopen {
        index: usize,
        target: device_names::ListedDevice,
        reply: mpsc::Sender<Result<(), String>>,
    },
}

#[derive(Clone)]
struct EngineInfo {
    sample_rate: u32,
    buffer_frames: u32,
    latency_ms: f32,
    input_device_name: String,
    input_device_identity: String,
    monitor_output_name: String,
    broadcast_output_name: Option<String>,
    main_output_name: Option<String>,
    monitor2_output_name: Option<String>,
    monitor3_output_name: Option<String>,
    neural_available: bool,
    input_channels: u32,
    ndi_sending: Vec<String>,
    ndi_applied: NdiOutputs,
    ndi_inputs: Vec<NdiInputHandle>,
    ndi_inputs_applied: NdiInputs,
    net_audio: NetAudioInfo,
    computer_audio: Option<ComputerAudioHandle>,
    computer_audio_applied: bool,
    input_first_channel: u32,
    links: Arc<EngineLinks>,
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
    /// The same for Stream, which hears the voice strip's full chain.
    stream_path_latency_samples: AtomicU64,
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
            stream_path_latency_samples: AtomicU64::new(0),
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
            input_device_identity: self.input_device_identity.clone(),
            monitor_output_name: self.monitor_output_name.clone(),
            broadcast_output_name: self.broadcast_output_name.clone(),
            main_output_name: self.main_output_name.clone(),
            monitor2_output_name: self.monitor2_output_name.clone(),
            monitor3_output_name: self.monitor3_output_name.clone(),
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
            net_audio: self.net_audio.clone(),
            computer_audio: self.computer_audio.clone(),
            computer_audio_applied: self.computer_audio_applied,
            input_first_channel: self.input_first_channel,
            links: self.links.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn start<R: Runtime>(
        app: AppHandle<R>,
        params: Arc<DspParams>,
        shared: Arc<EngineShared>,
        mixer: MixerLink,
        devices: DeviceSelection,
        ndi: NdiEngineConfig,
        net: NetAudioConfig,
        computer_audio_on: bool,
    ) -> Result<Self, String> {
        let resolved = open::resolve(&devices)?;
        let sr = resolved.in_cfg.sample_rate();
        let device_channels = (resolved.in_cfg.channels() as usize).max(1);
        let (first, count) =
            config::resolve_input_range(devices.input_range, device_channels, &resolved.input_name)?;
        let in_channels = count;
        let input_device_name = resolved.input_name.clone();

        let input_device_identity = device_names::device_uid(&resolved.in_dev)
            .unwrap_or_else(|| input_device_name.clone());

        let buf_frames = 256u32;
        let latency_ms = (buf_frames as f32 * 2.0 / sr as f32) * 1000.0;

        // One ring per planned output device, (L, R) per bus on it. Devices
        // missing now get their ring too, so they can join when plugged in.
        let ring_frames = (sr as usize / 5).max(buf_frames as usize * 8);
        let clock_for = |name: &str| {
            if name == resolved.input_name {
                ClockMode::Shared
            } else {
                ClockMode::Corrected
            }
        };
        let mut routes = Vec::with_capacity(resolved.plan.groups.len());
        let mut feeds: Vec<SharedFeed> = Vec::with_capacity(resolved.plan.groups.len());
        let mut output_links = Vec::with_capacity(resolved.plan.groups.len());
        for (group, opened) in resolved.plan.groups.iter().zip(resolved.outputs.iter()) {
            let (producer, consumer) =
                ringbuf::HeapRb::<f32>::new(ring_frames * group.width()).split();
            let state = if opened.is_some() { LinkState::Ok } else { LinkState::Waiting };
            let clock = clock_for(&group.name);
            let link = OutputLink::new(group.name.clone(), group.members.clone(), clock, state)
                .with_buffer_ms(output_feed::clock_buffer_ms(clock, buf_frames as usize, sr));
            routes.push(DeviceRoute {
                producer,
                buses: group.members.iter().map(|m| m.bus).collect(),
                health: link.health.clone(),
            });
            let mut feed = OutputFeed::new(
                consumer,
                group.pairs(),
                clock,
                sr,
                buf_frames as usize,
                link.stats.clone(),
            );
            if let Some((device, cfg)) = opened {
                let channels = cfg.channels() as usize;
                link.opened(channels, feed.set_device_channels(channels));
                link.health.bind(device_names::device_uid(device));
            }
            feeds.push(Arc::new(Mutex::new(feed)));
            output_links.push(link);
        }
        let bus_name = |bus: usize| resolved.plan.device_of(bus).map(str::to_string);
        let monitor_output_name = bus_name(BUS_MONITOR).unwrap_or_default();
        let broadcast_output_name = bus_name(BUS_STREAM);
        let main_output_name = bus_name(BUS_MAIN);
        let monitor2_output_name = bus_name(BUS_MONITOR2);
        let monitor3_output_name = bus_name(BUS_MONITOR3);
        let own_names: Vec<String> = resolved.plan.groups.iter().map(|g| g.name.clone()).collect();
        let links = Arc::new(EngineLinks {
            input: InputLink {
                name: resolved.input_name.clone(),
                health: {
                    let health = Arc::new(health::StreamHealth::new(LinkState::Ok));
                    health.bind(device_names::device_uid(&resolved.in_dev));
                    health
                },
                first,
                count,
                channels: AtomicUsize::new(device_channels),
            },
            outputs: output_links,
        });

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
        // Never receive one of this engine's own outputs: it would feed the
        // mix back into itself. The UI flags these, but a saved selection or
        // a renamed output can still match, so the engine checks too.
        let own_outputs: Vec<String> = ndi
            .outputs
            .plan()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        let local_machines = crate::ndi::receive::local_machine_names();
        let receivable = NdiInputs {
            sources: ndi
                .inputs
                .sources
                .iter()
                .filter(|source| {
                    !crate::ndi::receive::is_own_source(source, &local_machines, &own_outputs)
                })
                .cloned()
                .collect(),
        };
        // Computer audio: a stereo pair right after the hardware strips. Its
        // ring and feed exist now; the capture starts once the outputs run.
        let computer_on = computer_audio_on && StripLayout::computer_fits(in_channels);
        if computer_audio_on && !computer_on {
            log::warn!("computer audio not started: no room for its two strips after {in_channels} input channels");
        }
        // Network audio: Remote strips right after the hardware ones (before
        // the computer pair), one ring each, filled by the receiver thread.
        let net_started = crate::netaudio::start(
            &net,
            sr,
            StripLayout::new(in_channels, computer_on, 0).first_net_strip(),
            StripLayout::net_room(in_channels, computer_on),
        );
        let net_count = net_started.feeds.len();
        let (pending_computer, computer_feed) =
            match StripLayout::with_net(in_channels, computer_on, net_count, 0).computer_strips() {
                Some(strips) => {
                    let (pending, feed) = computer_audio::prepare(sr, strips);
                    (Some(pending), Some(feed))
                }
                None => (None, None),
            };
        let first_ndi_strip =
            StripLayout::with_net(in_channels, computer_on, net_count, 0).first_ndi_strip();
        let ndi_in = crate::ndi::receive::start_receivers(
            &receivable,
            sr,
            first_ndi_strip,
            StripLayout::ndi_room_with_net(in_channels, computer_on, net_count),
        );
        let layout =
            StripLayout::with_net(in_channels, computer_on, net_count, ndi_in.feeds.len());
        // Record what feeds each strip before the stream plays: a strip whose
        // feed changed since the last start is reset to the new-strip
        // defaults (fader off) before any audio passes through it. NDI strips
        // are named after their sources unless the operator renamed them.
        let ndi_strips: Vec<(usize, String, String)> = ndi_in
            .handles
            .iter()
            .map(|input| (input.strip, input.source.clone(), input.label.clone()))
            .collect();
        let mut strip_feeds = crate::mixer_control::engine_strip_feeds(
            layout.hardware,
            layout.computer_strips().map(|(left, _)| left),
            &ndi_strips,
        );
        crate::mixer_control::label_hardware_strips(&mut strip_feeds, first);
        strip_feeds.extend(crate::mixer_control::net_strip_feeds(
            layout.first_net_strip(),
            layout.net,
        ));
        strip_feeds.sort_by_key(|feed| feed.index);
        let reset = mixer.assign_strip_feeds(&strip_feeds);
        if !reset.is_empty() {
            log::info!("mixer strips {reset:?} have a new source; reset to defaults");
        }

        let (meters_tx, meters_rx) = mpsc::sync_channel::<MetersPayload>(32);
        let mut dsp = DspChain::new(sr as f64);
        dsp.sync_params(&params);
        shared
            .dsp_latency_samples
            .store(dsp.total_latency_samples() as u64, Ordering::Relaxed);
        let neural_available = dsp.neural_available();

        let meter_slots = Arc::new(MixerMeterSlots::new(layout.total()));
        // Record ring: every strip + Stream + Main, preallocated here.
        let (record_tap, record_port) =
            crate::recorder::record_ring(layout.total(), sr, MAX_CALLBACK_FRAMES);
        let (playback_tap, playback_port) = playback_channel(sr, layout.total())?;
        let (preview_tap, preview_player) = crate::preview::preview_ring(sr);
        let processor = InputProcessor::new(
            sr,
            HardwareInput {
                stride: device_channels,
                first,
                count,
            },
            dsp,
            params,
            shared.clone(),
            mixer,
            meter_slots.clone(),
            routes,
            ndi_routes,
            computer_feed,
            ndi_in.feeds,
            input_proc::NetIo {
                inputs: net_started.feeds,
                send: net_started.send_route,
            },
            record_tap,
            preview_tap,
            meters_tx,
        ).with_playback(playback_tap);

        let processor: SharedProcessor = Arc::new(Mutex::new(processor));
        let in_stream = streams::build_input(
            &resolved.in_dev,
            &resolved.in_cfg,
            buf_frames,
            processor.clone(),
            links.input.health.clone(),
        )?;

        // Outputs play before the input, as before. One that cannot open is
        // marked failed (its lamp says why); the others still play.
        let mut out_streams: Vec<Option<cpal::Stream>> = Vec::with_capacity(feeds.len());
        for ((opened, feed), link) in resolved
            .outputs
            .iter()
            .zip(feeds.iter())
            .zip(links.outputs.iter())
        {
            let Some((device, cfg)) = opened else {
                out_streams.push(None);
                continue;
            };
            let built = streams::build_output(
                device,
                cfg,
                buf_frames,
                feed.clone(),
                link.health.clone(),
                &link.name,
            )
            .and_then(|stream| stream.play().map(|_| stream).map_err(|e| e.to_string()));
            match built {
                Ok(stream) => out_streams.push(Some(stream)),
                Err(err) => {
                    log::error!("output '{}' did not open: {err}", link.name);
                    link.health.fail(LinkState::Failed, err);
                    out_streams.push(None);
                }
            }
        }

        // Start computer audio now that this engine plays its outputs (see
        // `computer_audio::PendingCapture`). A failure stops the start: the
        // operator turned it on and must hear why it is silent.
        let (computer_capture, computer_handle) = match pending_computer {
            Some(pending) => {
                let (capture, handle) = pending.start(&OwnOutputs {
                    device_names: &own_names,
                })?;
                (Some(capture), Some(handle))
            }
            None => (None, None),
        };

        in_stream.play().map_err(|e| e.to_string())?;

        meters_thread::spawn_meters_thread(app, shared, meters_rx, meter_slots);

        Ok(Self {
            in_stream: Some(in_stream),
            out_streams,
            processor,
            feeds,
            links,
            ndi_senders,
            ndi_applied: ndi.outputs,
            _ndi_receivers: NdiReceivers(ndi_in.receivers),
            ndi_inputs: ndi_in.handles,
            ndi_inputs_applied: ndi.inputs,
            _net_threads: net_started.threads,
            net_audio: net_started.info,
            _computer_capture: computer_capture,
            computer_audio: computer_handle,
            computer_audio_applied: computer_audio_on,
            record_port: Some(record_port),
            playback_port: Some(playback_port),
            preview_player: Some(preview_player),
            sample_rate: sr,
            buffer_frames: buf_frames,
            latency_ms,
            input_device_name,
            input_device_identity,
            monitor_output_name,
            broadcast_output_name,
            main_output_name,
            monitor2_output_name,
            monitor3_output_name,
            neural_available,
            input_channels: in_channels as u32,
            input_first_channel: first as u32,
        })
    }

    /// Reopen stream `index` (0 = input, i + 1 = output device i) on the
    /// device with its name, at the engine rate. Runs on the engine thread,
    /// which owns the streams. Sets the stream's link state either way.
    fn reopen(&mut self, index: usize, target: &device_names::ListedDevice) -> Result<(), String> {
        let result = if index == 0 {
            self.reopen_input(target)
        } else {
            self.reopen_output(index - 1, target)
        };
        let health = if index == 0 {
            Some(&self.links.input.health)
        } else {
            self.links.outputs.get(index - 1).map(|link| &link.health)
        };
        if let Some(health) = health {
            match &result {
                Ok(()) => health.set(LinkState::Ok),
                Err(err) => health.fail(LinkState::Failed, err.clone()),
            }
        }
        result
    }

    fn reopen_input(&mut self, target: &device_names::ListedDevice) -> Result<(), String> {
        self.in_stream = None;
        let host = cpal::default_host();
        let name = self.links.input.name.clone();
        let device = device_names::find_target(&host, true, target)?
            .ok_or_else(|| format!("'{name}' is not connected"))?;
        self.links.input.health.bind(device_names::device_uid(&device));
        let cfg = config::input_config_at(&device, self.sample_rate)?;
        let channels = cfg.channels() as usize;
        self.processor
            .lock()
            .set_stride(channels)
            .map_err(|needed| format!("has {channels} channels; the input needs {needed}"))?;
        self.links.input.channels.store(channels, Ordering::Relaxed);
        let stream = streams::build_input(
            &device,
            &cfg,
            self.buffer_frames,
            self.processor.clone(),
            self.links.input.health.clone(),
        )?;
        stream.play().map_err(|e| e.to_string())?;
        self.in_stream = Some(stream);
        log::info!("input '{name}' reconnected");
        Ok(())
    }

    fn reopen_output(&mut self, index: usize, target: &device_names::ListedDevice) -> Result<(), String> {
        let (Some(link), Some(feed)) = (self.links.outputs.get(index), self.feeds.get(index)) else {
            return Err(format!("no output {index}"));
        };
        if let Some(slot) = self.out_streams.get_mut(index) {
            *slot = None;
        }
        let host = cpal::default_host();
        let device = device_names::find_target(&host, false, target)?
            .ok_or_else(|| format!("'{}' is not connected", link.name))?;
        link.health.bind(device_names::device_uid(&device));
        let cfg = config::output_config_at(&device, self.sample_rate)?;
        let channels = cfg.channels() as usize;
        {
            let mut feed = feed.lock();
            link.opened(channels, feed.set_device_channels(channels));
            feed.restart();
        }
        let stream = streams::build_output(
            &device,
            &cfg,
            self.buffer_frames,
            feed.clone(),
            link.health.clone(),
            &link.name,
        )?;
        stream.play().map_err(|e| e.to_string())?;
        if let Some(slot) = self.out_streams.get_mut(index) {
            *slot = Some(stream);
        }
        log::info!("output '{}' reconnected", link.name);
        Ok(())
    }
}

impl RunningEngine {
    pub fn spawn<R: Runtime>(
        app: AppHandle<R>,
        params: Arc<DspParams>,
        mixer: MixerLink,
        devices: DeviceSelection,
        ndi: NdiEngineConfig,
        net: NetAudioConfig,
        computer_audio_on: bool,
    ) -> Result<Self, String> {
        let (ready_tx, ready_rx) =
            mpsc::sync_channel::<
                Result<(EngineInfo, RecordPort, PlaybackPort, crate::preview::PreviewPlayer), String>,
            >(1);
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
            net,
            computer_audio_on,
        ) {
            Ok(mut engine) => {
                let info = engine.info();
                let (Some(port), Some(preview)) =
                    (engine.record_port.take(), engine.preview_player.take())
                else {
                    let _ = ready_tx.send(Err("The record ring was not created".to_string()));
                    return;
                };
                let Some(playback) = engine.playback_port.take() else {
                    let _ = ready_tx.send(Err("The soundcheck ring was not created".to_string()));
                    return;
                };
                if ready_tx.send(Ok((info, port, playback, preview))).is_err() {
                    return;
                }

                let mut engine = engine;
                while let Ok(command) = stop_rx.recv() {
                    match command {
                        EngineCommand::Stop => break,
                        EngineCommand::Reopen { index, target, reply } => {
                            let _ = reply.send(engine.reopen(index, &target));
                        }
                    }
                }
                drop(engine);
            }
            Err(err) => {
                let _ = ready_tx.send(Err(err));
            }
        });

        let (info, record_port, playback, preview) = ready_rx
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
            input_device_identity: info.input_device_identity,
            monitor_output_name: info.monitor_output_name,
            broadcast_output_name: info.broadcast_output_name,
            main_output_name: info.main_output_name,
            monitor2_output_name: info.monitor2_output_name,
            monitor3_output_name: info.monitor3_output_name,
            neural_available: info.neural_available,
            input_channels: info.input_channels,
            ndi_sending: info.ndi_sending,
            ndi_applied: info.ndi_applied,
            ndi_inputs: info.ndi_inputs,
            ndi_inputs_applied: info.ndi_inputs_applied,
            net_audio: info.net_audio,
            computer_audio: info.computer_audio,
            computer_audio_applied: info.computer_audio_applied,
            recorder: EngineRecorder::new(record_port),
            input_first_channel: info.input_first_channel,
            links: info.links,
            id: ENGINE_IDS.fetch_add(1, Ordering::Relaxed),
            playback,
            preview,
        })
    }

    /// A handle that asks this engine's thread to reopen a stream, usable
    /// without holding the engine lock.
    pub fn reopener(&self) -> Reopener {
        Reopener {
            tx: self.stop_tx.clone(),
        }
    }

    /// `engine_status.devices`: every stream's device, state, pair and clock.
    pub fn devices_json(&self) -> serde_json::Value {
        self.links.status_json()
    }

    pub fn stop(mut self) {
        // A recording is finalized before the streams stop.
        let _ = self.recorder.stop();
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
        self.latency_ms + self.samples_ms(self.shared.path_latency_samples.load(Ordering::Relaxed))
    }

    /// Like `total_latency_ms`, for Stream (the voice strip's full chain).
    pub fn stream_latency_ms(&self) -> f32 {
        self.latency_ms
            + self.samples_ms(self.shared.stream_path_latency_samples.load(Ordering::Relaxed))
    }

    fn samples_ms(&self, samples: u64) -> f32 {
        if self.sample_rate > 0 {
            samples as f32 * 1000.0 / self.sample_rate as f32
        } else {
            0.0
        }
    }

    pub fn noise_profile_ready(&self) -> bool {
        self.shared.noise_profile_ready.load(Ordering::Acquire)
    }

    pub fn dropped_output_samples(&self) -> u64 {
        self.shared.dropped_output_samples.load(Ordering::Relaxed)
    }

    /// Mixer strips: input channels, the Remote strips, the computer-audio
    /// pair, then NDI inputs.
    pub fn strip_layout(&self) -> StripLayout {
        StripLayout::with_net(
            self.input_channels as usize,
            self.computer_audio.is_some(),
            self.net_audio.inputs.len(),
            self.ndi_inputs.len(),
        )
    }

    /// The device each bus plays on, in `BUS_*` order (`mixer_state`
    /// `buses[].output`). A bus with no device set is `None`; one whose
    /// device is waiting or lost still names it, so its strip and sends stay
    /// on the desk while it is unplugged.
    pub fn bus_outputs(&self) -> BusOutputs {
        let mut outputs = BusOutputs::default();
        outputs[BUS_MAIN] = self.main_output_name.clone();
        outputs[BUS_STREAM] = self.broadcast_output_name.clone();
        outputs[BUS_MONITOR] = Some(self.monitor_output_name.clone());
        outputs[BUS_MONITOR2] = self.monitor2_output_name.clone();
        outputs[BUS_MONITOR3] = self.monitor3_output_name.clone();
        outputs
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

/// Asks an engine thread to reopen one stream (`RunningEngine::reopener`).
pub struct Reopener {
    tx: mpsc::Sender<EngineCommand>,
}

impl Reopener {
    /// Reopen stream `index` (0 = input, i + 1 = output device i) on
    /// `target`. The link state says how it went.
    pub fn reopen(&self, index: usize, target: device_names::ListedDevice) -> Result<(), String> {
        let (reply, answer) = mpsc::channel();
        self.tx
            .send(EngineCommand::Reopen { index, target, reply })
            .map_err(|_| "The engine has stopped".to_string())?;
        answer
            .recv_timeout(REOPEN_TIMEOUT)
            .map_err(|_| "The engine did not answer in time".to_string())?
    }
}

impl Drop for RunningEngine {
    fn drop(&mut self) {
        let _ = self.recorder.stop();
        let _ = self.stop_tx.send(EngineCommand::Stop);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[inline]
pub(super) fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[inline]
pub(super) fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / 32_768.0
}

#[inline]
pub(super) fn u16_to_f32(sample: u16) -> f32 {
    sample as f32 / 65_535.0 * 2.0 - 1.0
}

#[inline]
pub(super) fn f32_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[inline]
pub(super) fn f32_to_u16(sample: f32) -> u16 {
    (((sample.clamp(-1.0, 1.0) + 1.0) * 0.5) * u16::MAX as f32).round() as u16
}

#[cfg(test)]
mod device_probe {
    //! Manual check of the real device lists (issue #25: a USB headset whose
    //! mic and speaker are separate CoreAudio devices must appear in both).
    //! Needs audio hardware, so it is ignored by default:
    //! `cargo test print_devices -- --ignored --nocapture`.
    use super::list_devices;

    #[test]
    #[ignore = "needs audio hardware; run manually"]
    fn print_devices() {
        for (label, input) in [("input", true), ("output", false)] {
            let devices = list_devices(input);
            println!("{label} devices ({}):", devices.len());
            for device in &devices {
                println!("  {}{}", device.name, if device.is_default { " (default)" } else { "" });
            }
            assert!(!devices.is_empty(), "no {label} devices found");
        }
    }
}

#[cfg(test)]
mod input_channel_tests {
    use super::config::widest_config as widest_input_config;
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange};

    fn range(ch: u16, min: u32, max: u32, fmt: SampleFormat) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(ch, min, max, SupportedBufferSize::Unknown, fmt)
    }

    #[test]
    fn a_one_channel_default_opens_every_channel_at_that_rate() {
        // What a macOS aggregate of two USB mics reports.
        let chosen = SupportedStreamConfig::new(1, 48_000, SupportedBufferSize::Unknown, SampleFormat::F32);
        let ranges = [
            range(2, 8_000, 8_000, SampleFormat::F32),
            range(2, 44_100, 44_100, SampleFormat::F32),
            range(2, 48_000, 48_000, SampleFormat::F32),
        ];
        let cfg = widest_input_config(&ranges, chosen);
        assert_eq!((cfg.channels(), cfg.sample_rate()), (2, 48_000));
    }

    #[test]
    fn rate_and_format_never_change() {
        let chosen = SupportedStreamConfig::new(2, 48_000, SupportedBufferSize::Unknown, SampleFormat::F32);
        let ranges = [
            range(8, 44_100, 44_100, SampleFormat::F32), // other rate
            range(16, 48_000, 48_000, SampleFormat::I16), // other format
            range(1, 48_000, 48_000, SampleFormat::F32), // fewer channels
        ];
        let cfg = widest_input_config(&ranges, chosen.clone());
        assert_eq!(cfg, chosen);
        let wide = [range(18, 32_000, 96_000, SampleFormat::F32)];
        let cfg = widest_input_config(&wide, chosen);
        assert_eq!((cfg.channels(), cfg.sample_rate(), cfg.sample_format()), (18, 48_000, SampleFormat::F32));
    }
}
