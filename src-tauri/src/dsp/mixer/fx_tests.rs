use super::fx::{Delay, Reverb};
use super::{Mixer, MixerParams, BUS_MAIN, BUS_MONITOR2, BUS_STREAM};
use std::sync::{atomic::Ordering::Relaxed, Arc};
use std::time::{Duration, Instant};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;

fn normal(x: f32) -> bool {
    x == 0.0 || x.is_normal()
}

#[test]
fn reverb_has_a_stereo_tail_and_flushes_denormals() {
    let mut reverb = Reverb::new(SR);
    reverb.set_decay(1.8, true);
    let mut early: f32 = 0.0;
    let mut late: f32 = 0.0;
    let mut stereo = false;
    for i in 0..480_000 {
        let x = if i == 0 { 1.0 } else { f32::from_bits(1) };
        let [left, right] = reverb.tick([x, x]);
        assert!(normal(left) && normal(right));
        stereo |= left != right;
        if (24_000..48_000).contains(&i) {
            early = early.max(left.abs());
        }
        if (70_000..96_000).contains(&i) {
            late = late.max(left.abs());
        }
        if i > 450_000 {
            assert!(left.abs().max(right.abs()) < 1e-12);
        }
    }
    assert!(stereo && early > 1e-5 && late > 1e-8 && late < early * 0.1, "early={early}, late={late}");
}

#[test]
fn delay_repeats_on_the_beat_at_the_requested_feedback() {
    let mut delay = Delay::new(SR);
    delay.configure(120.0, 25.0, true);
    for i in 0..96_001 {
        let x = if i == 0 {
            [1.0, -0.5]
        } else {
            [f32::from_bits(1), f32::NAN]
        };
        let out = delay.tick(x);
        let expected = if i != 0 && i % 24_000 == 0 {
            0.25f32.powi(i / 24_000 - 1)
        } else {
            0.0
        };
        assert_eq!(out, [expected, -0.5 * expected], "sample {i}");
    }
}

#[test]
fn tempo_crossfade_and_reverb_decay_change_have_no_parameter_step() {
    let mut delay = Delay::new(SR);
    let mut reverb = Reverb::new(SR);
    delay.configure(120.0, 0.0, true);
    reverb.set_decay(1.8, true);
    let mut previous = [0.0; 2];
    for i in 0..52_000 {
        if i == 48_000 {
            delay.configure(240.0, 80.0, false);
            reverb.set_decay(5.0, false);
        }
        let x = (i as f32 * 0.002).sin() * 0.25;
        let sample = [delay.tick([x, x])[0], reverb.tick([x, x])[0]];
        if i >= 48_000 {
            for (a, b) in sample.into_iter().zip(previous) {
                assert!((a - b).abs() < 0.005, "sample {i}: {a} -> {b}");
            }
        }
        previous = sample;
    }
}

#[test]
fn sends_follow_fader_and_mute_and_returns_route_independently() {
    for (fader, mute, audible) in [
        (0.0, false, true),
        (-90.0, false, false),
        (0.0, true, false),
    ] {
        let params = Arc::new(MixerParams::new(1));
        let input = &params.strips[0];
        input.fader_db.store(fader, Relaxed);
        input.mute.store(mute, Relaxed);
        input.pan.store(-1.0, Relaxed);
        input.send_db[BUS_MAIN].store(-90.0, Relaxed);
        input.send_db[BUS_STREAM].store(-90.0, Relaxed);
        input.fx_send_db[1].store(0.0, Relaxed);
        let ret = &params.fx.returns[1];
        ret.sends[BUS_STREAM].store(-90.0, Relaxed);
        ret.sends[BUS_MONITOR2].store(0.0, Relaxed);
        let mut mixer = Mixer::new(SR as f64, BLOCK, 1, params);
        let mut impulse = [0.0; BLOCK];
        impulse[0] = 0.1;
        let silent = [0.0; BLOCK];
        let mut peak: f32 = 0.0;
        for i in 0..110 {
            mixer.process(&[if i == 0 { &impulse } else { &silent }], BLOCK);
            let (left, right) = mixer.output(BUS_MAIN);
            let (monitor, _) = mixer.output(BUS_MONITOR2);
            assert_eq!(left, monitor);
            assert!(right.iter().all(|v| v.abs() < 1e-8));
            assert!(mixer.output(BUS_STREAM).0.iter().all(|v| *v == 0.0));
            peak = peak.max(left.iter().fold(0.0f32, |m, x| m.max(x.abs())));
        }
        assert_eq!(peak > 0.1, audible);
    }
}

#[test]
fn both_effects_fit_the_block_budget() {
    use super::fx_rack::FxRack;
    use super::smooth::ramp_samples;
    use super::{StereoBuffer, NUM_BUSES};

    let params = MixerParams::new(1);
    let mut rack = FxRack::new(SR, BLOCK, ramp_samples(SR as f64));
    let mut buses: [StereoBuffer; NUM_BUSES] = std::array::from_fn(|_| StereoBuffer::new(BLOCK));
    for send in &mut rack.sends {
        for (i, (left, right)) in send.left.iter_mut().zip(&mut send.right).enumerate() {
            *left = (i as f32 * 0.5).sin() * 0.04;
            *right = *left;
        }
    }
    let mut elapsed = Duration::ZERO;
    let mut worst = Duration::ZERO;
    for block in 0..800 {
        for bus in &mut buses {
            bus.clear(BLOCK);
        }
        let start = Instant::now();
        rack.process(&mut buses, BLOCK, &params.fx, false, block == 0);
        let block = start.elapsed();
        elapsed += block;
        worst = worst.max(block);
        std::hint::black_box(&buses);
    }
    let average = elapsed / 800;
    eprintln!("256 frames at 48 kHz: average {average:?}, worst {worst:?}");
    let budget = BLOCK as f32 / SR * 0.25;
    assert!(average.as_secs_f32() < budget);
    if !cfg!(debug_assertions) {
        assert!(worst.as_secs_f32() < budget, "worst block {worst:?}");
    }
}

