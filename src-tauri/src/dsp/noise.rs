/// FFT spectral subtraction noise reducer.
/// Operates on full blocks with overlap-add using a captured noise profile.
use rustfft::{num_complex::Complex32, FftPlanner};

const N: usize = 1024;
const HOP: usize = 256;
const HALF: usize = N / 2 + 1;

pub struct SpectralDenoiser {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    ifft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    noise_pow: Vec<f32>,
    pub has_profile: bool,
    pub alpha: f32,
    pub beta: f32,
    pub enabled: bool,
}

impl SpectralDenoiser {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(N);
        let ifft = planner.plan_fft_inverse(N);

        let window: Vec<f32> = (0..N)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (N - 1) as f32).cos()))
            .collect();

        Self {
            fft,
            ifft,
            window,
            noise_pow: vec![0.0; HALF],
            has_profile: false,
            alpha: 1.5,
            beta: 0.05,
            enabled: false,
        }
    }

    pub fn capture_profile(&mut self, samples: &[f32]) {
        let n_frames = samples.len().div_ceil(HOP);
        if n_frames == 0 {
            return;
        }

        let mut power = vec![0.0f64; HALF];
        let mut buf = vec![Complex32::default(); N];

        for frame in 0..n_frames {
            let offset = frame * HOP;
            for i in 0..N {
                let sample = samples.get(offset + i).copied().unwrap_or(0.0);
                buf[i] = Complex32::new(sample * self.window[i], 0.0);
            }

            self.fft.process(&mut buf);

            for k in 0..HALF {
                power[k] += buf[k].norm_sqr() as f64;
            }
        }

        for (dst, src) in self.noise_pow.iter_mut().zip(power.iter()) {
            *dst = (*src / n_frames as f64) as f32;
        }

        self.has_profile = true;
        self.enabled = true;
    }

    pub fn process_block(&mut self, buf: &mut Vec<f32>) {
        if !self.enabled || !self.has_profile || buf.is_empty() {
            return;
        }

        let input = buf.clone();
        let n_frames = input.len().div_ceil(HOP);
        let mut output = vec![0.0f32; input.len() + N];
        let mut fft_buf = vec![Complex32::default(); N];

        for frame_idx in 0..n_frames {
            let offset = frame_idx * HOP;

            for i in 0..N {
                let sample = input.get(offset + i).copied().unwrap_or(0.0);
                fft_buf[i] = Complex32::new(sample * self.window[i], 0.0);
            }

            self.fft.process(&mut fft_buf);

            for k in 0..HALF {
                let mag = fft_buf[k].norm();
                let noise_mag = self.noise_pow[k].sqrt();
                let suppressed = (mag - self.alpha * noise_mag).max(self.beta * mag);
                let gain = if mag > 1e-10 { suppressed / mag } else { self.beta };
                fft_buf[k] *= gain;

                if k > 0 && k < HALF - 1 {
                    fft_buf[N - k] = fft_buf[k].conj();
                }
            }

            self.ifft.process(&mut fft_buf);

            let norm = 2.0 / N as f32;
            for i in 0..N {
                let out_i = offset + i;
                if out_i < output.len() {
                    output[out_i] += fft_buf[i].re * self.window[i] * norm;
                }
            }
        }

        output.truncate(input.len());
        *buf = output;
    }
}
