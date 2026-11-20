//! A/B render of the whole voice chain in balanced and speech-only mode.
//! Ignored by default; run it in a release build to listen to the result:
//!
//! ```text
//! HYPERGATE_DEMO_OUT=/tmp/hg HYPERGATE_DEMO_SPEECH=speech48k.wav \
//!   cargo test --release --lib render_hyper_gate_ab -- --ignored --nocapture
//! ```
//!
//! `HYPERGATE_DEMO_SPEECH` (optional) is a 48 kHz 16-bit mono WAV; without it
//! synthetic speech is used. Writes input.wav, balanced.wav and
//! speech_only.wav and prints the level of each section per mode.
use super::signals::{mix, music, speech, steady_noise, SR};
use crate::dsp::test_util::rms;
use crate::dsp::{DspChain, DspParams};
use std::path::Path;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

const SECTION: usize = SR as usize * 8;

fn read_wav_mono16(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read speech wav");
    let data = bytes.windows(4).position(|w| w == b"data").expect("data chunk") + 8;
    bytes[data..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
        .collect()
}

fn write_wav_mono16(path: &Path, samples: &[f32]) {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(SR as u32).to_le_bytes());
    out.extend_from_slice(&(SR as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out).expect("write wav");
}

/// Speech section: the given recording (looped to 8 s, active part at
/// −20 dBFS) or synthetic speech.
fn speech_section() -> Vec<f32> {
    let Some(path) = std::env::var_os("HYPERGATE_DEMO_SPEECH") else {
        return speech(SECTION, 41, -20.0);
    };
    let raw = read_wav_mono16(Path::new(&path));
    let active: Vec<f32> = raw.iter().copied().filter(|s| s.abs() > 1e-3).collect();
    let gain = 0.1 / rms(&active).max(1e-6);
    (0..SECTION).map(|i| raw[i % raw.len()] * gain).collect()
}

/// Same settings `syncNativeParams` sends with Reduction on. Returns the
/// output and the gate gain at the end of each 480-sample block.
fn render(input: &[f32], speech_only: bool) -> (Vec<f32>, Vec<f32>) {
    let params = Arc::new(DspParams::defaults());
    params.gate_enabled.store(true, Relaxed);
    params.noise_enabled.store(true, Relaxed);
    params.neural_enabled.store(true, Relaxed);
    params.gate_threshold_db.store(-45.0, Relaxed);
    params.hyper.speech_only.store(speech_only, Relaxed);
    let mut chain = DspChain::new(SR as f64);
    let mut out = input.to_vec();
    let mut gains = Vec::with_capacity(input.len() / 480 + 1);
    for block in out.chunks_mut(480) {
        chain.sync_params(&params);
        chain.process_block(block, false);
        gains.push(chain.last_gate_gain);
    }
    (out, gains)
}

#[test]
#[ignore = "writes WAV files; run by hand in a release build"]
fn render_hyper_gate_ab() {
    let out_dir = std::env::var_os("HYPERGATE_DEMO_OUT").expect("set HYPERGATE_DEMO_OUT");
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir_all(out_dir).expect("create output dir");

    let voice = speech_section();
    let keys = music(SECTION, -20.0);
    let room = steady_noise(SECTION, 43, -30.0);
    let voice_over_keys = mix(&voice, &music(SECTION, -32.0));
    let sections = [
        ("speech", voice),
        ("music", keys),
        ("steady noise", room),
        ("speech over quiet music", voice_over_keys),
    ];
    let input: Vec<f32> = sections.iter().flat_map(|(_, s)| s.iter().copied()).collect();

    let (balanced, gate_b) = render(&input, false);
    let (speech_only, gate_s) = render(&input, true);
    write_wav_mono16(&out_dir.join("input.wav"), &input);
    write_wav_mono16(&out_dir.join("balanced.wav"), &balanced);
    write_wav_mono16(&out_dir.join("speech_only.wav"), &speech_only);

    let db = |s: &[f32]| 20.0 * rms(s).max(1e-9).log10();
    let skip = SR as usize * 2; // settle inside each section
    let mean_gain = |g: &[f32], i: usize| {
        let blocks = &g[(i * SECTION + skip) / 480..((i + 1) * SECTION) / 480];
        blocks.iter().sum::<f32>() / blocks.len() as f32
    };
    eprintln!("section                    input  balanced  speech-only   gate gain b / s");
    for (i, (name, _)) in sections.iter().enumerate() {
        let range = i * SECTION + skip..(i + 1) * SECTION;
        eprintln!(
            "{name:<24} {:>7.1} {:>9.1} {:>12.1} {:>10.2} / {:.2}",
            db(&input[range.clone()]),
            db(&balanced[range.clone()]),
            db(&speech_only[range]),
            mean_gain(&gate_b, i),
            mean_gain(&gate_s, i),
        );
    }
}
