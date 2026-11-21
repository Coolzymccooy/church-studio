//! The real audio callback with network audio: a Remote strip's peer audio
//! reaches Main, and the send bus reaches the send ring.
use super::input_proc::{HardwareInput, InputProcessor, NetIo};
use super::{EngineShared, MAX_CALLBACK_FRAMES};
use crate::dsp::mixer::BUS_MAIN;
use crate::dsp::{DspChain, DspParams};
use crate::mixer_control::MixerControl;
use crate::mixer_meters::MixerMeterSlots;
use crate::netaudio::{ring_capacity, NetInputFeed, NetSendRoute, PeerStats, SenderStats};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::{mpsc, Arc};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

struct Rig {
    processor: InputProcessor<HeapProd<f32>>,
    peer: HeapProd<f32>,
    sent: HeapCons<f32>,
    mixer: MixerControl,
}

fn rig() -> Rig {
    let mixer = MixerControl::new();
    let (peer, peer_ring) = HeapRb::<f32>::new(ring_capacity(RATE)).split();
    let (send_ring, sent) = HeapRb::<f32>::new(ring_capacity(RATE)).split();
    let feed = NetInputFeed::new(peer_ring, RATE, 20, Arc::new(PeerStats::default()));
    let (record_tap, _port) = crate::recorder::record_ring(2, RATE, MAX_CALLBACK_FRAMES);
    let (meters_tx, _meters_rx) = mpsc::sync_channel(32);
    let processor = InputProcessor::new(
        RATE,
        HardwareInput::all(1),
        DspChain::new(RATE as f64),
        Arc::new(DspParams::defaults()),
        Arc::new(EngineShared::new(RATE as usize * 2)),
        mixer.link(),
        Arc::new(MixerMeterSlots::new(2)),
        Vec::new(),
        Vec::new(),
        None,
        Vec::new(),
        NetIo {
            inputs: vec![feed],
            send: Some(NetSendRoute {
                producer: send_ring,
                bus: BUS_MAIN,
                stats: Arc::new(SenderStats::default()),
            }),
        },
        record_tap,
        crate::preview::preview_ring(48_000).0,
        meters_tx,
    );
    Rig { processor, peer, sent, mixer }
}

/// Run `blocks` callbacks of silent hardware input while the peer sends a
/// 440 Hz tone; returns the peak of what reached the send ring (Main).
fn run(rig: &mut Rig, blocks: usize) -> f32 {
    let silence = vec![0.0f32; BLOCK];
    let step = 2.0 * std::f32::consts::PI * 440.0 / RATE as f32;
    let mut phase = 0usize;
    let mut peak = 0.0f32;
    let mut sent = vec![0.0f32; BLOCK * 2];
    for _ in 0..blocks {
        for _ in 0..BLOCK {
            let sample = (phase as f32 * step).sin() * 0.5;
            phase += 1;
            let _ = rig.peer.try_push(sample);
            let _ = rig.peer.try_push(sample);
        }
        rig.processor.process(&silence, |s| s);
        while rig.sent.occupied_len() >= sent.len() {
            rig.sent.pop_slice(&mut sent);
            peak = sent.iter().fold(peak, |p, s| p.max(s.abs()));
        }
    }
    peak
}

#[test]
fn a_new_remote_strip_starts_fader_down() {
    let mut rig = rig();
    assert!(run(&mut rig, 60) < 1e-3, "a Remote strip must not reach Main before its fader is up");
}

#[test]
fn remote_audio_reaches_main_and_the_send_ring() {
    let mut rig = rig();
    rig.mixer.set_strip_param(1, "fader_db", 0.0).unwrap();
    let peak = run(&mut rig, 60);
    assert!(peak > 0.1, "Remote strip did not reach Main (peak {peak})");
}
