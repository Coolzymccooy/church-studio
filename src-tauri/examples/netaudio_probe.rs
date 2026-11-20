//! Two-machine test for network audio (spec test plan).
//!
//! Uses the real sender, receiver, packet format and jitter buffer; a
//! real-time clock stands in for the sound card (128-frame blocks at 48 kHz).
//!
//!   echo:    receive on --port, play out through the jitter buffer, and send
//!            what plays straight back to --to.
//!   measure: send a click every 500 ms to --to, receive the echo on --port
//!            through its own jitter buffer, and time each click's round trip.
//!            Half of it is the one-way audio latency (packet, network,
//!            jitter buffer, block), set against the ping estimate.
//!   send / recv: one direction only, with a tone, printing peer status.
//!
//! cargo run --example netaudio_probe -- echo    --port 47811 --to MAC:47812 --pass PASS
//! cargo run --example netaudio_probe -- measure --port 47812 --to WIN:47811 --pass PASS
use app_lib::netaudio::protocol::RoomKey;
use app_lib::netaudio::receiver::{NetReceiver, ReceivePlan};
use app_lib::netaudio::sender::{NetSender, SendPlan};
use app_lib::netaudio::{
    new_session_id, ring_capacity, NetInputFeed, PeerStats, ReceiverStats, SenderStats,
};
use ringbuf::traits::{Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
const CLICK_EVERY: usize = RATE as usize / 2;
const CLICK_LEVEL: f32 = 0.9;
const DETECT_LEVEL: f32 = 0.3;

struct Args {
    mode: String,
    port: u16,
    to: String,
    pass: String,
    buffer_ms: u32,
    seconds: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut raw = std::env::args().skip(1);
    let mode = raw.next().ok_or("mode: echo | measure | send | recv")?;
    let mut args = Args {
        mode,
        port: 47_811,
        to: String::new(),
        pass: String::new(),
        buffer_ms: 20,
        seconds: 20,
    };
    while let Some(flag) = raw.next() {
        let value = raw.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--port" => args.port = value.parse().map_err(|e| format!("--port: {e}"))?,
            "--to" => args.to = value,
            "--pass" => args.pass = value,
            "--buffer-ms" => args.buffer_ms = value.parse().map_err(|e| format!("--buffer-ms: {e}"))?,
            "--seconds" => args.seconds = value.parse().map_err(|e| format!("--seconds: {e}"))?,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if args.pass.len() < 8 {
        return Err("--pass needs at least 8 characters".to_string());
    }
    Ok(args)
}

/// Calls `block` once per due block of `BLOCK` frames, paced by the clock.
struct Pacer {
    start: Instant,
    done_frames: u64,
}

impl Pacer {
    fn new() -> Self {
        Pacer { start: Instant::now(), done_frames: 0 }
    }

    /// Run every block that is due now; returns false once `limit` passed.
    fn tick(&mut self, limit: Duration, mut block: impl FnMut(u64)) -> bool {
        let elapsed = self.start.elapsed();
        let due = (elapsed.as_secs_f64() * RATE as f64) as u64;
        while self.done_frames + BLOCK as u64 <= due {
            block(self.done_frames);
            self.done_frames += BLOCK as u64;
        }
        std::thread::sleep(Duration::from_micros(500));
        elapsed < limit
    }
}

struct Listener {
    _receiver: NetReceiver,
    peer: Arc<PeerStats>,
    socket: Arc<ReceiverStats>,
    feed: NetInputFeed<HeapCons<f32>>,
}

fn listen(args: &Args, key: &RoomKey, session: u32) -> Result<Listener, String> {
    let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(RATE)).split();
    let peer = Arc::new(PeerStats::default());
    let socket = Arc::new(ReceiverStats::default());
    let receiver = NetReceiver::start(ReceivePlan {
        port: args.port,
        key: key.clone(),
        sample_rate: RATE,
        own_session: session,
        producers: vec![producer],
        stats: vec![peer.clone()],
        socket_stats: socket.clone(),
    })?;
    let feed = NetInputFeed::new(consumer, RATE, args.buffer_ms, peer.clone());
    Ok(Listener { _receiver: receiver, peer, socket, feed })
}

fn send(args: &Args, key: &RoomKey, session: u32, name: &str) -> (NetSender, HeapProd<f32>, Arc<SenderStats>) {
    let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(RATE)).split();
    let stats = Arc::new(SenderStats::default());
    let sender = NetSender::start(SendPlan {
        target: args.to.clone(),
        name: name.to_string(),
        stereo: false,
        key: key.clone(),
        sample_rate: RATE,
        session,
        consumer,
        stats: stats.clone(),
    });
    (sender, producer, stats)
}

