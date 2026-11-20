//! The sending thread (decision 7).
//!
//! The audio callback copies the chosen bus, interleaved stereo, into a
//! lock-free ring. This thread pops `PACKET_FRAMES` frames at a time, folds
//! them to mono unless sending stereo, signs and sends them. It also pings
//! the receiver once a second (with this sender's name), answers the
//! receiver's pings, and says `bye` when it stops. Clock drift is handled
//! at the receiver; a stall here trims the ring back so latency never
//! builds up.
//!
//! The target is resolved on this thread (DNS may block) and again every
//! `RESOLVE_RETRY` until it works.
use super::protocol::{
    decode, encode, pcm16_encode, Header, Kind, RoomKey, CODEC_PCM16, MAX_PACKET, MAX_PAYLOAD,
    PACKET_FRAMES,
};
use super::receiver::is_benign;
use super::{SenderStats, STEREO};
use ringbuf::traits::{Consumer, Observer};
use ringbuf::HeapCons;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_millis(1);
const PING_INTERVAL: Duration = Duration::from_secs(1);
const RESOLVE_RETRY: Duration = Duration::from_secs(2);
/// No pong for this long: not connected.
const CONNECTED_WINDOW_US: u64 = 3_000_000;
/// Above this fill the ring is trimmed back to `TRIM_TO_MS`.
const STALL_MS: usize = 200;
const TRIM_TO_MS: usize = 10;

pub struct SendPlan {
    /// "host:port".
    pub target: String,
    pub name: String,
    pub stereo: bool,
    pub key: RoomKey,
    pub sample_rate: u32,
    pub session: u32,
    pub consumer: HeapCons<f32>,
    pub stats: Arc<SenderStats>,
}

pub struct NetSender {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NetSender {
    /// Start the thread. It never fails here: an unresolvable target or
    /// socket error shows up in the stats and is retried.
    pub fn start(plan: SendPlan) -> NetSender {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("net-audio-send".to_string())
            .spawn(move || Sender::new(plan).run(&thread_stop))
            .map_err(|e| log::warn!("network audio sender not started: {e}"))
            .ok();
        NetSender { stop, thread }
    }
}

impl Drop for NetSender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Frames → packet payload: stereo pairs, or mono as (L + R) / 2.
/// Returns the payload length in bytes.
pub fn frames_to_payload(stereo_in: &[f32], stereo_out: bool, out: &mut [u8]) -> usize {
    if stereo_out {
        return pcm16_encode(stereo_in, out);
    }
    let mut mono = [0.0f32; PACKET_FRAMES];
    let frames = (stereo_in.len() / STEREO).min(PACKET_FRAMES);
    for (frame, sample) in mono.iter_mut().enumerate().take(frames) {
        *sample = (stereo_in[frame * STEREO] + stereo_in[frame * STEREO + 1]) * 0.5;
    }
    pcm16_encode(&mono[..frames], out)
}

struct Sender {
    plan: SendPlan,
    socket: Option<UdpSocket>,
    target: Option<SocketAddr>,
    clock: Instant,
    seq: u32,
    ping_seq: u32,
    last_pong_us: Option<u64>,
    frames: [f32; PACKET_FRAMES * STEREO],
    payload: [u8; MAX_PAYLOAD],
    out: [u8; MAX_PACKET],
}

impl Sender {
    fn new(plan: SendPlan) -> Self {
        Sender {
            plan,
            socket: None,
            target: None,
            clock: Instant::now(),
            seq: 0,
            ping_seq: 0,
            last_pong_us: None,
            frames: [0.0; PACKET_FRAMES * STEREO],
            payload: [0u8; MAX_PAYLOAD],
            out: [0u8; MAX_PACKET],
        }
    }

    fn now_us(&self) -> u64 {
        self.clock.elapsed().as_micros() as u64
    }

    fn run(mut self, stop: &AtomicBool) {
        let mut last_ping: Option<Instant> = None;
        let mut last_resolve: Option<Instant> = None;
        while !stop.load(Ordering::Relaxed) {
            if self.target.is_none() && last_resolve.is_none_or(|t| t.elapsed() >= RESOLVE_RETRY) {
                last_resolve = Some(Instant::now());
                self.connect();
            }
            self.trim_stall();
            let sent = self.send_ready_packets();
            self.read_replies();
            if last_ping.is_none_or(|t| t.elapsed() >= PING_INTERVAL) {
                last_ping = Some(Instant::now());
                self.ping();
            }
            self.update_connected();
            if sent == 0 {
                std::thread::sleep(IDLE);
            }
        }
        self.say_bye();
    }

    /// Resolve the target and open a socket of its address family.
    fn connect(&mut self) {
        let resolved = self
            .plan
            .target
            .to_socket_addrs()
            .map_err(|e| e.to_string())
            .and_then(|mut addrs| addrs.next().ok_or_else(|| "no address".to_string()));
        let addr = match resolved {
            Ok(addr) => addr,
            Err(err) => {
                self.set_address(format!("cannot resolve {}: {err}", self.plan.target));
                return;
            }
        };
        let bind = if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
        match UdpSocket::bind(bind).and_then(|s| s.set_nonblocking(true).map(|_| s)) {
            Ok(socket) => {
                self.socket = Some(socket);
                self.target = Some(addr);
                self.set_address(addr.to_string());
                log::info!("network audio sending to {addr}");
            }
            Err(err) => self.set_address(format!("cannot open a socket: {err}")),
        }
    }