#[test]
fn sustained_sine_sweep_stays_below_three_db_at_every_decay_setting() {
    let mut worst = 0.0_f32;
    let mut worst_setting = 0.0_f32;
    for tenths in 3..=50 {
        let decay = tenths as f32 / 10.0;
        let mut reverb = Reverb::new(SR);
        reverb.set_decay(decay, true);
        let mut phase = 0.0_f64;
        // Sweep the vocal band, then hold the original regression tone long
        // enough to build up feedback at the longest decay.
        let frames = SR as usize * 3;
        for i in 0..frames {
            let hz = if i < frames / 2 {
                100.0 * 80.0_f64.powf(2.0 * i as f64 / frames as f64)
            } else {
                1_000.0
            };
            phase = (phase + std::f64::consts::TAU * hz / SR as f64) % std::f64::consts::TAU;
            let dry = 0.5 * phase.sin() as f32;
            for wet in reverb.tick([dry, dry]) {
                if wet.abs() * 2.0 > worst {
                    worst = wet.abs() * 2.0;
                    worst_setting = decay;
                }
            }
        }
    }
    eprintln!(
        "Worst wet peak {:+.2} dB at {worst_setting}s",
        20.0 * worst.log10()
    );
    assert!(worst <= 10f32.powf(3.0 / 20.0), "wet/input peak {worst}");
}

#[test]
fn low_frequency_rt60_tracks_decay_at_all_engine_rates() {
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        for decay in [0.3, 1.8, 5.0] {
            let mut reverb = Reverb::new(rate);
            reverb.set_decay(decay, true);
            // Measure low-band RT60; feedback damping intentionally shortens
            // the treble. Fit the Schroeder energy curve from -8 to -35 dB.
            let filter = 1.0 - (-std::f32::consts::TAU * 160.0 / rate).exp();
            let mut low = [0.0; 2];
            let mut energy = Vec::with_capacity((rate * (decay * 2.0 + 0.25)) as usize);
            for i in 0..energy.capacity() {
                let x = if i == 0 { 1.0 } else { 0.0 };
                let wet = reverb.tick([x, x]);
                for (out, input) in low.iter_mut().zip(wet) {
                    *out += filter * (input - *out);
                }
                energy.push(f64::from(low[0] * low[0] + low[1] * low[1]));
            }
            let mut remaining = 0.0;
            for value in energy.iter_mut().rev() {
                remaining += *value;
                *value = remaining;
            }
            let crossing = |db: f64| {
                let threshold = remaining * 10f64.powf(db / 10.0);
                energy.iter().position(|e| *e <= threshold).unwrap() as f32 / rate
            };
            let measured = (crossing(-35.0) - crossing(-8.0)) * (60.0 / 27.0);
            eprintln!("{rate} Hz {decay}s: measured {measured:.3}s");
            assert!((measured / decay - 1.0).abs() < 0.20);
        }
    }
}

#[test]
fn mixer_fx_gain_changes_and_retargeted_tempo_are_continuous() {
    let params = Arc::new(MixerParams::new(1));
    params.strips[0].send_db[BUS_MAIN].store(-90.0, Relaxed);
    params.strips[0].fx_send_db[1].store(0.0, Relaxed);
    params.fx.feedback.store(0.0, Relaxed);
    let mut mixer = Mixer::new(SR as f64, BLOCK, 1, params.clone());
    let mut last = 0.0;
    let mut highest = 0.0_f32;
    for block in 0..480 {
        match block {
            130 => params.strips[0].fx_send_db[1].store(-90.0, Relaxed),
            145 => params.strips[0].fx_send_db[1].store(0.0, Relaxed),
            260 => params.fx.returns[1].fader.store(-30.0, Relaxed),
            270 => params.fx.returns[1].fader.store(0.0, Relaxed),
            280 => params.fx.returns[1].mute.store(true, Relaxed),
            290 => params.fx.returns[1].mute.store(false, Relaxed),
            310 => params.fx.tempo.store(170.0, Relaxed),
            312 => params.fx.tempo.store(230.0, Relaxed),
            _ => {}
        }
        let input: [f32; BLOCK] =
            std::array::from_fn(|i| 0.18 * (1.0 + ((block * BLOCK + i) as f32 * 0.0021).sin()));
        mixer.process(&[&input], BLOCK);
        for &out in mixer.output(BUS_MAIN).0 {
            if block >= 129 {
                assert!((out - last).abs() < 0.003, "block {block}: {last} -> {out}");
            }
            last = out;
            highest = highest.max(out);
        }
    }
    assert!(highest > 0.3);
}