fn push_mono(producer: &mut HeapProd<f32>, block: &[f32]) {
    for &sample in block {
        let _ = producer.try_push(sample);
        let _ = producer.try_push(sample);
    }
}

fn print_peer(label: &str, listener: &Listener) {
    let peer = &listener.peer;
    let info = peer.info();
    println!(
        "{label}: peer '{}' {} | rtt {:.1} ms | jitter {:.2} ms | buffer {:.1} ms | lost {} late {} underruns {} | bad {}",
        info.name,
        info.address,
        peer.rtt_us.load(Ordering::Relaxed) as f64 / 1000.0,
        peer.jitter_us.load(Ordering::Relaxed) as f64 / 1000.0,
        peer.fill_frames.load(Ordering::Relaxed) as f64 * 1000.0 / RATE as f64,
        peer.lost_packets.load(Ordering::Relaxed),
        peer.late_packets.load(Ordering::Relaxed),
        peer.underruns.load(Ordering::Relaxed),
        listener.socket.bad_packets.load(Ordering::Relaxed),
    );
}

fn run_echo(args: &Args) -> Result<(), String> {
    let key = RoomKey::from_passphrase(&args.pass);
    let session = new_session_id();
    let mut listener = listen(args, &key, session)?;
    let (_sender, mut producer, _stats) = send(args, &key, new_session_id(), "probe echo");
    println!("echo: listening on UDP {}, echoing to {}", args.port, args.to);
    let mut pacer = Pacer::new();
    let mut buf = [0.0f32; BLOCK];
    let mut last_print = Instant::now();
    while pacer.tick(Duration::from_secs(args.seconds), |_| {
        listener.feed.pull(&mut buf);
        push_mono(&mut producer, &buf);
    }) {
        if last_print.elapsed() >= Duration::from_secs(2) {
            last_print = Instant::now();
            print_peer("echo", &listener);
        }
    }
    print_peer("echo final", &listener);
    Ok(())
}

fn run_measure(args: &Args) -> Result<(), String> {
    let key = RoomKey::from_passphrase(&args.pass);
    let mut listener = listen(args, &key, new_session_id())?;
    let (_sender, mut producer, sender_stats) = send(args, &key, new_session_id(), "probe measure");
    println!("measure: clicks to {}, echo back on UDP {}", args.to, args.port);
    let mut pacer = Pacer::new();
    let mut out = [0.0f32; BLOCK];
    let mut sent_clicks: Vec<u64> = Vec::new();
    let mut round_trips_ms: Vec<f64> = Vec::new();
    let mut refractory_until = 0u64;
    let mut last_print = Instant::now();
    while pacer.tick(Duration::from_secs(args.seconds), |frame| {
        // Outgoing block: a one-sample click every CLICK_EVERY frames.
        let mut block = [0.0f32; BLOCK];
        for (offset, sample) in block.iter_mut().enumerate() {
            let at = frame + offset as u64;
            if at % CLICK_EVERY as u64 == 0 && at > 0 {
                *sample = CLICK_LEVEL;
                sent_clicks.push(at);
            }
        }
        push_mono(&mut producer, &block);
        // Incoming block: the echo, through this side's jitter buffer.
        listener.feed.pull(&mut out);
        for (offset, sample) in out.iter().enumerate() {
            let at = frame + offset as u64;
            if *sample > DETECT_LEVEL && at >= refractory_until {
                refractory_until = at + CLICK_EVERY as u64 / 2;
                if let Some(sent) = sent_clicks.iter().rev().find(|&&s| s <= at) {
                    round_trips_ms.push((at - sent) as f64 * 1000.0 / RATE as f64);
                }
            }
        }
    }) {
        if last_print.elapsed() >= Duration::from_secs(2) {
            last_print = Instant::now();
            print_peer("measure", &listener);
            if let Some(last) = round_trips_ms.last() {
                println!("  last click round trip {last:.1} ms");
            }
        }
    }
    print_peer("measure final", &listener);
    report(&round_trips_ms, &listener, &sender_stats, args.buffer_ms);
    Ok(())
}

