//! Automixer and auto-ducking: simulated talkers, bleed and music.
use super::params::MIN_DB;
use super::*;
use crate::dsp::test_util::{max_abs_diff, sine, Lcg};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SR: f64 = 48_000.0;
const SRU: usize = 48_000;
const BLOCK: usize = 480;
/// Room noise on every mic: about −75 dBFS.
const ROOM_NOISE: f32 = 3e-4;
/// Bleed of a talker into the other mic: −15 dB.
const BLEED: f32 = 0.178;
/// A strong VAD "speech" probability, and "no speech".
const SPEECH: Option<f32> = Some(0.9);
const NO_SPEECH: Option<f32> = Some(0.0);

fn new_mixer(num_strips: usize, max_block: usize) -> (Arc<MixerParams>, Mixer) {
    let params = Arc::new(MixerParams::new(num_strips));
    let mixer = Mixer::new(SR, max_block, num_strips, params.clone());
    (params, mixer)
}

/// Syllable envelope: 180 ms of voice, 70 ms gap, repeating.
fn syllable_env(i: usize) -> f32 {
    let period = SRU / 4;
    let on = SRU * 18 / 100;
    let pos = i % period;
    if pos < on {
        (std::f32::consts::PI * pos as f32 / on as f32).sin()
    } else {
        0.0
    }
}

/// Noise "voice" with syllables, peak `amp`.
fn talker(len: usize, seed: u32, amp: f32) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    (0..len).map(|i| rng.next_f32() * amp * syllable_env(i)).collect()
}

fn room(len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    (0..len).map(|_| rng.next_f32() * ROOM_NOISE).collect()
}

fn add(a: &[f32], b: &[f32], gain_b: f32) -> Vec<f32> {
    a.iter().zip(b.iter()).map(|(x, y)| x + y * gain_b).collect()
}

fn music(len: usize) -> Vec<f32> {
    let a = sine(len, 220.0, SR as f32, 0.08);
    let b = sine(len, 330.0, SR as f32, 0.06);
    let c = sine(len, 880.0, SR as f32, 0.04);
    a.iter().zip(b.iter()).zip(c.iter()).map(|((x, y), z)| x + y + z).collect()
}

/// Run `inputs` in blocks; returns the strip meters of every block.
fn run_meters(mixer: &mut Mixer, inputs: &[Vec<f32>], block: usize) -> Vec<Vec<StripMeters>> {
    let len = inputs.iter().map(|v| v.len()).max().unwrap_or(0);
    let mut blocks = Vec::new();
    let mut pos = 0;
    while pos < len {
        let end = (pos + block).min(len);
        let slices: Vec<&[f32]> = inputs.iter().map(|v| &v[pos..end]).collect();
        assert_eq!(mixer.process(&slices, end - pos), end - pos);
        blocks.push(mixer.strip_meters().to_vec());
        pos = end;
    }
    blocks
}

