//! Loopback integration: real sender and receiver threads on 127.0.0.1,
//! and block-size invariance of a Remote strip's output.
use super::protocol::{RoomKey, PACKET_FRAMES};
use super::receiver::{NetReceiver, ReceivePlan};
use super::sender::{NetSender, SendPlan};
use super::stream::PeerStream;
use super::*;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    done()
}

struct Loopback {
    receiver: NetReceiver,
    peer: Arc<PeerStats>,
    peer_ring: HeapCons<f32>,
    socket_stats: Arc<ReceiverStats>,
}

fn listen(passphrase: &str, own_session: u32) -> Loopback {
    let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(RATE)).split();
    let peer = Arc::new(PeerStats::default());
    let socket_stats = Arc::new(ReceiverStats::default());
    let receiver = NetReceiver::start(ReceivePlan {
        port: 0,
        key: RoomKey::from_passphrase(passphrase),
        sample_rate: RATE,
        own_session,
        producers: vec![producer],
        stats: vec![peer.clone()],
        socket_stats: socket_stats.clone(),
    })
    .unwrap();
    Loopback {
        receiver,
        peer,
        peer_ring: consumer,
        socket_stats,
    }
}

fn send_to(port: u16, passphrase: &str, session: u32) -> (NetSender, HeapProd<f32>, Arc<SenderStats>) {
    send_at(port, passphrase, session, RATE)
}

fn send_at(
    port: u16,
    passphrase: &str,
    session: u32,
    rate: u32,
) -> (NetSender, HeapProd<f32>, Arc<SenderStats>) {
    let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(rate)).split();
    let stats = Arc::new(SenderStats::default());
    let sender = NetSender::start(SendPlan {
        target: format!("127.0.0.1:{port}"),
        name: "Pastor laptop".to_string(),
        stereo: false,
        key: RoomKey::from_passphrase(passphrase),
        sample_rate: rate,
        session,
        consumer,
        stats: stats.clone(),
    });
    (sender, producer, stats)
}

fn push_tone(producer: &mut HeapProd<f32>, frames: usize, value: f32) {
    for _ in 0..frames {
        let _ = producer.try_push(value);
        let _ = producer.try_push(value);
    }
}

#[test]
fn audio_arrives_in_the_peer_ring_with_name_and_round_trip() {
    let mut rx = listen("loopback-room", 1);
    let (sender, mut producer, sender_stats) = send_to(rx.receiver.port(), "loopback-room", 2);
    push_tone(&mut producer, PACKET_FRAMES * 10, 0.5);
    assert!(wait_until(Duration::from_secs(3), || rx.peer_ring.occupied_len()
        >= PACKET_FRAMES * 10 * STEREO));
    let mut got = vec![0.0f32; PACKET_FRAMES * 10 * STEREO];
    rx.peer_ring.pop_slice(&mut got);
    assert!(got.iter().all(|s| (s - 0.5).abs() < 1e-3));
    assert!(rx.peer.connected.load(Ordering::Relaxed));
    assert_eq!(rx.peer.lost_packets.load(Ordering::Relaxed), 0);
    assert!(wait_until(Duration::from_secs(3), || rx.peer.info().name == "Pastor laptop"));
    // Both sides measure a round trip from the pings.
    assert!(wait_until(Duration::from_secs(3), || sender_stats.connected.load(Ordering::Relaxed)));
    assert!(wait_until(Duration::from_secs(3), || rx.peer.rtt_us.load(Ordering::Relaxed) > 0));
    assert!(sender_stats.packets_sent.load(Ordering::Relaxed) >= 10);
    // Stopping the sender says bye and frees the slot.
    drop(sender);
    assert!(wait_until(Duration::from_secs(2), || !rx.peer.connected.load(Ordering::Relaxed)));
}

#[test]
fn a_sender_with_another_passphrase_is_ignored() {
    let rx = listen("room-one-pass", 1);
    let (_sender, mut producer, _) = send_to(rx.receiver.port(), "room-two-pass", 2);
    push_tone(&mut producer, PACKET_FRAMES * 4, 0.5);
    assert!(wait_until(Duration::from_secs(2), || rx
        .socket_stats
        .bad_packets
        .load(Ordering::Relaxed)
        >= 4));
    assert_eq!(rx.peer_ring.occupied_len(), 0);
    assert!(!rx.peer.connected.load(Ordering::Relaxed));
}

