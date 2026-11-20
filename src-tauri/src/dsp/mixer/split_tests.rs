//! The voice strip's split: Main and the monitors hear its live (fast)
//! signal, Stream hears the full voice-chain signal, both through the same
//! strip settings and fader.
use super::*;
use crate::dsp::test_util::peak;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn tone(len: usize, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| amp * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR as f32).sin())
        .collect()
}

fn mixer_with(setup: impl Fn(&super::params::StripParams)) -> Mixer {
    let params = Arc::new(MixerParams::new(1));
    let s = &params.strips[0];
    s.send_db[BUS_MONITOR].store(0.0, Relaxed);
    setup(s);
    Mixer::new(SR, BLOCK, 1, params)
}

fn mixer() -> Mixer {
    mixer_with(|_| {})
}

/// Run `live` as the strip input and `full` (if any) as the Stream-only input;
/// return the left side of every bus.
fn run(m: &mut Mixer, live: &[f32], full: Option<&[f32]>) -> Vec<Vec<f32>> {
    let mut outs = vec![Vec::new(); NUM_BUSES];
    for (i, chunk) in live.chunks(BLOCK).enumerate() {
        let inputs: [&[f32]; 1] = [chunk];
        let alt = full.map(|f| (0usize, &f[i * BLOCK..i * BLOCK + chunk.len()]));
        m.process_split(&inputs, alt, chunk.len());
        for (bus, out) in outs.iter_mut().enumerate() {
            out.extend_from_slice(m.output(bus).0);
        }
    }
    outs
}

fn tail_level(v: &[f32]) -> f32 {
    peak(&v[v.len() - 4_800..])
}

#[test]
fn stream_hears_the_full_signal_and_the_room_hears_the_live_one() {
    let live = tone(48_000, 0.1);
    let full = tone(48_000, 0.4);
    let mut m = mixer();
    let out = run(&mut m, &live, Some(&full));
    let main = tail_level(&out[BUS_MAIN]);
    let stream = tail_level(&out[BUS_STREAM]);
    let mon = tail_level(&out[BUS_MONITOR]);
    assert!((stream / main - 4.0).abs() < 0.1, "stream {stream} vs main {main}");
    assert!((mon - main).abs() < 0.01, "monitor follows the live signal");
}

#[test]
fn without_a_stream_signal_every_bus_hears_the_strip_as_before() {
    let live = tone(48_000, 0.2);
    let mut split = mixer();
    let a = run(&mut split, &live, None);
    let mut plain = mixer();
    let mut b = vec![Vec::new(); NUM_BUSES];
    for chunk in live.chunks(BLOCK) {
        let inputs: [&[f32]; 1] = [chunk];
        plain.process(&inputs, chunk.len());
        for (bus, out) in b.iter_mut().enumerate() {
            out.extend_from_slice(plain.output(bus).0);
        }
    }
    assert_eq!(a, b);
}

#[test]
fn the_fader_and_mute_still_govern_the_stream_send() {
    let live = tone(48_000, 0.1);
    let full = tone(48_000, 0.4);
    let mut m = mixer_with(|s| s.fader_db.store(-6.0, Relaxed));
    let half = run(&mut m, &live, Some(&full));
    let mut unity = mixer();
    let full_out = run(&mut unity, &live, Some(&full));
    let ratio = tail_level(&half[BUS_STREAM]) / tail_level(&full_out[BUS_STREAM]);
    assert!((ratio - 0.501).abs() < 0.02, "fader not applied to stream: {ratio}");

    let mut muted = mixer_with(|s| s.mute.store(true, Relaxed));
    let out = run(&mut muted, &live, Some(&full));
    assert!(tail_level(&out[BUS_STREAM]) < 1e-4);
}