fn report(round_trips_ms: &[f64], listener: &Listener, sender: &SenderStats, buffer_ms: u32) {
    // Skip the first two clicks while the buffers settle.
    let settled: Vec<f64> = round_trips_ms.iter().skip(2).copied().collect();
    if settled.is_empty() {
        println!("RESULT: no clicks came back");
        return;
    }
    let mut sorted = settled.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let ping_ms = sender.rtt_us.load(Ordering::Relaxed) as f64 / 1000.0;
    println!(
        "RESULT: {} clicks; audio round trip median {median:.1} ms (min {min:.1}, max {max:.1}); \
         one-way audio latency ≈ {:.1} ms; network ping {ping_ms:.1} ms; buffer target {buffer_ms} ms; \
         lost packets here {}",
        settled.len(),
        median / 2.0,
        listener.peer.lost_packets.load(Ordering::Relaxed),
    );
}

fn run_send(args: &Args) -> Result<(), String> {
    let key = RoomKey::from_passphrase(&args.pass);
    let (_sender, mut producer, stats) = send(args, &key, new_session_id(), "probe tone");
    println!("send: 440 Hz tone to {}", args.to);
    let mut pacer = Pacer::new();
    let mut last_print = Instant::now();
    let step = 2.0 * std::f32::consts::PI * 440.0 / RATE as f32;
    while pacer.tick(Duration::from_secs(args.seconds), |frame| {
        let mut block = [0.0f32; BLOCK];
        for (offset, sample) in block.iter_mut().enumerate() {
            *sample = ((frame as usize + offset) as f32 * step).sin() * 0.25;
        }
        push_mono(&mut producer, &block);
    }) {
        if last_print.elapsed() >= Duration::from_secs(2) {
            last_print = Instant::now();
            println!(
                "send: connected {} | rtt {:.1} ms | sent {} | errors {}",
                stats.connected.load(Ordering::Relaxed),
                stats.rtt_us.load(Ordering::Relaxed) as f64 / 1000.0,
                stats.packets_sent.load(Ordering::Relaxed),
                stats.send_errors.load(Ordering::Relaxed),
            );
        }
    }
    Ok(())
}

fn run_recv(args: &Args) -> Result<(), String> {
    let key = RoomKey::from_passphrase(&args.pass);
    let mut listener = listen(args, &key, new_session_id())?;
    println!("recv: listening on UDP {}", args.port);
    let mut pacer = Pacer::new();
    let mut buf = [0.0f32; BLOCK];
    let mut peak = 0.0f32;
    let mut last_print = Instant::now();
    while pacer.tick(Duration::from_secs(args.seconds), |_| {
        listener.feed.pull(&mut buf);
        peak = buf.iter().fold(peak, |p, s| p.max(s.abs()));
    }) {
        if last_print.elapsed() >= Duration::from_secs(2) {
            last_print = Instant::now();
            print_peer("recv", &listener);
            println!("  peak {peak:.3}");
            peak = 0.0;
        }
    }
    Ok(())
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let result = match args.mode.as_str() {
        "echo" => run_echo(&args),
        "measure" => run_measure(&args),
        "send" => run_send(&args),
        "recv" => run_recv(&args),
        other => Err(format!("unknown mode {other}")),
    };
    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
