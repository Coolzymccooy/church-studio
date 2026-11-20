//! Behaviour tests for the Hyper gate on synthetic speech, music and noise.
use super::signals::{mix, music, speech, steady_noise, SR};
use super::{Fingerprint, LearnOutcome};
use crate::dsp::biquad::Biquad;
use crate::dsp::gate::{GateMode, SwitchedGate};
use crate::dsp::test_util::{max_abs_diff, rms, run_blocks};

const SECONDS: usize = 8;
const LEN: usize = SR as usize * SECONDS;
/// Skip the switch's dry fill, crossfade and the first analysis windows.
const WARMUP: usize = SR as usize;

fn gate(mode: GateMode, fp: Option<Fingerprint>) -> SwitchedGate {
    let mut g = SwitchedGate::new_hyper(SR as f64);
    g.set_threshold_db(-45.0);
    g.set_mode(mode);
    g.set_fingerprint(fp);
    g
}

/// Gain (dB) the gate applied to `input` after warm-up, lookahead-aligned.
fn gain_db(g: &mut SwitchedGate, input: &[f32], vad: Option<f32>) -> f32 {
    let lat = g.latency_samples();
    let out = run_blocks(input, 480, |b| {
        g.process_block_vad(b, vad);
    });
    let out_rms = rms(&out[WARMUP..]);
    let in_rms = rms(&input[WARMUP - lat..input.len() - lat]);
    20.0 * (out_rms.max(1e-9) / in_rms.max(1e-9)).log10()
}

fn run(mode: GateMode, input: &[f32], vad: Option<f32>) -> f32 {
    gain_db(&mut gate(mode, None), input, vad)
}

/// Music played staccato (4 Hz on/off): dips like syllables, so only the
/// VAD can tell it from speech.
fn staccato_music() -> Vec<f32> {
    music(LEN, -20.0)
        .iter()
        .enumerate()
        .map(|(i, s)| if (i * 8 / SR as usize) % 2 == 0 { *s } else { 0.0 })
        .collect()
}

/// The same talker heard from a TV across the room: thin and bright.
fn tv_speech() -> Vec<f32> {
    let mut hp = Biquad::hpf(900.0, SR as f64);
    let mut hp2 = Biquad::hpf(900.0, SR as f64);
    let raw: Vec<f32> = speech(LEN, 21, -20.0).iter().map(|&s| hp2.tick(hp.tick(s))).collect();
    let r = rms(&raw.iter().copied().filter(|s| s.abs() > 1e-4).collect::<Vec<_>>());
    raw.iter().map(|s| s * 0.1 / r).collect()
}

#[test]
fn speech_passes_in_both_modes() {
    let voice = speech(LEN, 1, -20.0);
    for mode in [GateMode::Balanced, GateMode::Speech] {
        let g = run(mode, &voice, None);
        assert!(g > -3.0, "{mode:?}: speech lost {g:.1} dB");
    }
}

#[test]
fn speech_over_room_noise_passes_speech_only() {
    let voice = mix(&speech(LEN, 2, -20.0), &steady_noise(LEN, 9, -50.0));
    let g = run(GateMode::Speech, &voice, None);
    assert!(g > -3.0, "speech lost {g:.1} dB");
}

#[test]
fn steady_noise_is_held_back_in_speech_only() {
    // -30 dBFS: 15 dB above the threshold, so a level gate would open.
    let noise = steady_noise(LEN, 3, -30.0);
    let g = run(GateMode::Speech, &noise, None);
    assert!(g < -20.0, "noise only {g:.1} dB down");
}

#[test]
fn music_is_held_back_in_speech_only_but_passes_balanced() {
    let keys = music(LEN, -20.0);
    let speech_only = run(GateMode::Speech, &keys, None);
    let balanced = run(GateMode::Balanced, &keys, None);
    assert!(speech_only < -15.0, "music only {speech_only:.1} dB down");
    assert!(balanced > -3.0, "balanced cut music by {balanced:.1} dB");
}

#[test]
fn quiet_noise_below_threshold_is_closed_in_both_modes() {
    let hum = steady_noise(LEN, 4, -60.0);
    for mode in [GateMode::Balanced, GateMode::Speech] {
        let g = run(mode, &hum, None);
        assert!(g < -30.0, "{mode:?}: only {g:.1} dB down");
    }
}

#[test]
fn rnnoise_vad_separates_rhythmic_music_from_speech() {
    let staccato = staccato_music();
    let voice = speech(LEN, 5, -20.0);
    assert!(run(GateMode::Speech, &voice, Some(0.95)) > -3.0);
    let g = run(GateMode::Speech, &staccato, Some(0.05));
    assert!(g < -15.0, "staccato music only {g:.1} dB down with VAD 0.05");
}

/// Learn the fingerprint from `input` on a running speech-only gate.
fn learn(input: &[f32]) -> Option<LearnOutcome> {
    let mut g = gate(GateMode::Speech, None);
    g.start_learning();
    let mut outcome = None;
    run_blocks(input, 256, |b| {
        g.process_block(b);
        if let Some(o) = g.take_learn_outcome() {
            outcome = Some(o);
        }
    });
    outcome
}

#[test]
fn fingerprint_is_learned_from_speech_and_rejects_a_tv() {
    let Some(LearnOutcome::Learned(fp)) = learn(&speech(SR as usize * 6, 11, -20.0)) else {
        panic!("no fingerprint learned from speech");
    };

    // Applied: the learned talker still passes, louder or not.
    let same_talker = speech(LEN, 12, -14.0);
    let own = gain_db(&mut gate(GateMode::Speech, Some(fp)), &same_talker, None);
    assert!(own > -3.0, "learned talker lost {own:.1} dB");

    // A TV with speech on it opens a speech-only gate without a fingerprint,
    // and is held back once the mic has been learned.
    let tv = tv_speech();
    let without = gain_db(&mut gate(GateMode::Speech, None), &tv, None);
    let with = gain_db(&mut gate(GateMode::Speech, Some(fp)), &tv, None);
    assert!(without > -6.0, "TV already held back without fingerprint ({without:.1} dB)");
    // Held back: the speech-only cap is −26 dB; a few stray openings remain.
    assert!(with < -10.0, "TV only {with:.1} dB down with fingerprint");
}

#[test]
fn learning_needs_voice() {
    let outcome = learn(&steady_noise(SR as usize * 6, 13, -30.0));
    assert_eq!(outcome, Some(LearnOutcome::NotEnoughVoice));
}

#[test]
fn hyper_gate_is_block_size_invariant() {
    let input = mix(&speech(SR as usize * 4, 14, -20.0), &music(SR as usize * 4, -32.0));
    for mode in [GateMode::Balanced, GateMode::Speech] {
        let outputs: Vec<Vec<f32>> = [64usize, 256, 480, 1000, 4096]
            .iter()
            .map(|&block| {
                let mut g = gate(mode, None);
                run_blocks(&input, block, |b| {
                    g.process_block_vad(b, Some(0.7));
                })
            })
            .collect();
        for out in outputs.iter().skip(1) {
            assert_eq!(max_abs_diff(&outputs[0], out), 0.0, "{mode:?}");
        }
    }
}

