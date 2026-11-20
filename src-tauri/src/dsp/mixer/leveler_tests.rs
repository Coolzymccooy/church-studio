//! Loudness leveler tests: convergence, the gain bound and slew rate,
//! silence freeze, block-size invariance and the bus limiter behind it.
use super::leveler::{Leveler, LEVELER_RANGE_DB, LEVELER_RATE_DB_PER_S};
use super::params::{MixerParams, BUS_STREAM};
use super::Mixer;
use crate::dsp::lufs::LufsMeter;
use crate::dsp::test_util::{peak, test_signal, Lcg};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SR: f64 = 48_000.0;
const SRU: usize = 48_000;
const TARGET: f32 = -14.0;

/// Integrated (ungated) stereo BS.1770 loudness of `left`/`right`, from the
/// reference meter's last short-term reading on each channel.
fn stereo_short_term(left: &[f32], right: &[f32]) -> f32 {
    let channel_power = |x: &[f32]| {
        let mut meter = LufsMeter::new(SR);
        let mut st = -70.0;
        for &s in x {
            if let Some(r) = meter.tick(s) {
                st = r.short_term;
            }
        }
        10f64.powf((st as f64 + 0.691) / 10.0)
    };
    let sum = channel_power(left) + channel_power(right);
    (10.0 * sum.log10() - 0.691) as f32
}

/// Speech-like programme: a steady signal switched on for 300 ms and off for
/// 200 ms (pauses between phrases), scaled so its stereo short-term
/// loudness (measured over the last 3 s) is `lufs`.
fn programme(secs: usize, lufs: f32, seed: u32) -> Vec<f32> {
    let raw: Vec<f32> = test_signal(secs * SRU, seed)
        .iter()
        .enumerate()
        .map(|(i, &s)| if (i % 24_000) < 14_400 { s } else { 0.0 })
        .collect();
    let tail = &raw[raw.len() - 3 * SRU..];
    let measured = stereo_short_term(tail, tail);
    let gain = 10f32.powf((lufs - measured) / 20.0);
    raw.iter().map(|&s| s * gain).collect()
}

fn concat(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

/// Run the leveler over a mono programme fed to both sides; returns the
/// output and the gain (dB) after every `block`.
fn run(lev: &mut Leveler, input: &[f32], block: usize, on: bool) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut left = input.to_vec();
    let mut right = input.to_vec();
    let mut gains = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let end = (pos + block).min(input.len());
        lev.process(&mut left[pos..end], &mut right[pos..end], on, TARGET);
        gains.push(lev.gain_db());
        pos = end;
    }
    (left, right, gains)
}

fn last_secs(v: &[f32], secs: usize) -> &[f32] {
    &v[v.len() - secs * SRU..]
}

#[test]
fn programme_under_target_converges_to_it() {
    let input = programme(12, TARGET - 4.0, 1);
    let mut lev = Leveler::new(SR);
    let (l, r, _) = run(&mut lev, &input, 512, true);
    assert!((lev.gain_db() - 4.0).abs() < 0.5, "gain {}", lev.gain_db());
    let out = stereo_short_term(last_secs(&l, 3), last_secs(&r, 3));
    assert!((out - TARGET).abs() < 0.5, "output {out} LUFS, target {TARGET}");
}

#[test]
fn programme_over_target_is_brought_down() {
    let input = programme(12, TARGET + 4.0, 2);
    let mut lev = Leveler::new(SR);
    let (l, r, _) = run(&mut lev, &input, 512, true);
    let out = stereo_short_term(last_secs(&l, 3), last_secs(&r, 3));
    assert!((out - TARGET).abs() < 0.5, "output {out} LUFS, target {TARGET}");
}

#[test]
fn ten_db_under_target_stops_at_the_bound() {
    let input = programme(15, TARGET - 10.0, 3);
    let mut lev = Leveler::new(SR);
    let (l, r, gains) = run(&mut lev, &input, 512, true);
    for g in &gains {
        assert!(*g <= LEVELER_RANGE_DB + 1e-4, "gain {g} beyond the bound");
    }
    assert!((lev.gain_db() - LEVELER_RANGE_DB).abs() < 1e-3, "gain {}", lev.gain_db());
    let out = stereo_short_term(last_secs(&l, 3), last_secs(&r, 3));
    let expected = TARGET - 10.0 + LEVELER_RANGE_DB;
    assert!((out - expected).abs() < 0.5, "output {out} LUFS, expected {expected}");
}

#[test]
fn gain_moves_no_faster_than_the_rate() {
    let input = programme(10, TARGET - 10.0, 4);
    let mut lev = Leveler::new(SR);
    let block = 4_800; // 100 ms
    let (_, _, gains) = run(&mut lev, &input, block, true);
    let max_step = LEVELER_RATE_DB_PER_S * 0.1 + 1e-4;
    let mut prev = 0.0;
    for g in gains {
        assert!((g - prev).abs() <= max_step, "step {} dB in 100 ms", g - prev);
        prev = g;
    }
}

