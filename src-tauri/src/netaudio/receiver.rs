//! The listening thread (decisions 4–6): one UDP socket, every peer.
//!
//! It checks each datagram's signature, gives each new peer a slot, writes
//! audio into the slot's ring (`stream`), answers pings, pings every peer
//! once a second for the round trip, and frees quiet slots. It owns every
//! ring producer, so each ring stays single-producer.
//!
//! Windows reports an ICMP "port unreachable" from an earlier send as a
//! `ConnectionReset` on the next receive; that is ignored, not fatal. It
//! also fails a receive (WSAEMSGSIZE) instead of truncating a datagram
//! longer than the buffer, so the buffer holds any UDP datagram and an
//! oversized one is just a bad packet.
//!
//! Only signed audio claims a slot, and a session already playing from one
//! address cannot claim a second slot from another (a replayed packet).
use super::protocol::{
    decode, encode, Header, Kind, RoomKey, CODEC_PCM16, MAX_NAME_BYTES, MAX_PACKET, PONG_ACCEPTED,
    PONG_FULL, PONG_RATE_MISMATCH,
};
use super::slots::{Claim, SlotTable};
use super::stream::PeerStream;
use super::{PeerInfo, PeerStats, ReceiverStats};
use ringbuf::HeapProd;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long one receive waits before the thread checks its timers.
const POLL: Duration = Duration::from_millis(5);
const HOUSEKEEPING_MS: u64 = 100;
const PING_INTERVAL_MS: u64 = 1_000;
/// Largest UDP datagram: the receive buffer never truncates.
const MAX_DATAGRAM: usize = 65_536;
/// Windows `WSAEMSGSIZE`: the datagram did not fit the buffer.
const WSAEMSGSIZE: i32 = 10_040;
/// OS receive buffer: about a second of stereo packets, so a burst after
/// a Wi-Fi stall is not dropped (Windows defaults are small).
pub(super) const RECV_BUFFER_BYTES: usize = 1 << 20;
/// Back off only after this many unexpected receive errors in a row.
const ERRORS_BEFORE_BACKOFF: u32 = 3;

/// Everything the receiver thread needs.
pub struct ReceivePlan {
    pub port: u16,
    pub key: RoomKey,
    pub sample_rate: u32,
    /// This engine's own sender session: its packets are never received.
    pub own_session: u32,
    pub producers: Vec<HeapProd<f32>>,
    pub stats: Vec<Arc<PeerStats>>,
    pub socket_stats: Arc<ReceiverStats>,
}

pub struct NetReceiver {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    port: u16,
}

impl NetReceiver {
    /// Bind the port (all IPv4 interfaces) and start the thread.
    pub fn start(plan: ReceivePlan) -> Result<NetReceiver, String> {
        let socket = bind_socket(plan.port)
            .map_err(|e| format!("cannot listen on UDP port {}: {e}", plan.port))?;
        socket
            .set_read_timeout(Some(POLL))
            .map_err(|e| format!("cannot set the receive timeout: {e}"))?;
        let port = socket.local_addr().map(|a| a.port()).unwrap_or(plan.port);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("net-audio-receive".to_string())
            .spawn(move || Receiver::new(socket, plan).run(&thread_stop))
            .map_err(|e| format!("cannot start the network receiver: {e}"))?;
        Ok(NetReceiver {
            stop,
            thread: Some(thread),
            port,
        })
    }

    /// The bound port (useful when started on port 0).
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for NetReceiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Receiver {
    socket: UdpSocket,
    key: RoomKey,
    sample_rate: u32,
    own_session: u32,
    producers: Vec<HeapProd<f32>>,
    stats: Vec<Arc<PeerStats>>,
    socket_stats: Arc<ReceiverStats>,
    streams: Vec<PeerStream>,
    table: SlotTable,
    clock: Instant,
    ping_seq: u32,
    out: [u8; MAX_PACKET],
}

impl Receiver {
    fn new(socket: UdpSocket, plan: ReceivePlan) -> Self {
        let slots = plan.producers.len().min(plan.stats.len());
        Receiver {
            socket,
            key: plan.key,
            sample_rate: plan.sample_rate,
            own_session: plan.own_session,
            producers: plan.producers,
            stats: plan.stats,
            socket_stats: plan.socket_stats,
            streams: (0..slots).map(|_| PeerStream::new(plan.sample_rate)).collect(),
            table: SlotTable::new(slots),
            clock: Instant::now(),
            ping_seq: 0,
            out: [0u8; MAX_PACKET],
        }
    }

