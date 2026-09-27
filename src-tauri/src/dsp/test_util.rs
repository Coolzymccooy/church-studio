//! Deterministic test signals for DSP unit tests (no `rand` dependency).
#![allow(dead_code)]

/// Tiny linear congruential generator (Numerical Recipes constants).
pub struct Lcg(u32);

impl Lcg {
    pub fn new(seed: u32) -> Self {
        Lcg(seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453))
    }

    /// Uniform sample in [-1, 1).
    pub fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.0 >> 8) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0
    }
}

/// White noise with the given peak amplitude.
pub fn white_noise(len: usize, seed: u32, amp: f32) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    (0..len).map(|_| rng.next_f32() * amp).collect()
}

/// 440 Hz + 1.3 kHz sines plus pseudo-random noise at 48 kHz.
pub fn test_signal(len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    (0..len)
        .map(|i| {
            let t = i as f32 / 48_000.0;
            0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                + 0.2 * (2.0 * std::f32::consts::PI * 1_300.0 * t).sin()
                + 0.1 * rng.next_f32()
        })
        .collect()
}

pub fn sine(len: usize, freq: f32, sr: f32, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
        .collect()
}

pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
}

/// Run `process` over `input` split into blocks of `block` samples.
pub fn run_blocks<F: FnMut(&mut [f32])>(input: &[f32], block: usize, mut process: F) -> Vec<f32> {
    let mut out = input.to_vec();
    for chunk in out.chunks_mut(block.max(1)) {
        process(chunk);
    }
    out
}

pub fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
}
