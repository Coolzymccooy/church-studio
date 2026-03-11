/// Spectral dereverberation — two-component running-minimum estimator.
/// OLA with N=1024, HOP=256 (75% overlap), Hann window.
use rustfft::{FftPlanner, num_complex::Complex32};

const N: usize = 1024;
const HOP: usize = 256;
const HALF: usize = N / 2 + 1;

pub struct SpectralDereverb {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    ifft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,

    // Two-component reverb estimate (fast + slow)
    fast_min: Vec<f32>,   // short-term min (decays slower)
    slow_min: Vec<f32>,   // long-term min (very slow tail)
    fast_age: Vec<u32>,
    slow_age: Vec<u32>,

    // OLA I/O
    in_hist: Vec<f32>,    // last (N-HOP) input samples

    pub strength: f32,    // 0.0–1.0
    pub floor: f32,       // spectral floor (prevent over-suppression)
    pub enabled: bool,
}

impl SpectralDereverb {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        SpectralDereverb {
            fft: planner.plan_fft_forward(N),
            ifft: planner.plan_fft_inverse(N),
            window: hann(N),
            fast_min: vec![1e-6; HALF],
            slow_min: vec![1e-6; HALF],
            fast_age: vec![0; HALF],
            slow_age: vec![0; HALF],
            in_hist: vec![0.0; N - HOP],
            strength: 0.6,
            floor: 0.08,
            enabled: true,
        }
    }

    pub fn process_block(&mut self, buf: &mut Vec<f32>) {
        if !self.enabled {
            return;
        }

        let input = buf.clone();
        let out_len = input.len();
        let mut output = vec![0.0f32; out_len + N];
        let mut fft_buf = vec![Complex32::default(); N];

        let n_frames = (input.len() + HOP - 1) / HOP;

        for frame_idx in 0..n_frames {
            let offset = frame_idx * HOP;

            // Windowed analysis frame (prepend history)
            let hist_len = self.in_hist.len();
            for i in 0..N {
                let s = if i < hist_len {
                    self.in_hist[i]
                } else {
                    let src_i = offset + (i - hist_len);
                    if src_i < input.len() { input[src_i] } else { 0.0 }
                };
                fft_buf[i] = Complex32::new(s * self.window[i], 0.0);
            }

            // Update history for next frame
            {
                let src_start = offset;
                let new_hist: Vec<f32> = (0..hist_len)
                    .map(|i| {
                        let src_i = src_start + i + HOP;
                        if src_i < input.len() { input[src_i] }
                        else { 0.0 }
                    })
                    .collect();
                self.in_hist.copy_from_slice(&new_hist);
            }

            self.fft.process(&mut fft_buf);

            // Update reverb estimate and apply suppression
            for k in 0..HALF {
                let mag = fft_buf[k].norm();

                // Update fast running min
                if mag < self.fast_min[k] || self.fast_age[k] > 8 {
                    self.fast_min[k] = mag;
                    self.fast_age[k] = 0;
                } else {
                    self.fast_min[k] = self.fast_min[k] * 0.97 + mag * 0.03;
                    self.fast_age[k] += 1;
                }

                // Update slow running min
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

                let suppressed = (mag - self.strength * est).max(self.floor * mag);
                let gain = if mag > 1e-10 { suppressed / mag } else { self.floor };
                fft_buf[k] = fft_buf[k] * gain;
                if k > 0 && k < HALF - 1 {
                    fft_buf[N - k] = fft_buf[k].conj();
                }
            }

            self.ifft.process(&mut fft_buf);

            // OLA accumulate
            let norm = 2.0 / N as f32;
            for i in 0..N {
                let out_i = offset + i;
                if out_i < output.len() {
                    output[out_i] += fft_buf[i].re * self.window[i] * norm;
                }
            }
        }

        output.truncate(out_len);
        *buf = output;
    }
}

fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos())).collect()
}