    fn now_us(&self) -> u64 {
        self.clock.elapsed().as_micros() as u64
    }

    fn run(mut self, stop: &AtomicBool) {
        // Holds any datagram, so none is truncated (decode rejects the
        // oversized ones).
        let mut buf = vec![0u8; MAX_DATAGRAM];
        let mut last_housekeeping = 0u64;
        let mut last_ping = 0u64;
        let mut logged_error = false;
        let mut errors_in_a_row = 0u32;
        while !stop.load(Ordering::Relaxed) {
            match self.socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    errors_in_a_row = 0;
                    self.handle(&buf[..len], from);
                }
                Err(e) if is_benign(e.kind()) => {}
                Err(e) if e.raw_os_error() == Some(WSAEMSGSIZE) => {
                    self.socket_stats.bad_packets.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    if !logged_error {
                        log::warn!("network audio receive error: {e}");
                        logged_error = true;
                    }
                    errors_in_a_row += 1;
                    if errors_in_a_row >= ERRORS_BEFORE_BACKOFF {
                        std::thread::sleep(POLL);
                    }
                }
            }
            let now_ms = self.now_us() / 1_000;
            if now_ms - last_housekeeping >= HOUSEKEEPING_MS {
                last_housekeeping = now_ms;
                self.expire(now_ms);
            }
            if now_ms - last_ping >= PING_INTERVAL_MS {
                last_ping = now_ms;
                self.ping_peers();
            }
        }
    }

    fn handle(&mut self, packet: &[u8], from: SocketAddr) {
        let Ok((header, payload)) = decode(&self.key, packet) else {
            self.socket_stats.bad_packets.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if header.session == self.own_session {
            return;
        }
        match header.kind {
            Kind::Audio => self.on_audio(&header, payload, from),
            Kind::Ping => self.on_ping(&header, payload, from),
            Kind::Pong => self.on_pong(&header, from),
            Kind::Bye => {
                if let Some(slot) = self.table.release(from, header.session) {
                    self.stats[slot].connected.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    /// The slot for this peer, resetting it when the peer is new. A
    /// session already playing from another address is a replay: refused.
    fn claim(&mut self, header: &Header, from: SocketAddr) -> Option<usize> {
        let now_ms = self.now_us() / 1_000;
        if self.table.session_elsewhere(from, header.session) {
            self.socket_stats.refused_packets.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        match self.table.claim(from, header.session, now_ms) {
            Claim::Existing(slot) => Some(slot),
            Claim::New(slot) => {
                self.streams[slot] = PeerStream::new(self.sample_rate);
                self.stats[slot].reset_for(from.to_string());
                log::info!("network audio peer {from} on Remote {}", slot + 1);
                Some(slot)
            }
            Claim::Full => {
                self.socket_stats.refused_packets.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    fn on_audio(&mut self, header: &Header, payload: &[u8], from: SocketAddr) {
        let Some(slot) = self.claim(header, from) else {
            return;
        };
        let stats = &self.stats[slot];
        let rate = header.stamp.min(u32::MAX as u64) as u32;
        stats.sender_rate.store(rate, Ordering::Relaxed);
        if rate != self.sample_rate || header.codec != CODEC_PCM16 {
            stats.rate_mismatch_packets.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let arrival_us = self.clock.elapsed().as_micros() as f64;
        let report = self.streams[slot].push(
            header.seq,
            header.channels as usize,
            payload,
            arrival_us,
            &mut self.producers[slot],
        );
        if report.lost_packets > 0 {
            stats.lost_packets.fetch_add(report.lost_packets as u64, Ordering::Relaxed);
        }
        if report.late {
            stats.late_packets.fetch_add(1, Ordering::Relaxed);
        }
        if report.dropped_samples > 0 {
            stats.dropped_samples.fetch_add(report.dropped_samples as u64, Ordering::Relaxed);
        }
        let jitter = self.streams[slot].jitter_us();
        stats.jitter_us.store(jitter as u64, Ordering::Relaxed);
    }

    /// A sender's ping names it; the pong says whether it is played. A ping
    /// never claims a slot: only audio does.
    fn on_ping(&mut self, header: &Header, payload: &[u8], from: SocketAddr) {
        let slot = self.table.find(from, header.session);
        if let Some(slot) = slot {
            let name = clean_name(payload);
            if !name.is_empty() {
                self.stats[slot].set_name(name);
            }
        }
        let verdict = match slot {
            Some(slot) => {
                let rate = self.stats[slot].sender_rate.load(Ordering::Relaxed);
                if rate != 0 && rate != self.sample_rate {
                    PONG_RATE_MISMATCH
                } else {
                    PONG_ACCEPTED
                }
            }
            None if self.table.is_full() => PONG_FULL,
            None => PONG_ACCEPTED,
        };
        let pong = Header {
            kind: Kind::Pong,
            channels: 0,
            codec: verdict,
            session: self.own_session,
            seq: header.seq,
            stamp: header.stamp,
        };
        self.send(&pong, &[], from);
    }

    /// The answer to our ping: the round trip to that peer.
    fn on_pong(&mut self, header: &Header, from: SocketAddr) {
        if let Some(slot) = self.table.find(from, header.session) {
            let rtt = self.now_us().saturating_sub(header.stamp);
            self.stats[slot].rtt_us.store(rtt, Ordering::Relaxed);
        }
    }

    fn ping_peers(&mut self) {
        self.ping_seq = self.ping_seq.wrapping_add(1);
        let ping = Header {
            kind: Kind::Ping,
            channels: 0,
            codec: 0,
            session: self.own_session,
            seq: self.ping_seq,
            stamp: self.now_us(),
        };
        let peers: Vec<SocketAddr> = self.table.held().map(|(_, addr, _)| addr).collect();
        for addr in peers {
            self.send(&ping, &[], addr);
        }
    }

    fn send(&mut self, header: &Header, payload: &[u8], to: SocketAddr) {
        if let Ok(len) = encode(&self.key, header, payload, &mut self.out) {
            // A lost ping or pong only delays the next measurement.
            let _ = self.socket.send_to(&self.out[..len], to);
        }
    }

    fn expire(&mut self, now_ms: u64) {
        let stats = &self.stats;
        self.table.expire(now_ms, |slot| {
            stats[slot].connected.store(false, Ordering::Relaxed);
            stats[slot].set_info(PeerInfo::default());
        });
    }
}

/// A UDP socket on `port` (all IPv4 interfaces) with a large receive
/// buffer. A buffer the OS refuses to grow is logged, not fatal.
pub(super) fn bind_socket(port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    if let Err(err) = socket.set_recv_buffer_size(RECV_BUFFER_BYTES) {
        log::warn!("network audio receive buffer left at the OS default: {err}");
    }
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    socket.bind(&addr.into())?;
    Ok(socket.into())
}

/// Receive errors that only mean "nothing yet" or a stale ICMP report.
pub(super) fn is_benign(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset | ErrorKind::Interrupted
    )
}

/// A peer name from a ping payload: UTF-8 (lossy), no control characters,
/// trimmed, at most `MAX_NAME_CHARS` characters.
pub(super) fn clean_name(payload: &[u8]) -> String {
    let bytes = &payload[..payload.len().min(MAX_NAME_BYTES)];
    let text = String::from_utf8_lossy(bytes);
    let kept: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(super::settings::MAX_NAME_CHARS)
        .collect();
    kept.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listening_socket_has_room_for_a_burst() {
        let socket = bind_socket(0).unwrap();
        let size = socket2::SockRef::from(&socket).recv_buffer_size().unwrap();
        assert!(size >= RECV_BUFFER_BYTES / 2, "receive buffer only {size} bytes");
    }
}
