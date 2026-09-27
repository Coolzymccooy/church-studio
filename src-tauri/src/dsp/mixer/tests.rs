use super::params::MIN_DB;
use super::*;
use crate::dsp::test_util::{max_abs_diff, peak, rms, sine};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SR: f64 = 48_000.0;
const SRU: usize = 48_000;
const BLOCK: usize = 512;
const CENTRE: f32 = std::f32::consts::FRAC_1_SQRT_2;

struct BusOut {
    left: Vec<f32>,
    right: Vec<f32>,
}

fn new_mixer(num_strips: usize) -> (Arc<MixerParams>, Mixer) {
    let params = Arc::new(MixerParams::new(num_strips));
    let mixer = Mixer::new(SR, BLOCK, num_strips, params.clone());
    (params, mixer)
}

/// Run `inputs` through the mixer in blocks; returns [Main, Stream, Monitor].
fn run(mixer: &mut Mixer, inputs: &[Vec<f32>], block: usize) -> Vec<BusOut> {
    let len = inputs.iter().map(|v| v.len()).max().unwrap_or(0);
    let mut outs: Vec<BusOut> = (0..NUM_BUSES)
        .map(|_| BusOut { left: Vec::with_capacity(len), right: Vec::with_capacity(len) })
        .collect();
    let mut pos = 0;
    while pos < len {
        let end = (pos + block).min(len);
        let slices: Vec<&[f32]> = inputs.iter().map(|v| &v[pos.min(v.len())..end.min(v.len())]).collect();
        let n = mixer.process(&slices, end - pos);
        assert_eq!(n, end - pos);
        for (b, out) in outs.iter_mut().enumerate() {
            let (l, r) = mixer.output(b);
            out.left.extend_from_slice(l);
            out.right.extend_from_slice(r);
        }
        pos = end;
    }
    outs
}

fn dc(len: usize, v: f32) -> Vec<f32> {
    vec![v; len]
}

fn tail(v: &[f32], n: usize) -> &[f32] {
    &v[v.len() - n..]
}

#[test]
fn unity_passthrough_is_centre_panned() {
    let (_p, mut mixer) = new_mixer(1);
    let out = run(&mut mixer, &[dc(SRU, 0.5)], BLOCK);
    for bus in [BUS_MAIN, BUS_STREAM] {
        for side in [&out[bus].left, &out[bus].right] {
            for &s in tail(side, 1_000) {
                assert!((s - 0.5 * CENTRE).abs() < 1e-3, "bus {bus}: {s}");
            }
        }
    }
    // Monitor send defaults to off.
    assert!(peak(&out[BUS_MONITOR].left) < 1e-9);
}

#[test]
fn hard_left_pan_silences_right() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].pan.store(-1.0, Relaxed);
    let out = run(&mut mixer, &[sine(SRU / 2, 440.0, 48_000.0, 0.5)], BLOCK);
    assert!(peak(&out[BUS_MAIN].right) < 1e-9);
    assert!(peak(tail(&out[BUS_MAIN].left, 4_800)) > 0.4);
}

#[test]
fn mute_silences_every_bus_after_ramp() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].send_db[BUS_MONITOR].store(0.0, Relaxed);
    let input = dc(SRU, 0.5);
    let _ = run(&mut mixer, &[input[..SRU / 2].to_vec()], BLOCK);
    p.strips[0].mute.store(true, Relaxed);
    let out = run(&mut mixer, &[input[SRU / 2..].to_vec()], BLOCK);
    for bus in &out {
        assert!(peak(tail(&bus.left, 4_800)) < 1e-7);
        assert!(peak(tail(&bus.right, 4_800)) < 1e-7);
    }
}