#[test]
fn the_engine_never_receives_its_own_sender() {
    let rx = listen("same-engine-room", 7);
    let (_sender, mut producer, _) = send_to(rx.receiver.port(), "same-engine-room", 7);
    push_tone(&mut producer, PACKET_FRAMES * 4, 0.5);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(rx.peer_ring.occupied_len(), 0);
}

/// A UDP port nothing listens on right now.
fn free_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap().port()
}

#[test]
fn every_send_bus_name_maps_to_its_mixer_bus() {
    use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, BUS_STREAM};
    assert_eq!(bus_index("main"), BUS_MAIN);
    assert_eq!(bus_index("stream"), BUS_STREAM);
    assert_eq!(bus_index("monitor"), BUS_MONITOR);
    assert_eq!(bus_index("monitor2"), BUS_MONITOR2);
    assert_eq!(bus_index("monitor3"), BUS_MONITOR3);
    for name in settings::BUSES {
        assert!(crate::mixer_control::BUS_IDS.contains(&name), "{name}");
    }
}

#[test]
fn start_reserves_strips_only_when_receiving() {
    let mut config = NetAudioConfig::fresh();
    let started = start(&config, RATE, 4, 4);
    assert!(started.feeds.is_empty() && started.info.inputs.is_empty());
    assert!(started.send_route.is_none());

    config.receive.enabled = true;
    config.receive.port = free_port();
    config.receive.slots = 3;
    let started = start(&config, RATE, 4, 2);
    assert_eq!(started.feeds.len(), 2, "capped by the strips that fit");
    assert_eq!(started.info.inputs[0].strip, 4);
    assert_eq!(started.info.inputs[1].label, "Remote 2");

    config.passphrase = "short".to_string();
    let started = start(&config, RATE, 4, 4);
    assert!(started.feeds.is_empty());
    assert!(started.info.receive_error.is_some());
}

/// Run `input` through a PeerStream and a Remote strip's feed with blocks
/// of `block` frames, keeping the ring at the buffer target before every
/// read (so the drift nudge stays idle).
fn strip_output(input: &[f32], block: usize, buffer_ms: u32) -> Vec<f32> {
    // Every packet through the receiver-side stream, staged as stereo.
    let (mut stage_prod, mut stage_cons) = HeapRb::<f32>::new(input.len() * STEREO + 16).split();
    let mut stream = PeerStream::new(RATE);
    for (seq, chunk) in input.chunks(PACKET_FRAMES).enumerate() {
        let mut bytes = vec![0u8; chunk.len() * 2];
        protocol::pcm16_encode(chunk, &mut bytes);
        stream.push(seq as u32, 1, &bytes, 0.0, &mut stage_prod);
    }
    let mut staged = vec![0.0f32; stage_cons.occupied_len()];
    stage_cons.pop_slice(&mut staged);

    let (mut producer, consumer) = HeapRb::<f32>::new(staged.len() + 16).split();
    let stats = Arc::new(PeerStats::default());
    let mut feed = NetInputFeed::new(consumer, RATE, buffer_ms, stats);
    let target = crate::ndi::receive::drift::frames_for_ms(RATE, buffer_ms as usize);
    let mut next = 0usize;
    let mut move_frames = |frames: usize, producer: &mut HeapProd<f32>| {
        let end = (next + frames * STEREO).min(staged.len());
        producer.push_slice(&staged[next..end]);
        next = end;
    };
    // Exactly `target` frames before the first read.
    move_frames(target, &mut producer);
    let total = input.len() - target - 2;
    let mut out = Vec::new();
    let mut buf = vec![0.0f32; block];
    let mut first = true;
    while out.len() + block <= total {
        feed.pull(&mut buf);
        out.extend_from_slice(&buf);
        // Refill what this read consumed (plus the two primed frames once).
        move_frames(block + if first { 2 } else { 0 }, &mut producer);
        first = false;
    }
    out
}