#[test]
fn silence_does_not_pump_the_gain_up() {
    let speech = programme(8, TARGET - 3.0, 5);
    let mut lev = Leveler::new(SR);
    let (_, _, _) = run(&mut lev, &speech, 512, true);
    let held = lev.gain_db();
    assert!((held - 3.0).abs() < 0.5, "settled gain {held}");

    // 30 s of silence, then 30 s of a quiet room (40 dB under target).
    let mut rng = Lcg::new(9);
    let room_amp = 10f32.powf((TARGET - 40.0) / 20.0);
    let room: Vec<f32> = (0..30 * SRU).map(|_| rng.next_f32() * room_amp).collect();
    let quiet = concat(&[vec![0.0; 30 * SRU], room]);
    let (_, _, gains) = run(&mut lev, &quiet, 512, true);
    // It may finish the last 0.1 dB it was already ramping toward, no more.
    for g in gains {
        assert!((g - held).abs() < 0.1, "gain moved to {g} in silence (held {held})");
    }
}

#[test]
fn silence_from_the_start_leaves_unity_gain() {
    let mut lev = Leveler::new(SR);
    let input = vec![0.0; 10 * SRU];
    let (l, _, _) = run(&mut lev, &input, 512, true);
    assert_eq!(lev.gain_db(), 0.0);
    assert!(l.iter().all(|&s| s == 0.0));
}

#[test]
fn off_is_bit_exact_passthrough() {
    let input = programme(4, TARGET - 8.0, 6);
    let mut lev = Leveler::new(SR);
    let (l, r, _) = run(&mut lev, &input, 512, false);
    assert_eq!(l, input);
    assert_eq!(r, input);
    assert_eq!(lev.gain_db(), 0.0);
}

#[test]
fn turning_off_ramps_back_to_unity() {
    let input = programme(10, TARGET - 5.0, 7);
    let mut lev = Leveler::new(SR);
    run(&mut lev, &input, 512, true);
    assert!(lev.gain_db() > 4.0);
    let (_, _, gains) = run(&mut lev, &input[..SRU], 480, false);
    assert!(gains[0] > 4.0, "gain must not jump to 0 dB");
    let (_, _, _) = run(&mut lev, &input[..2 * SRU], 480, false);
    assert_eq!(lev.gain_db(), 0.0);
}

#[test]
fn output_does_not_depend_on_block_size() {
    let input = concat(&[
        programme(6, TARGET - 7.0, 8),
        vec![0.0; 2 * SRU],
        programme(6, TARGET + 3.0, 9),
    ]);
    let mut reference = Leveler::new(SR);
    let (ref_l, ref_r, _) = run(&mut reference, &input, 4_096, true);
    for block in [1usize, 37, 480, 512, 10_000] {
        let mut lev = Leveler::new(SR);
        let (l, r, _) = run(&mut lev, &input, block, true);
        assert_eq!(lev.gain_db(), reference.gain_db(), "block {block}");
        assert!(l == ref_l && r == ref_r, "block {block} output differs");
    }
}

#[test]
fn recovers_from_a_nan_sample() {
    let mut input = programme(10, TARGET - 4.0, 10);
    input[SRU] = f32::NAN;
    let mut lev = Leveler::new(SR);
    let (l, _, _) = run(&mut lev, &input, 512, true);
    assert!(lev.gain_db().is_finite());
    assert!(last_secs(&l, 2).iter().all(|s| s.is_finite()));
    assert!(lev.gain_db() > 2.0, "leveler stopped working after a NaN: {}", lev.gain_db());
}

#[test]
fn stream_bus_limiter_still_holds_the_ceiling() {
    // Quiet programme with hot transients: the leveler boosts it by the
    // full +6 dB and the bus limiter must still hold −1 dBFS.
    let mut input = programme(12, TARGET - 12.0, 11);
    for i in (0..input.len()).step_by(SRU / 4) {
        input[i] = 0.95;
    }
    let params = Arc::new(MixerParams::new(1));
    params.buses[BUS_STREAM].leveler_enabled.store(true, Relaxed);
    params.buses[BUS_STREAM].leveler_target_lufs.store(TARGET, Relaxed);
    let mut mixer = Mixer::new(SR, 512, 1, params.clone());
    let ceiling = 10f32.powf(-1.0 / 20.0);
    let mut last_l = Vec::new();
    for chunk in input.chunks(512) {
        mixer.process(&[chunk], chunk.len());
        let (l, r) = mixer.output(BUS_STREAM);
        assert!(peak(l) <= ceiling + 1e-5 && peak(r) <= ceiling + 1e-5);
        last_l.extend_from_slice(l);
    }
    assert!((mixer.bus_meters()[BUS_STREAM].leveler_db - LEVELER_RANGE_DB).abs() < 1e-3);
    // Main has no leveler by default.
    assert_eq!(mixer.bus_meters()[0].leveler_db, 0.0);
    assert!(peak(last_secs(&last_l, 3)) > 0.8 * ceiling);
}