#[test]
fn solo_is_pfl_on_monitor_only() {
    let a = sine(SRU, 440.0, 48_000.0, 0.3);
    let b = sine(SRU, 1_000.0, 48_000.0, 0.3);
    let silent = vec![0.0f32; SRU];

    let setup = |solo_a: bool| {
        let (p, mixer) = new_mixer(2);
        for s in &p.strips {
            s.send_db[BUS_MONITOR].store(0.0, Relaxed);
            s.fader_db.store(-10.0, Relaxed);
        }
        p.strips[0].solo.store(solo_a, Relaxed);
        mixer
    };

    let no_solo = run(&mut setup(false), &[a.clone(), b.clone()], BLOCK);
    let solo = run(&mut setup(true), &[a.clone(), b.clone()], BLOCK);
    let solo_a_only = run(&mut setup(true), &[a.clone(), silent], BLOCK);

    // Main and Stream are bit-identical with and without solo.
    for bus in [BUS_MAIN, BUS_STREAM] {
        assert_eq!(no_solo[bus].left, solo[bus].left);
        assert_eq!(no_solo[bus].right, solo[bus].right);
    }
    // Monitor carries only the soloed strip...
    assert!(max_abs_diff(&solo[BUS_MONITOR].left, &solo_a_only[BUS_MONITOR].left) < 1e-9);
    assert!(max_abs_diff(&solo[BUS_MONITOR].right, &solo_a_only[BUS_MONITOR].right) < 1e-9);
    // ...pre-fader at unity (0 dB) on BOTH sides: the fader is −10 dB and the
    // pan law is not applied, so the level equals the input (0.3).
    for side in [&solo[BUS_MONITOR].left, &solo[BUS_MONITOR].right] {
        let gain = peak(tail(side, 4_800)) / 0.3;
        assert!((gain - 1.0).abs() < 0.01, "PFL gain {gain}");
    }
    // Without solo the monitor has both strips.
    assert!(max_abs_diff(&no_solo[BUS_MONITOR].left, &solo[BUS_MONITOR].left) > 0.01);
}

#[test]
fn send_level_scales_bus() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].send_db[BUS_MAIN].store(-6.0, Relaxed);
    let out = run(&mut mixer, &[dc(SRU, 0.5)], BLOCK);
    let s = *tail(&out[BUS_MAIN].left, 1).first().unwrap();
    assert!((s - 0.5 * CENTRE * 0.501).abs() < 1e-3, "{s}");
    let st = *tail(&out[BUS_STREAM].left, 1).first().unwrap();
    assert!((st - 0.5 * CENTRE).abs() < 1e-3, "{st}");
}

#[test]
fn bus_limiter_holds_ceiling() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].trim_db.store(20.0, Relaxed);
    p.buses[BUS_STREAM].limiter_ceiling_db.store(-6.0, Relaxed);
    let out = run(&mut mixer, &[sine(SRU, 220.0, 48_000.0, 0.5)], BLOCK);
    let main_ceiling = 10f32.powf(-1.0 / 20.0);
    let stream_ceiling = 10f32.powf(-6.0 / 20.0);
    assert!(peak(&out[BUS_MAIN].left) <= main_ceiling + 1e-5);
    assert!(peak(&out[BUS_MAIN].right) <= main_ceiling + 1e-5);
    assert!(peak(&out[BUS_STREAM].left) <= stream_ceiling + 1e-5);
    assert!(peak(tail(&out[BUS_MAIN].left, 4_800)) > 0.8 * main_ceiling);
}

#[test]
fn eq_mid_boost_is_12_db_at_1k() {
    let input = sine(SRU, 1_000.0, 48_000.0, 0.05);
    let (_p, mut flat) = new_mixer(1);
    let flat_out = run(&mut flat, &[input.clone()], BLOCK);
    let (p, mut boosted) = new_mixer(1);
    p.strips[0].eq_mid_db.store(12.0, Relaxed);
    let boost_out = run(&mut boosted, &[input], BLOCK);

    let n = SRU / 2;
    let gain_db =
        20.0 * (rms(tail(&boost_out[BUS_MAIN].left, n)) / rms(tail(&flat_out[BUS_MAIN].left, n))).log10();
    assert!((gain_db - 12.0).abs() < 1.0, "gain {gain_db} dB");
}

