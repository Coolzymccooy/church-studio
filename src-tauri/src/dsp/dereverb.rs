/// Spectral dereverberation — two-component running-minimum estimator.
///
/// Runs on the streaming STFT engine (`stft.rs`): N=1024, HOP=256 (75%
/// overlap), Hann analysis + synthesis, state carried across callbacks, no
/// allocation in `process_block`, any block length. Fixed latency =
/// `latency_samples()` (1024 samples) whether enabled or not — when disabled
/// the audio passes through the same delay line so toggling never jumps in
/// time. The reverb estimate is only updated while the stage is enabled.
use super::stft::{StreamingStft, HALF};
use rustfft::num_complex::Complex32;

/// Per-bin reverb tail estimate (fast + slow running minimum).
struct ReverbEstimate {
    fast_min: Vec<f32>,
    slow_min: Vec<f32>,
    fast_age: Vec<u32>,
    slow_age: Vec<u32>,
}

impl ReverbEstimate {
    fn new() -> Self {
        ReverbEstimate {
            fast_min: vec![1e-6; HALF],
            slow_min: vec![1e-6; HALF],
            fast_age: vec![0; HALF],
            slow_age: vec![0; HALF],
        }
    }

    fn apply(&mut self, spec: &mut [Complex32], strength: f32, floor: f32) {
        for k in 0..HALF {
            let mag = spec[k].norm();

            // Fast running min
            if mag < self.fast_min[k] || self.fast_age[k] > 8 {
                self.fast_min[k] = mag;
                self.fast_age[k] = 0;
            } else {
                self.fast_min[k] = self.fast_min[k] * 0.97 + mag * 0.03;
                self.fast_age[k] += 1;
            }

            // Slow running min
            if mag < self.slow_min[k] || self.slow_age[k] > 12 {
                self.slow_min[k] = mag;
                self.slow_age[k] = 0;
            } else {
                self.slow_min[k] = self.slow_min[k] * 0.992 + mag * 0.008;
                self.slow_age[k] += 1;
            }

            // Reverb estimate = max of both components
            let est = (self.fast_min[k] * 1.2)
                .max(self.slow_min[k] * 1.4)
                .max(1e-10);

            let suppressed = (mag - strength * est).max(floor * mag);
            let gain = if mag > 1e-10 { suppressed / mag } else { floor };
            spec[k] *= gain;
        }
    }
}

pub struct SpectralDereverb {
    stft: StreamingStft,
    estimate: ReverbEstimate,
    pub strength: f32, // 0.0–1.0
    pub floor: f32,    // spectral floor (prevent over-suppression)
    pub enabled: bool,
}

impl SpectralDereverb {
    pub fn new() -> Self {
        SpectralDereverb {
            stft: StreamingStft::new(),
            estimate: ReverbEstimate::new(),
            strength: 0.6,
            floor: 0.08,
            enabled: true,
        }
    }

    pub fn latency_samples(&self) -> usize {
        self.stft.latency_samples()
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        let active = self.enabled;
        let strength = self.strength;
        let floor = self.floor;
        let estimate = &mut self.estimate;
        self.stft
            .process(buf, active, |spec| estimate.apply(spec, strength, floor));
    }
}

impl Default for SpectralDereverb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::{max_abs_diff, run_blocks, test_signal};

    const SR: usize = 48_000;

    #[test]
    fn block_size_invariance() {
        let input = test_signal(SR * 3, 21);
        let outputs: Vec<Vec<f32>> = [64usize, 256, 480, 1000, 4096]
            .iter()
            .map(|&block| {
                let mut d = SpectralDereverb::new();
                run_blocks(&input, block, |b| d.process_block(b))
            })
            .collect();
        for (i, out) in outputs.iter().enumerate().skip(1) {
            let diff = max_abs_diff(&outputs[0], out);
            assert!(diff < 1e-5, "run {i}: diff {diff}");
        }
    }

    #[test]
    fn zero_strength_is_perfect_reconstruction() {
        let input = test_signal(SR * 2, 4);
        let mut d = SpectralDereverb::new();
        d.strength = 0.0;
        let lat = d.latency_samples();
        let out = run_blocks(&input, 480, |b| d.process_block(b));
        for t in lat..out.len() {
            assert!((out[t] - input[t - lat]).abs() < 1e-4, "t={t}");
        }
    }

    #[test]
    fn disabled_passes_through_with_same_latency() {
        let input = test_signal(SR, 6);
        let mut d = SpectralDereverb::new();
        d.enabled = false;
        let lat = d.latency_samples();
        let out = run_blocks(&input, 1000, |b| d.process_block(b));
        for t in lat..out.len() {
            assert!((out[t] - input[t - lat]).abs() < 1e-4, "t={t}");
        }
    }

    #[test]
    fn no_nan_on_silence_or_full_scale() {
        let mut d = SpectralDereverb::new();
        let silence = vec![0.0f32; SR];
        let out = run_blocks(&silence, 256, |b| d.process_block(b));
        assert!(out.iter().all(|s| s.is_finite()));

        let full: Vec<f32> = (0..SR).map(|i| if (i / 50) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = run_blocks(&full, 256, |b| d.process_block(b));
        assert!(out.iter().all(|s| s.is_finite()));
    }
}