    fn set_address(&self, text: String) {
        match self.plan.stats.address.lock() {
            Ok(mut guard) => *guard = text,
            Err(poisoned) => *poisoned.into_inner() = text,
        }
    }

    fn ms_samples(&self, ms: usize) -> usize {
        self.plan.sample_rate as usize * ms / 1_000 * STEREO
    }

    /// A stall (or no target yet) must not build latency: trim the ring.
    fn trim_stall(&mut self) {
        let occupied = self.plan.consumer.occupied_len();
        if occupied > self.ms_samples(STALL_MS) {
            let excess = occupied - self.ms_samples(TRIM_TO_MS);
            let skipped = self.plan.consumer.skip(excess - excess % STEREO);
            self.plan
                .stats
                .dropped_samples
                .fetch_add(skipped as u64, Ordering::Relaxed);
        }
    }

    /// Send every whole packet waiting in the ring. Returns packets sent.
    fn send_ready_packets(&mut self) -> usize {
        let (Some(_), Some(target)) = (self.socket.as_ref(), self.target) else {
            return 0;
        };
        let packet_samples = PACKET_FRAMES * STEREO;
        let mut sent = 0;
        while self.plan.consumer.occupied_len() >= packet_samples {
            self.plan.consumer.pop_slice(&mut self.frames);
            let len = frames_to_payload(&self.frames, self.plan.stereo, &mut self.payload);
            let header = Header {
                kind: Kind::Audio,
                channels: if self.plan.stereo { 2 } else { 1 },
                codec: CODEC_PCM16,
                session: self.plan.session,
                seq: self.seq,
                stamp: self.plan.sample_rate as u64,
            };
            self.seq = self.seq.wrapping_add(1);
            let payload = self.payload;
            if self.send(&header, &payload[..len], target) {
                self.plan.stats.packets_sent.fetch_add(1, Ordering::Relaxed);
            } else {
                self.plan.stats.send_errors.fetch_add(1, Ordering::Relaxed);
            }
            sent += 1;
        }
        sent
    }

    fn send(&mut self, header: &Header, payload: &[u8], to: SocketAddr) -> bool {
        let Some(socket) = self.socket.as_ref() else {
            return false;
        };
        match encode(&self.plan.key, header, payload, &mut self.out) {
            Ok(len) => socket.send_to(&self.out[..len], to).is_ok(),
            Err(_) => false,
        }
    }

    fn ping(&mut self) {
        let Some(target) = self.target else {
            return;
        };
        self.ping_seq = self.ping_seq.wrapping_add(1);
        let header = Header {
            kind: Kind::Ping,
            channels: 0,
            codec: 0,
            session: self.plan.session,
            seq: self.ping_seq,
            stamp: self.now_us(),
        };
        let name = self.plan.name.clone();
        let bytes = name.as_bytes();
        let cut = bytes.len().min(super::protocol::MAX_NAME_BYTES);
        self.send(&header, &bytes[..cut], target);
    }

    /// Pongs give the round trip; the receiver's pings get a pong.
    fn read_replies(&mut self) {
        let mut buf = [0u8; MAX_PACKET + 1];
        loop {
            let Some(socket) = self.socket.as_ref() else {
                return;
            };
            let (len, from) = match socket.recv_from(&mut buf) {
                Ok(received) => received,
                Err(e) if is_benign(e.kind()) => return,
                Err(_) => return,
            };
            let Ok((header, _)) = decode(&self.plan.key, &buf[..len]) else {
                continue;
            };
            match header.kind {
                // The stamp is our own clock, echoed in a signed packet.
                Kind::Pong if header.stamp <= self.now_us() => {
                    let now = self.now_us();
                    let rtt = now.saturating_sub(header.stamp);
                    self.plan.stats.rtt_us.store(rtt, Ordering::Relaxed);
                    self.last_pong_us = Some(now);
                }
                Kind::Ping => {
                    let pong = Header {
                        kind: Kind::Pong,
                        channels: 0,
                        codec: 0,
                        session: self.plan.session,
                        seq: header.seq,
                        stamp: header.stamp,
                    };
                    self.send(&pong, &[], from);
                }
                _ => {}
            }
        }
    }

    fn update_connected(&self) {
        let connected = self
            .last_pong_us
            .is_some_and(|at| self.now_us().saturating_sub(at) < CONNECTED_WINDOW_US);
        self.plan.stats.connected.store(connected, Ordering::Relaxed);
    }

    fn say_bye(&mut self) {
        let Some(target) = self.target else {
            return;
        };
        let bye = Header {
            kind: Kind::Bye,
            channels: 0,
            codec: 0,
            session: self.plan.session,
            seq: 0,
            stamp: 0,
        };
        self.send(&bye, &[], target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netaudio::protocol::pcm16_decode;

    #[test]
    fn mono_payload_is_the_average_of_left_and_right() {
        let mut frames = [0.0f32; PACKET_FRAMES * STEREO];
        for frame in frames.chunks_exact_mut(2) {
            frame[0] = 0.5;
            frame[1] = 0.1;
        }
        let mut payload = [0u8; MAX_PAYLOAD];
        let len = frames_to_payload(&frames, false, &mut payload);
        assert_eq!(len, PACKET_FRAMES * 2);
        let mut back = [0.0f32; PACKET_FRAMES];
        pcm16_decode(&payload[..len], &mut back);
        assert!(back.iter().all(|s| (s - 0.3).abs() < 1e-4));
        assert_eq!(frames_to_payload(&frames, true, &mut payload), MAX_PAYLOAD);
    }
}
