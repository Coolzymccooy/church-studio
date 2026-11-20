//! Tests for the live (fast) voice path.
use super::*;
use crate::dsp::{DspChain, DspParams};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SR: f64 = 48_000.0;

fn params(all_on: bool) -> Arc<DspParams> {
    let p = Arc::new(DspParams::default());
    p.gate_enabled.store(all_on, Relaxed);
    p.neural_enabled.store(all_on, Relaxed);
    p.noise_enabled.store(all_on, Relaxed);
    p.dereverb_enabled.store(all_on, Relaxed);
    p.deess_enabled.store(all_on, Relaxed);
    p.comp_enabled.store(all_on, Relaxed);
    p
}

/// First index where |x| crosses `level`, or None.
fn onset(buf: &[f32], level: f32) -> Option<usize> {
    buf.iter().position(|s| s.abs() > level)
}

/// A click after `at` samples of silence, then a steady tone the gate keeps open.
fn click_then_tone(len: usize, at: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            if i < at {
                0.0
            } else {
                let t = (i - at) as f32 / SR as f32;
                0.5 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            }
        })
        .collect()
}

#[test]
fn live_path_waits_at_most_the_short_gate_lookahead() {
    let p = params(true);
    let mut fast = FastVoiceChain::new(SR);
    fast.sync_params(&p);
    let limit = (SR * FAST_GATE_LOOKAHEAD_MS / 1000.0) as usize;
    assert!(fast.latency_samples() <= limit, "{} > {limit}", fast.latency_samples());

    p.gate_enabled.store(false, Relaxed);
    fast.sync_params(&p);
    assert_eq!(fast.latency_samples(), 0, "no lookahead with the gate off");
}

#[test]
fn full_chain_with_everything_on_is_far_slower_than_the_live_path() {
    let p = params(true);
    let mut full = DspChain::new(SR);
    full.sync_params(&p);
    let mut fast = FastVoiceChain::new(SR);
    fast.sync_params(&p);
    // The whole point of the split: the room path is at least 40 ms quicker.
    let saved_ms = (full.total_latency_samples() - fast.latency_samples()) as f64 * 1000.0 / SR;
    assert!(saved_ms >= 40.0, "only {saved_ms:.1} ms quicker");
}

#[test]
fn live_path_passes_sound_promptly_with_the_gate_off() {
    let p = params(false);
    let mut fast = FastVoiceChain::new(SR);
    fast.sync_params(&p);
    let mut buf = click_then_tone(4_800, 1_000);
    for chunk in buf.chunks_mut(256) {
        fast.process_block(chunk, false, 1.0);
    }
    let at = onset(&buf, 0.01).expect("sound comes through");
    // The HPF and EQ add phase delay only, well under 1 ms.
    assert!(at <= 1_000 + 48, "onset at {at}");
}

#[test]
fn live_path_output_is_finite_and_follows_the_shared_level() {
    let p = params(true);
    let mut fast = FastVoiceChain::new(SR);
    fast.sync_params(&p);
    let mut quiet = click_then_tone(9_600, 0);
    let mut loud = quiet.clone();
    let mut fast2 = FastVoiceChain::new(SR);
    fast2.sync_params(&p);
    for (a, b) in quiet.chunks_mut(256).zip(loud.chunks_mut(256)) {
        fast.process_block(a, false, 0.5);
        fast2.process_block(b, false, 1.0);
    }
    assert!(quiet.iter().chain(loud.iter()).all(|s| s.is_finite()));
    let peak = |b: &[f32]| b[4_800..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let ratio = peak(&quiet) / peak(&loud);
    assert!((ratio - 0.5).abs() < 0.02, "auto-gain not applied: {ratio}");
}

#[test]
fn bypass_leaves_the_block_untouched_and_nan_gain_is_ignored() {
    let p = params(true);
    let mut fast = FastVoiceChain::new(SR);
    fast.sync_params(&p);
    let original = click_then_tone(512, 0);
    let mut buf = original.clone();
    fast.process_block(&mut buf, true, 1.0);
    assert_eq!(buf, original);
    let mut buf = original.clone();
    fast.process_block(&mut buf, false, f32::NAN);
    assert!(buf.iter().all(|s| s.is_finite()));
}