#[test]
fn short_inputs_and_oversized_frames_are_safe() {
    let (_p, mut mixer) = new_mixer(3);
    let short = vec![0.25f32; 50];
    let inputs: [&[f32]; 1] = [&short];
    assert_eq!(mixer.process(&inputs, 100), 100);
    assert_eq!(mixer.output(BUS_MAIN).0.len(), 100);
    assert_eq!(mixer.process(&inputs, BLOCK * 3), BLOCK);
    assert_eq!(mixer.process(&[], 0), 0);
    assert!(mixer.output(99).0.is_empty());
    assert_eq!(mixer.meters().strips.len(), 3);
}

#[test]
fn no_nan_on_silence_with_all_processing_on() {
    let (p, mut mixer) = new_mixer(2);
    for s in &p.strips {
        s.hpf_enabled.store(true, Relaxed);
        s.gate_enabled.store(true, Relaxed);
        s.comp_enabled.store(true, Relaxed);
        s.eq_low_db.store(6.0, Relaxed);
        s.send_db[BUS_MONITOR].store(0.0, Relaxed);
    }
    let out = run(&mut mixer, &[dc(SRU / 2, 0.0), dc(SRU / 2, 0.0)], 480);
    for bus in &out {
        assert!(bus.left.iter().chain(bus.right.iter()).all(|s| s.is_finite()));
    }
    for m in mixer.strip_meters() {
        assert!(m.pre_fader_peak_db.is_finite() && m.gain_reduction_db.is_finite());
    }
}

#[test]
fn fader_jump_is_ramped_without_clicks() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].fader_db.store(MIN_DB, Relaxed);
    let input = dc(SRU, 1.0);
    let _ = run(&mut mixer, &[input[..SRU / 2].to_vec()], BLOCK);
    p.strips[0].fader_db.store(0.0, Relaxed);
    let out = run(&mut mixer, &[input[SRU / 2..].to_vec()], BLOCK);

    let ramp = smooth::ramp_samples(SR) as f32;
    assert!(ramp >= 0.005 * SR as f32, "ramp must be at least 5 ms");
    let max_step = CENTRE / ramp * 1.01 + 1e-6;
    let main = &out[BUS_MAIN].left;
    for w in main.windows(2) {
        assert!((w[1] - w[0]).abs() <= max_step, "step {}", (w[1] - w[0]).abs());
    }
    assert!((main[main.len() - 1] - CENTRE).abs() < 1e-3);
}

#[test]
fn muted_strip_is_still_audible_on_pfl() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].mute.store(true, Relaxed);
    p.strips[0].solo.store(true, Relaxed);
    let out = run(&mut mixer, &[sine(SRU, 440.0, 48_000.0, 0.3)], BLOCK);
    // Main and Stream honour the mute...
    for bus in [BUS_MAIN, BUS_STREAM] {
        assert!(peak(&out[bus].left) < 1e-9);
        assert!(peak(&out[bus].right) < 1e-9);
    }
    // ...but PFL ignores it: unity on both monitor sides.
    for side in [&out[BUS_MONITOR].left, &out[BUS_MONITOR].right] {
        let gain = peak(tail(side, 4_800)) / 0.3;
        assert!((gain - 1.0).abs() < 0.01, "PFL gain {gain}");
    }
}

#[test]
fn compressor_toggle_is_crossfaded() {
    let (p, mut mixer) = new_mixer(1);
    p.strips[0].comp_threshold_db.store(-40.0, Relaxed);
    p.strips[0].comp_ratio.store(10.0, Relaxed);
    let input = sine(SRU * 2, 440.0, 48_000.0, 0.5);
    let mut main = Vec::with_capacity(input.len());
    for (i, chunk) in input.chunks(480).enumerate() {
        p.strips[0].comp_enabled.store((i / 20) % 2 == 1, Relaxed);
        let n = mixer.process(&[chunk], chunk.len());
        main.extend_from_slice(&mixer.output(BUS_MAIN).0[..n]);
    }
    // Heavy compression (~20 dB of gain reduction) switched every 200 ms:
    // without the crossfade the gain would jump; with it the step stays at
    // the sine's own slope (< 0.03) plus a tiny ramp increment.
    let max_step = main.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    assert!(max_step < 0.04, "step {max_step}");
    // And the compressor really acts when on.
    assert!(mixer.strip_meters()[0].gain_reduction_db < -6.0);
}