/// Attenuation (positive dB) of strip `s` over the last `secs` seconds:
/// (least, most).
fn gr_range(blocks: &[Vec<StripMeters>], s: usize, secs: f64, duck: bool) -> (f32, f32) {
    let count = ((secs * SR) as usize / BLOCK).max(1);
    blocks[blocks.len() - count..]
        .iter()
        .map(|b| -(if duck { b[s].duck_gr_db } else { b[s].automix_gr_db }))
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

/// Mic A hears `voice_a` plus bleed of `voice_b`, and the other way round.
fn two_mics(voice_a: &[f32], voice_b: &[f32], seed: u32) -> Vec<Vec<f32>> {
    let len = voice_a.len();
    let a = add(&add(voice_a, voice_b, BLEED), &room(len, seed), 1.0);
    let b = add(&add(voice_b, voice_a, BLEED), &room(len, seed + 1), 1.0);
    vec![a, b]
}

fn automix_pair() -> (Arc<MixerParams>, Mixer) {
    let (params, mut mixer) = new_mixer(2, BLOCK);
    for i in 0..2 {
        params.strips[i].automix.store(true, Relaxed);
        mixer.set_vad_override(i, SPEECH);
    }
    (params, mixer)
}

#[test]
fn one_talker_gets_the_gain_and_the_other_mic_drops() {
    let (_params, mut mixer) = automix_pair();
    let len = 3 * SRU;
    let silent = vec![0.0; len];

    // Talker on mic A.
    let blocks = run_meters(&mut mixer, &two_mics(&talker(len, 1, 0.3), &silent, 10), BLOCK);
    let (_, a_most) = gr_range(&blocks, 0, 1.0, false);
    let (b_least, _) = gr_range(&blocks, 1, 1.0, false);
    assert!(a_most < 1.0, "talking mic A attenuated by {a_most} dB");
    assert!(b_least > 10.0, "idle mic B only {b_least} dB down");

    // The talker moves to mic B: the gains swap.
    let blocks = run_meters(&mut mixer, &two_mics(&silent, &talker(len, 2, 0.3), 20), BLOCK);
    let (a_least, _) = gr_range(&blocks, 0, 1.0, false);
    let (_, b_most) = gr_range(&blocks, 1, 1.0, false);
    assert!(b_most < 1.0, "talking mic B attenuated by {b_most} dB");
    assert!(a_least > 10.0, "idle mic A only {a_least} dB down");
}

#[test]
fn nobody_talking_holds_the_last_mic_steady() {
    let (_params, mut mixer) = automix_pair();
    let silent = vec![0.0; 2 * SRU];
    run_meters(&mut mixer, &two_mics(&talker(2 * SRU, 3, 0.3), &silent, 30), BLOCK);

    // Room noise only for 3 s.
    let quiet = vec![0.0; 3 * SRU];
    let blocks = run_meters(&mut mixer, &two_mics(&quiet, &quiet, 40), BLOCK);
    // Once the last word has faded (about 1 s), the gains are frozen.
    for strip in 0..2 {
        let (lo, hi) = gr_range(&blocks, strip, 2.0, false);
        assert!(hi - lo < 0.5, "strip {strip} drifted {lo}..{hi} dB with nobody talking");
    }
    let (_, a_most) = gr_range(&blocks, 0, 2.0, false);
    let (b_least, _) = gr_range(&blocks, 1, 2.0, false);
    assert!(a_most < 1.0, "last talker's mic closed to {a_most} dB");
    assert!(b_least > 10.0, "other mic opened to {b_least} dB");
}

#[test]
fn a_fresh_group_with_nobody_talking_shares_equally() {
    let (_params, mut mixer) = automix_pair();
    let quiet = vec![0.0; SRU];
    let blocks = run_meters(&mut mixer, &two_mics(&quiet, &quiet, 50), BLOCK);
    for strip in 0..2 {
        let (lo, hi) = gr_range(&blocks, strip, 0.5, false);
        assert!((lo - 3.0).abs() < 0.3 && (hi - 3.0).abs() < 0.3, "strip {strip}: {lo}..{hi}");
    }
}

#[test]
fn music_dips_while_speech_is_present_and_recovers() {
    let (params, mut mixer) = new_mixer(2, BLOCK);
    params.strips[0].automix.store(true, Relaxed);
    params.strips[1].duck.store(true, Relaxed);
    params.strips[1].duck_depth_db.store(12.0, Relaxed);
    mixer.set_vad_override(0, SPEECH);
    let len = 2 * SRU;

    // Music alone: no dip.
    let blocks = run_meters(&mut mixer, &[room(len, 60), music(len)], BLOCK);
    let (_, most) = gr_range(&blocks, 1, 1.0, true);
    assert!(most < 0.5, "music dipped {most} dB with nobody talking");

    // Speech over the music: dips by the depth.
    let speech = add(&talker(len, 4, 0.3), &room(len, 61), 1.0);
    let blocks = run_meters(&mut mixer, &[speech, music(len)], BLOCK);
    let (lo, hi) = gr_range(&blocks, 1, 1.0, true);
    assert!((lo - 12.0).abs() < 1.0 && (hi - 12.0).abs() < 1.0, "dip {lo}..{hi} dB");
    assert!(blocks.last().unwrap()[0].speech, "talker strip should report speech");
    // The talker is not ducked, and as the only group member stays at unity.
    let (_, talker_gr) = gr_range(&blocks, 0, 1.0, false);
    assert!(talker_gr < 0.1, "single automix strip attenuated {talker_gr} dB");

    // Speech stops: back to 0 dB.
    let blocks = run_meters(&mut mixer, &[room(3 * SRU, 62), music(3 * SRU)], BLOCK);
    let (_, end) = gr_range(&blocks, 1, 0.1, true);
    assert!(end < 1.0, "music still {end} dB down after recovery");
}

#[test]
fn ducking_ignores_a_loud_non_speech_source_on_a_voice_mic() {
    let (params, mut mixer) = new_mixer(2, BLOCK);
    params.strips[0].automix.store(true, Relaxed);
    params.strips[1].duck.store(true, Relaxed);
    mixer.set_vad_override(0, NO_SPEECH);
    let len = 2 * SRU;
    let hum = sine(len, 200.0, SR as f32, 0.1);
    let blocks = run_meters(&mut mixer, &[hum, music(len)], BLOCK);
    let (_, most) = gr_range(&blocks, 1, 1.5, true);
    assert!(most < 0.1, "music dipped {most} dB for a non-speech source");
}

#[test]
fn speech_beats_louder_non_speech_in_the_group() {
    let len = 3 * SRU;
    // Mic A: 5 dB louder, no speech. Mic B: a talker.
    let inputs = vec![
        add(&talker(len, 5, 0.53), &room(len, 70), 1.0),
        add(&talker(len, 6, 0.3), &room(len, 71), 1.0),
    ];

    let (_params, mut mixer) = automix_pair();
    mixer.set_vad_override(0, NO_SPEECH);
    let blocks = run_meters(&mut mixer, &inputs, BLOCK);
    let (a_lo, _) = gr_range(&blocks, 0, 1.0, false);
    let (_, b_hi) = gr_range(&blocks, 1, 1.0, false);
    let mean = |s: usize| {
        let tail = &blocks[blocks.len() - SRU / BLOCK..];
        tail.iter().map(|b| -b[s].automix_gr_db).sum::<f32>() / tail.len() as f32
    };
    assert!(mean(1) + 1.0 < mean(0), "talker {} dB vs louder non-speech {} dB", mean(1), mean(0));
    assert!(b_hi < 6.0 && a_lo > 1.0, "a {a_lo}, b {b_hi}");

    // Control: with speech on both, the louder mic wins.
    let (_params, mut mixer) = automix_pair();
    let blocks = run_meters(&mut mixer, &inputs, BLOCK);
    let tail = &blocks[blocks.len() - SRU / BLOCK..];
    let mean_a = tail.iter().map(|b| -b[0].automix_gr_db).sum::<f32>() / tail.len() as f32;
    let mean_b = tail.iter().map(|b| -b[1].automix_gr_db).sum::<f32>() / tail.len() as f32;
    assert!(mean_a < mean_b, "louder speech mic should win: a {mean_a}, b {mean_b}");
}

#[test]
fn muted_or_faded_out_strip_leaves_the_group() {
    let len = 2 * SRU;
    for (mute, fader) in [(true, 0.0), (false, MIN_DB)] {
        let (params, mut mixer) = automix_pair();
        params.strips[0].mute.store(mute, Relaxed);
        params.strips[0].fader_db.store(fader, Relaxed);
        let blocks = run_meters(&mut mixer, &two_mics(&talker(len, 7, 0.3), &vec![0.0; len], 80), BLOCK);
        let (_, b_most) = gr_range(&blocks, 1, 1.0, false);
        assert!(b_most < 0.1, "only remaining member should be unity, got {b_most} dB");
    }
}

#[test]
fn duck_wins_when_both_flags_are_set() {
    let (params, mut mixer) = new_mixer(2, BLOCK);
    params.strips[0].automix.store(true, Relaxed);
    params.strips[1].automix.store(true, Relaxed);
    params.strips[1].duck.store(true, Relaxed);
    mixer.set_vad_override(0, SPEECH);
    mixer.set_vad_override(1, SPEECH);
    let len = 2 * SRU;
    let blocks = run_meters(&mut mixer, &[talker(len, 8, 0.3), music(len)], BLOCK);
    // Strip 0 is the only member (unity); strip 1 ducks instead of mixing.
    let (_, a_most) = gr_range(&blocks, 0, 1.0, false);
    let (_, b_mix) = gr_range(&blocks, 1, 1.0, false);
    let (b_duck, _) = gr_range(&blocks, 1, 1.0, true);
    assert!(a_most < 0.1 && b_mix < 0.1 && b_duck > 11.0, "{a_most} {b_mix} {b_duck}");
}

#[test]
fn automix_gain_reaches_post_fader_buses_only() {
    let (params, mut mixer) = automix_pair();
    // Only idle mic A reaches the buses: Stream post-fader, Monitor pre-fader.
    params.strips[0].send_db[BUS_MONITOR].store(0.0, Relaxed);
    params.strips[0].send_db[BUS_MAIN].store(MIN_DB, Relaxed);
    for bus in [BUS_MAIN, BUS_STREAM, BUS_MONITOR] {
        params.strips[1].send_db[bus].store(MIN_DB, Relaxed);
    }
    let len = 2 * SRU;
    let inputs = two_mics(&vec![0.0; len], &talker(len, 9, 0.3), 90);
    let mut stream = Vec::new();
    let mut monitor = Vec::new();
    for pos in (0..len).step_by(BLOCK) {
        let end = (pos + BLOCK).min(len);
        let slices: Vec<&[f32]> = inputs.iter().map(|v| &v[pos..end]).collect();
        mixer.process(&slices, end - pos);
        stream.extend_from_slice(mixer.output(BUS_STREAM).0);
        monitor.extend_from_slice(mixer.output(BUS_MONITOR).0);
    }
    let tail = SRU / 2;
    let rms = crate::dsp::test_util::rms;
    let src = rms(&inputs[0][len - tail..]);
    let mon = rms(&monitor[len - tail..]);
    let st = rms(&stream[len - tail..]);
    assert!((mon / src - 1.0).abs() < 0.15, "monitor {mon} vs input {src}");
    assert!(st / src < 0.32, "stream should be >10 dB down: {st} vs {src}");
}

#[test]
fn block_size_does_not_change_the_output() {
    let len = SRU;
    let inputs = vec![
        add(&talker(len, 11, 0.3), &room(len, 12), 1.0),
        add(&talker(len, 13, 0.1), &room(len, 14), 1.0),
        music(len),
    ];
    let render = |block: usize| -> Vec<f32> {
        let (params, mut mixer) = new_mixer(3, 1024);
        params.strips[0].automix.store(true, Relaxed);
        params.strips[1].automix.store(true, Relaxed);
        params.strips[2].duck.store(true, Relaxed);
        // No override: the real RNNoise VAD runs at 48 kHz.
        let mut out = Vec::with_capacity(len);
        let mut pos = 0;
        while pos < len {
            let end = (pos + block).min(len);
            let slices: Vec<&[f32]> = inputs.iter().map(|v| &v[pos..end]).collect();
            mixer.process(&slices, end - pos);
            out.extend_from_slice(mixer.output(BUS_STREAM).0);
            pos = end;
        }
        out
    };
    let reference = render(480);
    for block in [64, 1024, 333] {
        let diff = max_abs_diff(&reference, &render(block));
        assert!(diff < 1e-6, "block {block}: max diff {diff}");
    }
}

#[test]
fn silence_and_non_finite_input_stay_finite() {
    let (params, mut mixer) = new_mixer(3, BLOCK);
    params.strips[0].automix.store(true, Relaxed);
    params.strips[1].automix.store(true, Relaxed);
    params.strips[2].duck.store(true, Relaxed);
    let mut bad = vec![0.0f32; SRU / 2];
    bad[100] = f32::NAN;
    bad[200] = f32::INFINITY;
    let blocks = run_meters(&mut mixer, &[vec![0.0; SRU / 2], bad, vec![0.0; SRU / 2]], BLOCK);
    for b in blocks.iter() {
        for m in b.iter() {
            assert!(m.automix_gr_db.is_finite() && m.duck_gr_db.is_finite());
        }
    }
    // The other strips' gains stay finite: a NaN on one strip's input does
    // not poison the group (that strip's own audio path is out of scope).
    let quiet = vec![0.0; SRU / 2];
    let blocks = run_meters(&mut mixer, &[quiet.clone(), quiet.clone(), quiet], BLOCK);
    assert!(blocks.iter().all(|b| b.iter().all(|m| m.automix_gr_db.is_finite())));

    let (params, mut mixer) = new_mixer(2, BLOCK);
    params.strips[0].automix.store(true, Relaxed);
    params.strips[1].duck.store(true, Relaxed);
    let quiet = vec![0.0; SRU / 2];
    run_meters(&mut mixer, &[quiet.clone(), quiet], BLOCK);
    assert!(mixer.output(BUS_STREAM).0.iter().all(|v| v.is_finite()));
}

#[test]
fn live_readouts_are_published_to_the_params() {
    let (params, mut mixer) = automix_pair();
    let len = 2 * SRU;
    run_meters(&mut mixer, &two_mics(&talker(len, 15, 0.3), &vec![0.0; len], 100), BLOCK);
    assert!(params.strips[1].live.automix_gr_db.load(Relaxed) > 10.0);
    assert!(params.strips[0].live.automix_gr_db.load(Relaxed) < 1.0);
}

/// CPU probe (release build): 8 automix mics with the real RNNoise VAD plus
/// 2 ducked music strips. Run with
/// `cargo test --release --lib automix_cpu -- --ignored --nocapture`.
#[test]
#[ignore]
fn automix_cpu() {
    let strips = 10;
    let secs = 10;
    let len = secs * SRU;
    let inputs: Vec<Vec<f32>> = (0..strips)
        .map(|i| add(&talker(len, 200 + i as u32, 0.2), &room(len, 300 + i as u32), 1.0))
        .collect();
    let time = |automix: bool| {
        let (params, mut mixer) = new_mixer(strips, BLOCK);
        for i in 0..strips {
            if automix {
                params.strips[i].automix.store(i < 8, Relaxed);
                params.strips[i].duck.store(i >= 8, Relaxed);
            }
        }
        let started = std::time::Instant::now();
        run_meters(&mut mixer, &inputs, BLOCK);
        started.elapsed().as_secs_f64()
    };
    let off = time(false);
    let on = time(true);
    println!(
        "{secs} s of audio, {strips} strips: mixer {off:.3} s, with automix {on:.3} s; \
         automix adds {:.2}% of one core ({:.2}% per automix mic)",
        (on - off) / secs as f64 * 100.0,
        (on - off) / secs as f64 * 100.0 / 8.0
    );
}

#[test]
fn first_block_warms_the_vad_fft_on_the_audio_thread() {
    let (_params, mut mixer) = new_mixer(1, BLOCK);
    assert!(!mixer.automix_warmed());
    run_meters(&mut mixer, &[vec![0.0; BLOCK]], BLOCK);
    assert!(mixer.automix_warmed());
}

#[test]
fn muting_a_member_freezes_its_gain_instead_of_swelling() {
    let (params, mut mixer) = automix_pair();
    let len = 2 * SRU;
    let silent = vec![0.0; len];
    let blocks = run_meters(&mut mixer, &two_mics(&talker(len, 16, 0.3), &silent, 110), BLOCK);
    let (b_before, _) = gr_range(&blocks, 1, 0.5, false);
    assert!(b_before > 10.0);

    // Mute the idle mic B, keep talking, then unmute it.
    params.strips[1].mute.store(true, Relaxed);
    let blocks = run_meters(&mut mixer, &two_mics(&talker(SRU, 17, 0.3), &vec![0.0; SRU], 120), BLOCK);
    let (b_least, _) = gr_range(&blocks, 1, 1.0, false);
    assert!(b_least > 10.0, "muted member swelled to {b_least} dB");
    params.strips[1].mute.store(false, Relaxed);
    let blocks = run_meters(&mut mixer, &two_mics(&talker(SRU, 18, 0.3), &vec![0.0; SRU], 130), BLOCK);
    let (b_least, _) = gr_range(&blocks, 1, 1.0, false);
    assert!(b_least > 10.0, "unmuted member came back at {b_least} dB");
}
