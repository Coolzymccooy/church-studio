//! Mon 2 / mon 3 (`docs/specs/2026-11-20-monitor-mixes.md`): pre-fader
//! sends, off at −90, silenced by mute, and untouched by PFL, the monitor
//! post-fader switch and the front-of-house fader.
use super::params::MIN_DB;
use super::*;
use crate::dsp::test_util::{max_abs_diff, peak};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SR: f64 = 48_000.0;
const SRU: usize = 48_000;
const BLOCK: usize = 512;

/// Left side of every bus, in `BUS_*` order.
fn run(mixer: &mut Mixer, input: &[f32]) -> Vec<Vec<f32>> {
    let mut outs = vec![Vec::with_capacity(input.len()); NUM_BUSES];
    for chunk in input.chunks(BLOCK) {
        let inputs: [&[f32]; 1] = [chunk];
        mixer.process(&inputs, chunk.len());
        for (bus, out) in outs.iter_mut().enumerate() {
            out.extend_from_slice(mixer.output(bus).0);
        }
    }
    outs
}

fn mixer_with(setup: impl Fn(&StripParams)) -> Mixer {
    let params = Arc::new(MixerParams::new(1));
    setup(&params.strips[0]);
    Mixer::new(SR, BLOCK, 1, params)
}

fn tail(v: &[f32]) -> &[f32] {
    &v[v.len() - 4_800..]
}

/// Steady level of the last 100 ms of `bus`.
fn level(out: &[Vec<f32>], bus: usize) -> f32 {
    peak(tail(&out[bus]))
}

#[test]
fn there_are_five_buses_in_contract_order() {
    assert_eq!(BUS_COUNT, 5);
    assert_eq!(
        [BUS_MAIN, BUS_STREAM, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3],
        [0, 1, 2, 3, 4]
    );
    let params = MixerParams::new(1);
    assert_eq!(params.buses.len(), BUS_COUNT);
    assert_eq!(params.strips[0].send_db[BUS_MONITOR2].load(Relaxed), MIN_DB);
    assert_eq!(params.strips[0].send_db[BUS_MONITOR3].load(Relaxed), MIN_DB);
}

#[test]
fn mon2_and_mon3_are_silent_at_minus_90() {
    let mut mixer = mixer_with(|_| {});
    let out = run(&mut mixer, &vec![0.5; SRU]);
    assert!(peak(&out[BUS_MONITOR2]) < 1e-9);
    assert!(peak(&out[BUS_MONITOR3]) < 1e-9);
    assert!(level(&out, BUS_MAIN) > 0.4, "main still carries the strip");
}

#[test]
fn mon2_and_mon3_sends_are_pre_fader() {
    let mut mixer = mixer_with(|s| {
        s.fader_db.store(MIN_DB, Relaxed); // front-of-house fader closed
        s.send_db[BUS_MONITOR2].store(0.0, Relaxed);
        s.send_db[BUS_MONITOR3].store(-6.0, Relaxed);
    });
    let out = run(&mut mixer, &vec![0.5; SRU]);
    assert!(level(&out, BUS_MAIN) < 1e-7, "main follows the fader");
    assert!((level(&out, BUS_MONITOR2) - 0.5).abs() < 1e-3);
    let mon3 = level(&out, BUS_MONITOR3);
    assert!((mon3 - 0.5 * 10f32.powf(-6.0 / 20.0)).abs() < 1e-3, "mon 3 at -6 dB: {mon3}");
}

#[test]
fn mute_silences_mon2_and_mon3() {
    let mut mixer = mixer_with(|s| {
        s.send_db[BUS_MONITOR2].store(0.0, Relaxed);
        s.send_db[BUS_MONITOR3].store(0.0, Relaxed);
        s.mute.store(true, Relaxed);
    });
    let out = run(&mut mixer, &vec![0.5; SRU]);
    assert!(peak(&out[BUS_MONITOR2]) < 1e-7);
    assert!(peak(&out[BUS_MONITOR3]) < 1e-7);
}