#[test]
fn a_remote_strip_sounds_the_same_at_any_block_size() {
    let input: Vec<f32> = (0..PACKET_FRAMES * 40)
        .map(|i| ((i as f32) * 0.013).sin() * 0.5)
        .collect();
    let reference = strip_output(&input, 64, 20);
    assert!(reference.len() > 4_000);
    // Pass-through: the strip plays the packets' samples unchanged.
    for (a, b) in reference.iter().zip(input.iter()) {
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
    }
    for block in [32, 37, 128, 480] {
        let other = strip_output(&input, block, 20);
        let n = other.len().min(reference.len());
        assert_eq!(&other[..n], &reference[..n], "block {block}");
    }
}

#[test]
fn a_burst_after_an_underrun_drains_back_to_the_buffer_target() {
    // Late packets often arrive together after a dropout, leaving the ring
    // well above its target. Latency must come back down within seconds.
    let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(RATE) * 2).split();
    let stats = Arc::new(PeerStats::default());
    let mut feed = NetInputFeed::new(consumer, RATE, 20, stats.clone());
    let frames_per_ms = RATE as usize / 1_000;
    push_tone(&mut producer, frames_per_ms * 60, 0.1);
    let block = 256;
    let mut out = vec![0.0f32; block];
    let seconds = 12;
    for _ in 0..(RATE as usize * seconds / block) {
        push_tone(&mut producer, block, 0.1);
        feed.pull(&mut out);
    }
    let fill_ms = stats.fill_frames.load(Ordering::Relaxed) as usize / frames_per_ms;
    assert!(fill_ms <= 28, "still {fill_ms} ms buffered after {seconds} s");
    assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
}

#[test]
fn a_sender_learns_when_the_room_is_full() {
    let rx = listen("full-room-pass", 1);
    let (_first, mut first_audio, first) = send_to(rx.receiver.port(), "full-room-pass", 2);
    push_tone(&mut first_audio, PACKET_FRAMES * 4, 0.5);
    assert!(wait_until(Duration::from_secs(3), || first.connected.load(Ordering::Relaxed)));
    let (_second, mut second_audio, second) = send_to(rx.receiver.port(), "full-room-pass", 3);
    push_tone(&mut second_audio, PACKET_FRAMES * 4, 0.5);
    assert!(wait_until(Duration::from_secs(4), || second.answered.load(Ordering::Relaxed)
        && second.verdict.load(Ordering::Relaxed) == protocol::PONG_FULL));
    assert!(!second.connected.load(Ordering::Relaxed));
    assert_eq!(sender_status(&second).refused, Some("full"));
}

#[test]
fn a_sender_at_another_sample_rate_is_told_so() {
    let rx = listen("rate-room-pass", 1);
    let (_sender, mut audio, stats) = send_at(rx.receiver.port(), "rate-room-pass", 2, 44_100);
    // Keep audio flowing until the next ping's pong reports the mismatch.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(4)
        && stats.verdict.load(Ordering::Relaxed) != protocol::PONG_RATE_MISMATCH
    {
        push_tone(&mut audio, PACKET_FRAMES, 0.5);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(stats.verdict.load(Ordering::Relaxed), protocol::PONG_RATE_MISMATCH);
    assert!(rx.peer.rate_mismatch_packets.load(Ordering::Relaxed) > 0);
    assert_eq!(rx.peer_ring.occupied_len(), 0);
}

#[test]
fn a_new_peer_never_hears_the_previous_peers_leftovers() {
    let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(RATE)).split();
    let stats = Arc::new(PeerStats::default());
    let mut feed = NetInputFeed::new(consumer, RATE, 20, stats.clone());
    let mut out = vec![0.0f32; 128];
    // The old peer leaves a partial buffer (under target: the reader waits).
    push_tone(&mut producer, 300, 0.7);
    feed.pull(&mut out);
    // The slot changes hands; the new peer's audio follows.
    stats.generation.fetch_add(1, Ordering::Release);
    let mut heard = Vec::new();
    for _ in 0..40 {
        push_tone(&mut producer, 128, 0.2);
        feed.pull(&mut out);
        heard.extend_from_slice(&out);
    }
    assert!(heard.iter().all(|s| (s - 0.7).abs() > 0.1), "old audio played");
    assert!(heard.iter().any(|s| (s - 0.2).abs() < 1e-3), "new audio never played");
}
