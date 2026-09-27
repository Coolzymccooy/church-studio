/// FFT spectral subtraction noise reducer using a captured noise profile.
///
/// Runs on the streaming STFT engine (`stft.rs`): N=1024, HOP=256, Hann
/// analysis + synthesis, state carried across callbacks, no allocation in
/// `process_block`, any block length.
///
/// Latency: 1024 samples (N) while active. When disabled, or before a profile
/// exists, the stage is a zero-latency passthrough (`latency_samples()` = 0)
/// and does no work. Switching it on resets the STFT and crossfades from dry
/// to wet (see `stage_switch.rs`); switching off crossfades back to dry.
use super::stage_switch::StageSwitch;
use super::stft::{periodic_hann, StreamingStft, HALF, HOP, N};
use rustfft::{num_complex::Complex32, FftPlanner};

pub struct SpectralDenoiser {
    stft: StreamingStft,
    switch: StageSwitch,
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
            switch: StageSwitch::new(N),
            noise_mag: vec![0.0; HALF],
            has_profile: false,
            alpha: 1.5,
            beta: 0.05,
            enabled: false,
        }
    }

    fn wants_active(&self) -> bool {
        self.enabled && self.has_profile
    }

    /// Current processing latency: N when active, 0 when bypassed.
    pub fn latency_samples(&self) -> usize {
        if self.wants_active() {
            self.stft.latency_samples()
        } else {
            0
        }
    }

    /// Drop the streaming state; the next active block fades in from dry.
    /// Used when the whole chain comes back from bypass.
    pub fn restart(&mut self) {
        self.switch.force_restart();
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
    #[allow(dead_code)]
    pub fn capture_profile(&mut self, samples: &[f32]) -> bool {
        match compute_noise_profile(samples) {
            Some(profile) => self.set_profile(&profile),
            None => false,
        }
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        if self.switch.update(self.wants_active()) {
            self.stft.reset();
        }
        if !self.switch.is_running() {
            return; // zero-latency passthrough
        }

        let noise_mag = &self.noise_mag;
        let alpha = self.alpha;
        let beta = self.beta;
        let mut shape = |spec: &mut [Complex32]| {
            for k in 0..HALF {
                let mag = spec[k].norm();
                let suppressed = (mag - alpha * noise_mag[k]).max(beta * mag);
                let gain = if mag > 1e-10 { suppressed / mag } else { beta };
                spec[k] *= gain;
            }
        };

        for s in buf.iter_mut() {
            if !self.switch.is_running() {
                break; // fade-out finished mid-block: rest stays dry
            }
            let x = *s;
            let wet = self.stft.tick(x, &mut shape);
            let mix = self.switch.next_mix();
            *s = x + (wet - x) * mix;
        }
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
    use crate::dsp::stage_switch::XFADE_SAMPLES;
    use crate::dsp::test_util::{max_abs_diff, rms, run_blocks, sine, test_signal, white_noise};

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
    fn block_size_invariance_when_disabled() {
        let input = test_signal(SR, 12);
        let outputs: Vec<Vec<f32>> = [64usize, 256, 480, 1000, 4096]
            .iter()
            .map(|&block| {
                let mut d = denoiser_with_white_profile();
                d.enabled = false;
                run_blocks(&input, block, |b| d.process_block(b))
            })
            .collect();
        for out in outputs.iter() {
            assert_eq!(out, &input);
        }
    }

    #[test]
    fn zero_profile_is_perfect_reconstruction() {
        let input = test_signal(SR * 2, 3);
        let mut d = SpectralDenoiser::new();
        assert!(d.set_profile(&[0.0f32; HALF]));
        let lat = d.latency_samples();
        assert_eq!(lat, N);
        let out = run_blocks(&input, 480, |b| d.process_block(b));
        // Dry while the STFT fills, then a crossfade, then input delayed by N.
        assert_eq!(&out[..lat], &input[..lat]);
        for t in lat + XFADE_SAMPLES..out.len() {
            assert!((out[t] - input[t - lat]).abs() < 1e-4, "t={t}");
        }
    }

    #[test]
    fn disabled_is_exact_zero_latency_passthrough() {
        let input = test_signal(SR, 5);
        let mut d = denoiser_with_white_profile();
        d.enabled = false;
        assert_eq!(d.latency_samples(), 0);
        let out = run_blocks(&input, 333, |b| d.process_block(b));
        assert_eq!(out, input);

        let mut no_profile = SpectralDenoiser::new();
        no_profile.enabled = true;
        assert_eq!(no_profile.latency_samples(), 0);
        let out = run_blocks(&input, 333, |b| no_profile.process_block(b));
        assert_eq!(out, input);
    }

    #[test]
    fn toggling_does_not_click() {
        let input = sine(SR * 2, 440.0, SR as f32, 0.5);
        let mut d = SpectralDenoiser::new();
        assert!(d.set_profile(&[0.0f32; HALF]));
        d.enabled = false;
        let mut out = Vec::with_capacity(input.len());
        for (i, chunk) in input.chunks(480).enumerate() {
            d.enabled = (i / 20) % 2 == 1; // toggle every 200 ms
            let mut block = chunk.to_vec();
            d.process_block(&mut block);
            out.extend_from_slice(&block);
        }
        // A 440 Hz sine at 0.5 moves < 0.03 per sample; the crossfade adds at
        // most 2 × 0.5 / 480. Anything larger is a click.
        let max_step = out.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(max_step < 0.04, "step {max_step}");
    }

    #[test]
    fn restart_after_bypass_fades_back_in() {
        let input = test_signal(SR, 8);
        let mut d = SpectralDenoiser::new();
        assert!(d.set_profile(&[0.0f32; HALF]));
        let _ = run_blocks(&input, 512, |b| d.process_block(b));
        d.restart();
        let out = run_blocks(&input, 512, |b| d.process_block(b));
        assert_eq!(&out[..N], &input[..N]);
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
