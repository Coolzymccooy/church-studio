//! Deterministic synthetic test material for the Hyper gate: speech-like
//! syllables, sustained church-keys music and steady noise. Test-only.
#![allow(dead_code)]

use crate::dsp::biquad::Biquad;
use crate::dsp::test_util::{rms, Lcg};
use std::f32::consts::PI;

pub const SR: f32 = 48_000.0;

/// Vowel formants (F1, F2, F3) in Hz: /a/ /i/ /u/ /e/ /o/.
const VOWELS: [[f32; 3]; 5] = [
    [730.0, 1_090.0, 2_440.0],
    [270.0, 2_290.0, 3_010.0],
    [300.0, 870.0, 2_240.0],
    [530.0, 1_840.0, 2_480.0],
    [570.0, 840.0, 2_410.0],
];

/// Scale `buf` so its RMS over the non-silent part is `rms_db` dBFS.
fn normalise(mut buf: Vec<f32>, rms_db: f32) -> Vec<f32> {
    let active: Vec<f32> = buf.iter().copied().filter(|s| s.abs() > 1e-4).collect();
    let current = rms(&active).max(1e-9);
    let gain = 10f32.powf(rms_db / 20.0) / current;
    for s in buf.iter_mut() {
        *s *= gain;
    }
    buf
}

/// Glottal-like source: band-limited sawtooth (harmonics 1/k up to 4 kHz).
fn glottal(phase: f32, f0: f32) -> f32 {
    let harmonics = (4_000.0 / f0).floor().max(1.0) as usize;
    let mut v = 0.0;
    for k in 1..=harmonics {
        v += (k as f32 * phase).sin() / k as f32;
    }
    v
}

/// Speech-like signal: voiced syllables through vowel formants with pitch
/// intonation, 4–5 syllables per second, gaps between them and occasional
/// fricatives. Active-part RMS is `rms_db` dBFS.
pub fn speech(len: usize, seed: u32, rms_db: f32) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    let mut out = vec![0.0f32; len];
    let mut t = 0usize;
    let mut phase = 0.0f32;
    let mut syllable = 0usize;
    while t < len {
        let voiced = (0.16 + 0.08 * rng.next_f32().abs()) * SR;
        let gap = (0.05 + 0.08 * rng.next_f32().abs()) * SR;
        let vowel = VOWELS[(rng.next_f32().abs() * 5.0) as usize % 5];
        let base_f0 = 115.0 + 25.0 * rng.next_f32();
        let mut formants: Vec<Biquad> = vowel
            .iter()
            .map(|&f| Biquad::bpf(f as f64, 90.0, SR as f64))
            .collect();
        let gains = [1.0f32, 0.6, 0.3];
        let voiced = voiced as usize;
        for i in 0..voiced {
            let idx = t + i;
            if idx >= len {
                break;
            }
            let pos = i as f32 / voiced as f32;
            let f0 = base_f0 * (1.0 + 0.12 * (PI * pos).sin() - 0.05 * pos);
            phase += 2.0 * PI * f0 / SR;
            if phase > 2.0 * PI {
                phase -= 2.0 * PI;
            }
            let src = glottal(phase, f0) + 0.02 * rng.next_f32();
            let mut y = 0.0;
            for (f, g) in formants.iter_mut().zip(gains.iter()) {
                y += f.tick(src) * g;
            }
            // Raised-cosine syllable envelope.
            let env = (PI * pos).sin().powf(0.6);
            out[idx] = y * env;
        }
        t += voiced;
        // A fricative ("s"/"sh") after every third syllable.
        syllable += 1;
        if syllable % 3 == 0 {
            let mut hp = Biquad::hpf(4_000.0, SR as f64);
            let fric = (0.06 * SR) as usize;
            for i in 0..fric {
                let idx = t + i;
                if idx >= len {
                    break;
                }
                let env = (PI * i as f32 / fric as f32).sin();
                out[idx] = hp.tick(rng.next_f32()) * 0.15 * env;
            }
            t += fric;
        }
        t += gap as usize;
    }
    normalise(out, rms_db)
}

/// Sustained "church keys" music: a bass note, a triad and a melody, all
/// harmonic tones with slow attack and gentle decay; chords change every
/// second, the melody every 250 ms. RMS `rms_db` dBFS.
pub fn music(len: usize, rms_db: f32) -> Vec<f32> {
    // C, Am, F, G as MIDI note numbers: bass, triad, melody pool.
    let chords: [[u8; 4]; 4] = [[48, 60, 64, 67], [45, 57, 60, 64], [41, 57, 60, 65], [43, 55, 59, 62]];
    let melody: [u8; 8] = [72, 74, 76, 79, 77, 76, 74, 72];
    let hz = |n: u8| 440.0 * 2f32.powf((n as f32 - 69.0) / 12.0);
    let tone = |f: f32, t: f32| -> f32 {
        let mut v = 0.0;
        for k in 1..=6 {
            let fk = f * k as f32;
            if fk < 10_000.0 {
                v += (2.0 * PI * fk * t).sin() / k as f32;
            }
        }
        v
    };
    let out: Vec<f32> = (0..len)
        .map(|i| {
            let t = i as f32 / SR;
            let chord = chords[(t as usize) % 4];
            let in_chord = t.fract();
            let chord_env = (in_chord / 0.03).min(1.0) * (1.0 - 0.3 * in_chord);
            let mut v = 0.0;
            for (j, &n) in chord.iter().enumerate() {
                let level = if j == 0 { 0.8 } else { 0.5 };
                v += level * tone(hz(n), t) * chord_env;
            }
            let m = (t * 4.0) as usize;
            let in_note = (t * 4.0).fract();
            let note_env = (in_note / 0.05).min(1.0) * (1.0 - 0.25 * in_note);
            v += 0.6 * tone(hz(melody[m % 8]), t) * note_env;
            v
        })
        .collect();
    normalise(out, rms_db)
}

/// Steady pink-ish noise (white noise through a gentle low-pass), RMS
/// `rms_db` dBFS: fans, air conditioning, room tone.
pub fn steady_noise(len: usize, seed: u32, rms_db: f32) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    let mut lp = 0.0f32;
    let out: Vec<f32> = (0..len)
        .map(|_| {
            let w = rng.next_f32();
            lp += 0.15 * (w - lp);
            0.5 * w + lp
        })
        .collect();
    normalise(out, rms_db)
}

/// Element-wise sum of two signals (length of the shorter).
pub fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}
