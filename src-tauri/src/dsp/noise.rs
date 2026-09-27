/// FFT spectral subtraction noise reducer using a captured noise profile.
///
/// Runs on the streaming STFT engine (`stft.rs`): N=1024, HOP=256, Hann
/// analysis + synthesis, state carried across callbacks, no allocation in
/// `process_block`, any block length. Fixed latency = `latency_samples()`
/// (1024 samples) whether the stage is enabled or not — when disabled (or
/// before a profile exists) the audio passes through the same delay line, so
/// toggling never jumps in time.
use super::stft::{periodic_hann, StreamingStft, HALF, HOP, N};
use rustfft::{num_complex::Complex32, FftPlanner};

pub struct SpectralDenoiser {
    stft: StreamingStft,
    /// sqrt of the captured noise power per bin.
    noise_mag: Vec<f32>,
    pub has_profile: bool,
    pub alpha: f32,
    pub beta: f32,
    pub enabled: bool,
}

impl SpectralDenoiser {
    pub fn new() -> Self {
        Self {
            stft: StreamingStft::new(),
            noise_mag: vec![0.0; HALF],
            has_profile: false,
            alpha: 1.5,
            beta: 0.05,
            enabled: false,
        }
    }

    pub fn latency_samples(&self) -> usize {
        self.stft.latency_samples()
    }

    /// Install a noise power spectrum (HALF bins) computed by
    /// [`compute_noise_profile`]. Does not allocate, so it is safe to call from
    /// the audio thread. Returns false if the profile has the wrong length.
    pub fn set_profile(&mut self, noise_pow: &[f32]) -> bool {
        if noise_pow.len() != HALF {
            return false;
        }
        for (dst, &p) in self.noise_mag.iter_mut().zip(noise_pow.iter()) {
            *dst = if p.is_finite() { p.max(0.0).sqrt() } else { 0.0 };
        }
        self.has_profile = true;
        self.enabled = true;
        true
    }

    /// Convenience: compute and install a profile in one step (allocates).
    pub fn capture_profile(&mut self, samples: &[f32]) -> bool {
        match compute_noise_profile(samples) {
            Some(profile) => self.set_profile(&profile),
            None => false,
        }
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        let active = self.enabled && self.has_profile;
        let noise_mag = &self.noise_mag;
        let alpha = self.alpha;
        let beta = self.beta;

        self.stft.process(buf, active, |spec| {
            for k in 0..HALF {
                let mag = spec[k].norm();
                let suppressed = (mag - alpha * noise_mag[k]).max(beta * mag);
                let gain = if mag > 1e-10 { suppressed / mag } else { beta };
                spec[k] *= gain;
            }
        });
    }
}

impl Default for SpectralDenoiser {
    fn default() -> Self {
        Self::new()
    }
}

/// Average noise power spectrum (HALF bins) of `samples`, using the same
/// window and scaling as the processing path. Allocates — call it off the
/// audio thread. Returns None if fewer than N samples are supplied.
pub fn compute_noise_profile(samples: &[f32]) -> Option<Vec<f32>> {
    if samples.len() < N {
        return None;
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N);
    let window = periodic_hann(N);
    let mut power = vec![0.0f64; HALF];
    let mut buf = vec![Complex32::new(0.0, 0.0); N];
    let mut frames = 0usize;
    let mut offset = 0usize;

    while offset + N <= samples.len() {
        for i in 0..N {
            buf[i] = Complex32::new(samples[offset + i] * window[i], 0.0);
        }
        fft.process(&mut buf[..]);
        for k in 0..HALF {
            power[k] += buf[k].norm_sqr() as f64;
        }
        frames += 1;
        offset += HOP;
    }

    if frames == 0 {
        return None;
    }
    Some(power.iter().map(|&p| (p / frames as f64) as f32).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::{max_abs_diff, rms, run_blocks, test_signal, white_noise};

    const SR: usize = 48_000;

    fn denoiser_with_white_profile() -> SpectralDenoiser {
        let mut d = SpectralDenoiser::new();
        assert!(d.capture_profile(&white_noise(SR, 7, 0.1)));
        d
    }

    #[test]
    fn block_size_invariance() {
        let input = test_signal(SR * 3, 11);
        let outputs: Vec<Vec<f32>> = [64usize, 256, 480, 1000, 4096]
            .iter()
            .map(|&block| {
                let mut d = denoiser_with_white_profile();
                run_blocks(&input, block, |b| d.process_block(b))
            })
            .collect();
        for (i, out) in outputs.iter().enumerate().skip(1) {
            let diff = max_abs_diff(&outputs[0], out);
            assert!(diff < 1e-5, "run {i}: diff {diff}");
        }
    }

    #[test]
    fn zero_profile_is_perfect_reconstruction() {
        let input = test_signal(SR * 2, 3);
        let mut d = SpectralDenoiser::new();
        assert!(d.set_profile(&[0.0f32; HALF]));
        let lat = d.latency_samples();
        let out = run_blocks(&input, 480, |b| d.process_block(b));
        for t in lat..out.len() {
            assert!((out[t] - input[t - lat]).abs() < 1e-4, "t={t}");
        }
        assert!(out[..lat].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn disabled_passes_through_with_same_latency() {
        let input = test_signal(SR, 5);
        let mut d = denoiser_with_white_profile();
        d.enabled = false;
        let lat = d.latency_samples();
        let out = run_blocks(&input, 333, |b| d.process_block(b));
        for t in lat..out.len() {
            assert!((out[t] - input[t - lat]).abs() < 1e-4, "t={t}");
        }
    }

    #[test]
    fn no_nan_on_silence_or_full_scale() {
        let mut d = denoiser_with_white_profile();
        let silence = vec![0.0f32; SR];
        let out = run_blocks(&silence, 256, |b| d.process_block(b));
        assert!(out.iter().all(|s| s.is_finite()));

        let full: Vec<f32> = (0..SR).map(|i| if (i / 50) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = run_blocks(&full, 256, |b| d.process_block(b));
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn white_noise_is_reduced_by_more_than_6_db() {
        let mut d = denoiser_with_white_profile();
        let noise = white_noise(SR * 3, 8, 0.1);
        let out = run_blocks(&noise, 512, |b| d.process_block(b));
        let skip = d.latency_samples() + N;
        let in_rms = rms(&noise[skip..]);
        let out_rms = rms(&out[skip..]);
        let reduction_db = 20.0 * (in_rms / out_rms.max(1e-12)).log10();
        assert!(reduction_db > 6.0, "only {reduction_db} dB");
    }
}
