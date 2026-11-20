//! Network audio: remote people as mixer strips
//! (`docs/specs/2026-11-20-network-audio-design.md`).
//!
//! - `protocol`: the signed UDP packet format.
//! - `stream`: one peer's packets → its ring (order, gaps, jitter).
//! - `slots`: which peer feeds which Remote strip.
//! - `receiver`: the listening thread; `sender`: the sending thread.
//! - `settings`: `net_audio.json`.
//!
//! The audio callback only touches the rings (through `NetInputFeed`, which
//! reads with the NDI drift reader) and atomics. Sockets, signing, logging
//! and allocation stay on the two network threads.
pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod settings;
pub mod slots;
pub mod stream;

#[cfg(test)]
mod net_tests;
#[cfg(test)]
mod protocol_tests;

pub use settings::{NetAudioConfig, NetAudioSettings, MAX_NET_INPUTS};

use crate::ndi::receive::drift::{frames_for_ms, DriftReader};
use protocol::{RoomKey, PACKET_FRAMES};
use receiver::NetReceiver;
use ringbuf::traits::{Consumer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use sender::NetSender;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

pub const STEREO: usize = 2;
/// Largest read-ratio nudge for a Remote strip: ±0.5 %, so a burst of late
/// packets drains back to the buffer target in seconds (NDI uses ±0.1 %).
/// About 9 cents at full nudge, which only lasts while far from target.
pub const NET_MAX_NUDGE: f64 = 0.005;
/// Above target + this, a Remote strip's ring is trimmed back to target.
pub const NET_OVERRUN_MS: usize = 80;
/// Smallest buffer target: one packet plus one 10 ms device block, so a
/// normal callback never finds the ring short.
pub const MIN_TARGET_MS: usize = 15;

/// Peer and send rings: one second of stereo at the engine rate.
pub fn ring_capacity(sample_rate: u32) -> usize {
    (sample_rate as usize * STEREO).max(PACKET_FRAMES * STEREO * 8)
}

/// A fresh random id for one sender run (0 only if the OS has no randomness).
pub fn new_session_id() -> u32 {
    let mut bytes = [0u8; 4];
    let _ = getrandom::getrandom(&mut bytes);
    u32::from_le_bytes(bytes)
}

/// Milliseconds as microseconds, for status.
fn us_to_ms(us: u64) -> f32 {
    us as f32 / 1_000.0
}

/// Who holds a slot. Written by the receiver thread only; the audio
/// callback never reads it.
#[derive(Debug, Clone, Default)]
pub struct PeerInfo {
    pub name: String,
    pub address: String,
}

/// Live counters of one Remote strip. Atomics shared by the receiver
/// thread, the audio callback and `engine_status`.
#[derive(Debug, Default)]
pub struct PeerStats {
    pub connected: AtomicBool,
    /// Bumped by the receiver thread whenever the slot changes hands (a new
    /// peer, or the peer left). The callback then drops what is left in the
    /// ring, so one person's audio never plays under another's strip.
    pub generation: AtomicU32,
    /// Stereo frames waiting in the ring (audio callback).
    pub fill_frames: AtomicU64,
    pub underruns: AtomicU64,
    /// Samples lost to a full ring or trimmed after a stall.
    pub dropped_samples: AtomicU64,
    pub lost_packets: AtomicU64,
    pub late_packets: AtomicU64,
    /// Packets refused because the sender runs at another sample rate.
    pub rate_mismatch_packets: AtomicU64,
    pub sender_rate: AtomicU32,
    pub rtt_us: AtomicU64,
    pub jitter_us: AtomicU64,
    pub info: Mutex<PeerInfo>,
}

impl PeerStats {
    /// A new peer took the slot: counters start again.
    pub fn reset_for(&self, address: String) {
        for counter in [
            &self.underruns,
            &self.dropped_samples,
            &self.lost_packets,
            &self.late_packets,
            &self.rate_mismatch_packets,
            &self.rtt_us,
            &self.jitter_us,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
        self.sender_rate.store(0, Ordering::Relaxed);
        self.set_info(PeerInfo { name: String::new(), address });
        self.connected.store(true, Ordering::Relaxed);
    }

    pub fn set_info(&self, info: PeerInfo) {
        match self.info.lock() {
            Ok(mut guard) => *guard = info,
            Err(poisoned) => *poisoned.into_inner() = info,
        }
    }

    pub fn set_name(&self, name: String) {
        match self.info.lock() {
            Ok(mut guard) => guard.name = name,
            Err(poisoned) => poisoned.into_inner().name = name,
        }
    }

    pub fn info(&self) -> PeerInfo {
        match self.info.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// The audio callback's side of one Remote strip.
pub struct NetInputFeed<C> {
    consumer: C,
    reader: DriftReader,
    stats: Arc<PeerStats>,
    generation: u32,
}

impl<C: Consumer<Item = f32>> NetInputFeed<C> {
    pub fn new(consumer: C, sample_rate: u32, buffer_ms: u32, stats: Arc<PeerStats>) -> Self {
        let target = frames_for_ms(sample_rate, (buffer_ms as usize).max(MIN_TARGET_MS));
        let high = target + frames_for_ms(sample_rate, NET_OVERRUN_MS);
        NetInputFeed {
            consumer,
            reader: DriftReader::with_settings(target, high, NET_MAX_NUDGE),
            stats,
            generation: 0,
        }
    }

    /// Audio thread: fill `out` with the next block of this peer (mono).
    /// Real-time safe: ring operations and relaxed atomics only.
    pub fn pull(&mut self, out: &mut [f32]) {
        let generation = self.stats.generation.load(Ordering::Acquire);
        if generation != self.generation {
            // The slot changed hands: drop the ring (whole frames only; the
            // producer pushes whole frames) and buffer again.
            self.generation = generation;
            let stale = self.consumer.occupied_len();
            self.consumer.skip(stale - stale % STEREO);
            self.reader.restart();
        }
        let report = self.reader.read(&mut self.consumer, out);
        self.stats
            .fill_frames
            .store(report.fill_frames as u64, Ordering::Relaxed);
        if report.dropped_samples > 0 {
            self.stats
                .dropped_samples
                .fetch_add(report.dropped_samples as u64, Ordering::Relaxed);
        }
        if report.underrun && self.stats.connected.load(Ordering::Relaxed) {
            self.stats.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// One Remote strip, for status and strip naming.
#[derive(Debug, Clone)]
pub struct NetInputHandle {
    pub strip: usize,
    pub label: String,
    pub stats: Arc<PeerStats>,
}

/// Default name of Remote strip `slot`.
pub fn strip_label(slot: usize) -> String {
    format!("Remote {}", slot + 1)
}

/// Stable identity of Remote strip `slot` (a strip keeps its settings while
/// the same slot feeds it).
pub fn strip_identity(slot: usize) -> String {
    format!("net:{}", slot + 1)
}

/// Counters of the sending side.
#[derive(Debug, Default)]
pub struct SenderStats {
    /// A pong arrived within the last 3 seconds and the receiver plays us.
    pub connected: AtomicBool,
    /// A pong arrived within the last 3 seconds.
    pub answered: AtomicBool,
    /// The last pong's verdict (`protocol::PONG_*`).
    pub verdict: AtomicU8,
    pub packets_sent: AtomicU64,
    pub send_errors: AtomicU64,
    /// Samples the send ring could not take (callback) or trimmed (thread).
    pub dropped_samples: AtomicU64,
    pub rtt_us: AtomicU64,
    pub address: Mutex<String>,
}

/// Counters of the receiving socket.
#[derive(Debug, Default)]
pub struct ReceiverStats {
    /// Datagrams that failed the signature or format checks.
    pub bad_packets: AtomicU64,
    /// Peers turned away because every slot was taken.
    pub refused_packets: AtomicU64,
}

/// What a started engine knows about network audio (for status).
#[derive(Debug, Clone, Default)]
pub struct NetAudioInfo {
    /// The settings this engine started with (sanitized).
    pub applied: NetAudioConfig,
    pub inputs: Vec<NetInputHandle>,
    pub receiver_stats: Arc<ReceiverStats>,
    pub receive_error: Option<String>,
    pub sender_stats: Option<Arc<SenderStats>>,
}

/// The network threads; dropping this stops and joins them.
#[derive(Default)]
pub struct NetAudioThreads {
    pub receiver: Option<NetReceiver>,
    pub sender: Option<NetSender>,
}

/// The engine's side of the send ring: the callback copies `bus` into it.
pub struct NetSendRoute<P> {
    pub producer: P,
    pub bus: usize,
    pub stats: Arc<SenderStats>,
}

/// What `start` hands the engine.
pub struct StartedNet {
    pub info: NetAudioInfo,
    pub threads: NetAudioThreads,
    pub feeds: Vec<NetInputFeed<HeapCons<f32>>>,
    pub send_route: Option<NetSendRoute<HeapProd<f32>>>,
}

/// Bus index of a settings bus name.
pub fn bus_index(name: &str) -> usize {
    use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_STREAM};
    match name {
        "stream" => BUS_STREAM,
        "monitor" => BUS_MONITOR,
        _ => BUS_MAIN,
    }
}

/// Start receiving (up to `max_strips` Remote strips from `first_strip`)
/// and sending, as `config` asks. A side that cannot start is logged and
/// reported, so the engine always starts.
pub fn start(
    config: &NetAudioConfig,
    sample_rate: u32,
    first_strip: usize,
    max_strips: usize,
) -> StartedNet {
    let config = config.sanitized();
    let key = RoomKey::from_passphrase(&config.passphrase);
    let session = new_session_id();
    let mut started = StartedNet {
        info: NetAudioInfo {
            applied: config.clone(),
            ..NetAudioInfo::default()
        },
        threads: NetAudioThreads::default(),
        feeds: Vec::new(),
        send_route: None,
    };
    if config.receive.enabled || config.send.enabled {
        if let Some(problem) = config.passphrase_problem() {
            log::warn!("network audio not started: {problem}");
            started.info.receive_error = Some(problem);
            return started;
        }
    }
    let slots = config.strips().min(max_strips);
    if slots > 0 {
        start_receive(&config, &key, sample_rate, first_strip, slots, session, &mut started);
    }
    if config.send_ready() {
        let stats = Arc::new(SenderStats::default());
        let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(sample_rate)).split();
        let sender = NetSender::start(sender::SendPlan {
            target: config.send.target.clone(),
            name: sender_name(&config.send.name),
            stereo: config.send.stereo,
            key,
            sample_rate,
            session,
            consumer,
            stats: stats.clone(),
        });
        log::info!("network audio sending {} to {}", config.send.bus, config.send.target);
        started.threads.sender = Some(sender);
        started.info.sender_stats = Some(stats.clone());
        started.send_route = Some(NetSendRoute {
            producer,
            bus: bus_index(&config.send.bus),
            stats,
        });
    }
    started
}

fn start_receive(
    config: &NetAudioConfig,
    key: &RoomKey,
    sample_rate: u32,
    first_strip: usize,
    slots: usize,
    session: u32,
    started: &mut StartedNet,
) {
    let mut producers = Vec::with_capacity(slots);
    let mut stats = Vec::with_capacity(slots);
    let mut consumers = Vec::with_capacity(slots);
    for _ in 0..slots {
        let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(sample_rate)).split();
        producers.push(producer);
        consumers.push(consumer);
        stats.push(Arc::new(PeerStats::default()));
    }
    let plan = receiver::ReceivePlan {
        port: config.receive.port,
        key: key.clone(),
        sample_rate,
        own_session: session,
        producers,
        stats: stats.clone(),
        socket_stats: started.info.receiver_stats.clone(),
    };
    match NetReceiver::start(plan) {
        Ok(receiver) => {
            log::info!("network audio listening on UDP {}", config.receive.port);
            for (slot, (consumer, stats)) in consumers.into_iter().zip(stats).enumerate() {
                started.info.inputs.push(NetInputHandle {
                    strip: first_strip + slot,
                    label: strip_label(slot),
                    stats: stats.clone(),
                });
                started.feeds.push(NetInputFeed::new(
                    consumer,
                    sample_rate,
                    config.receive.buffer_ms,
                    stats,
                ));
            }
            started.threads.receiver = Some(receiver);
        }
        Err(err) => {
            log::warn!("network audio not receiving: {err}");
            started.info.receive_error = Some(err);
        }
    }
}

/// The configured name, or this machine's name.
fn sender_name(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let host = host.split('.').next().unwrap_or("").trim().to_string();
    if host.is_empty() {
        "Remote".to_string()
    } else {
        host.chars().take(settings::MAX_NAME_CHARS).collect()
    }
}

/// One Remote strip in `engine_status`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerStatus {
    pub strip: usize,
    pub label: String,
    pub connected: bool,
    pub name: String,
    pub address: String,
    pub buffer_ms: f32,
    pub jitter_ms: f32,
    pub rtt_ms: f32,
    /// Packet + half the round trip + buffer + `engine_latency_ms`.
    pub latency_ms: f32,
    pub lost_packets: u64,
    pub late_packets: u64,
    pub underruns: u64,
    pub dropped_samples: u64,
    pub sender_rate: u32,
    pub rate_mismatch_packets: u64,
}

/// Status of every Remote strip; `engine_latency_ms` is the engine's own
/// input-to-output latency.
pub fn peer_status(
    run: &NetAudioInfo,
    sample_rate: u32,
    engine_latency_ms: f32,
) -> Vec<PeerStatus> {
    let packet_ms = PACKET_FRAMES as f32 * 1_000.0 / sample_rate.max(1) as f32;
    run.inputs
        .iter()
        .map(|input| {
            let stats = &input.stats;
            let connected = stats.connected.load(Ordering::Relaxed);
            let info = if connected { stats.info() } else { PeerInfo::default() };
            let fill = stats.fill_frames.load(Ordering::Relaxed) as f32;
            let buffer_ms = fill * 1_000.0 / sample_rate.max(1) as f32;
            let rtt_ms = us_to_ms(stats.rtt_us.load(Ordering::Relaxed));
            PeerStatus {
                strip: input.strip,
                label: input.label.clone(),
                connected,
                name: info.name,
                address: info.address,
                buffer_ms,
                jitter_ms: us_to_ms(stats.jitter_us.load(Ordering::Relaxed)),
                rtt_ms,
                latency_ms: packet_ms + rtt_ms / 2.0 + buffer_ms + engine_latency_ms,
                lost_packets: stats.lost_packets.load(Ordering::Relaxed),
                late_packets: stats.late_packets.load(Ordering::Relaxed),
                underruns: stats.underruns.load(Ordering::Relaxed),
                dropped_samples: stats.dropped_samples.load(Ordering::Relaxed),
                sender_rate: stats.sender_rate.load(Ordering::Relaxed),
                rate_mismatch_packets: stats.rate_mismatch_packets.load(Ordering::Relaxed),
            }
        })
        .collect()
}

/// The sending side in `engine_status`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderStatus {
    pub connected: bool,
    /// Why an answering receiver does not play us: "full" or "rate".
    pub refused: Option<&'static str>,
    pub address: String,
    pub rtt_ms: f32,
    pub packets_sent: u64,
    pub send_errors: u64,
    pub dropped_samples: u64,
}

pub fn sender_status(stats: &SenderStats) -> SenderStatus {
    let address = match stats.address.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    let refused = if stats.answered.load(Ordering::Relaxed) {
        match stats.verdict.load(Ordering::Relaxed) {
            protocol::PONG_FULL => Some("full"),
            protocol::PONG_RATE_MISMATCH => Some("rate"),
            _ => None,
        }
    } else {
        None
    };
    SenderStatus {
        connected: stats.connected.load(Ordering::Relaxed),
        refused,
        address,
        rtt_ms: us_to_ms(stats.rtt_us.load(Ordering::Relaxed)),
        packets_sent: stats.packets_sent.load(Ordering::Relaxed),
        send_errors: stats.send_errors.load(Ordering::Relaxed),
        dropped_samples: stats.dropped_samples.load(Ordering::Relaxed),
    }
}

/// `engine_status.net_audio`: what runs, the peers and the sender.
/// `engine_latency_ms` is the engine's own input-to-output latency.
pub fn status_json(
    info: Option<&NetAudioInfo>,
    sample_rate: u32,
    engine_latency_ms: f32,
) -> serde_json::Value {
    let Some(info) = info else {
        return serde_json::json!({ "receiving": false, "sending": null, "peers": [] });
    };
    serde_json::json!({
        "receiving": !info.inputs.is_empty(),
        "port": info.applied.receive.port,
        "bufferMs": info.applied.receive.buffer_ms,
        "error": info.receive_error,
        "badPackets": info.receiver_stats.bad_packets.load(Ordering::Relaxed),
        "refusedPackets": info.receiver_stats.refused_packets.load(Ordering::Relaxed),
        "peers": peer_status(info, sample_rate, engine_latency_ms),
        "sending": info.sender_stats.as_deref().map(sender_status),
    })
}