#[test]
fn pfl_reaches_mon1_only() {
    let setup = |solo: bool| {
        let params = Arc::new(MixerParams::new(2));
        for s in &params.strips {
            s.send_db[BUS_MONITOR].store(0.0, Relaxed);
            s.send_db[BUS_MONITOR2].store(-3.0, Relaxed);
            s.send_db[BUS_MONITOR3].store(-9.0, Relaxed);
        }
        params.strips[0].solo.store(solo, Relaxed);
        Mixer::new(SR, BLOCK, 2, params)
    };
    let a = vec![0.3f32; SRU];
    let b = vec![0.2f32; SRU];
    let render = |mut mixer: Mixer| {
        let mut outs = vec![Vec::new(); NUM_BUSES];
        for pos in (0..SRU).step_by(BLOCK) {
            let end = (pos + BLOCK).min(SRU);
            mixer.process(&[&a[pos..end], &b[pos..end]], end - pos);
            for (bus, out) in outs.iter_mut().enumerate() {
                out.extend_from_slice(mixer.output(bus).0);
            }
        }
        outs
    };
    let plain = render(setup(false));
    let soloed = render(setup(true));
    for bus in [BUS_MONITOR2, BUS_MONITOR3] {
        assert_eq!(plain[bus], soloed[bus], "bus {bus} must ignore solo");
    }
    // Mon 1 becomes PFL of strip 0 alone (0.3), losing strip 1.
    assert!((level(&soloed, BUS_MONITOR) - 0.3).abs() < 1e-3);
    assert!(max_abs_diff(&plain[BUS_MONITOR], &soloed[BUS_MONITOR]) > 0.1);
}

#[test]
fn monitor_post_fader_moves_mon1_only() {
    let mut mixer = mixer_with(|s| {
        s.fader_db.store(-20.0, Relaxed);
        s.monitor_post_fader.store(true, Relaxed);
        s.send_db[BUS_MONITOR].store(0.0, Relaxed);
        s.send_db[BUS_MONITOR2].store(0.0, Relaxed);
        s.send_db[BUS_MONITOR3].store(0.0, Relaxed);
    });
    let out = run(&mut mixer, &vec![0.5; SRU]);
    let post = 0.5 * 10f32.powf(-20.0 / 20.0);
    assert!((level(&out, BUS_MONITOR) - post).abs() < 1e-3, "mon 1 follows the fader");
    assert!((level(&out, BUS_MONITOR2) - 0.5).abs() < 1e-3, "mon 2 stays pre-fader");
    assert!((level(&out, BUS_MONITOR3) - 0.5).abs() < 1e-3, "mon 3 stays pre-fader");
}

#[test]
fn mon2_and_mon3_have_their_own_bus_fader_mute_and_limiter() {
    let params = Arc::new(MixerParams::new(1));
    params.strips[0].trim_db.store(20.0, Relaxed);
    params.strips[0].send_db[BUS_MONITOR2].store(0.0, Relaxed);
    params.strips[0].send_db[BUS_MONITOR3].store(0.0, Relaxed);
    params.buses[BUS_MONITOR2].limiter_ceiling_db.store(-6.0, Relaxed);
    params.buses[BUS_MONITOR3].mute.store(true, Relaxed);
    let mut mixer = Mixer::new(SR, BLOCK, 1, params.clone());
    let out = run(&mut mixer, &vec![0.5; SRU]);
    assert!(peak(&out[BUS_MONITOR2]) <= 10f32.powf(-6.0 / 20.0) + 1e-5);
    assert!(level(&out, BUS_MONITOR2) > 0.4);
    assert!(level(&out, BUS_MONITOR3) < 1e-7);
    assert_eq!(mixer.bus_meters().len(), BUS_COUNT);
    assert!(mixer.bus_meters()[BUS_MONITOR2].peak_l_db > -7.0);
}
