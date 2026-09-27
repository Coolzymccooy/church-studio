/// Streaming short-time Fourier transform engine shared by the spectral stages
/// (noise reduction, dereverb).
///
/// N = 1024, HOP = 256 (75% overlap), periodic Hann window applied at analysis
/// AND synthesis. For a periodic Hann window at 75% overlap the sum of the
/// squared, shifted windows is exactly 1.5, so the synthesis scale is 1 / 1.5
/// (plus 1 / N for rustfft's unnormalised inverse).
///
/// State (input history, overlap-add accumulator, finished output hop) is kept
/// across calls, so `process` accepts blocks of ANY length and the output does
/// not depend on how the stream was chopped into blocks. Everything is
/// allocated in `new()`; `process` never allocates.
///
/// Latency: exactly `N` samples. Output sample `t` is input sample `t - N`
/// after spectral processing (zeros for `t < N`).
///
/// When `active == false` the frame skips the FFT and adds `x · w²` to the
/// overlap-add accumulator instead. That reconstructs the input perfectly with
/// the SAME latency, so switching a stage on or off never jumps in time and the
/// overlap-add gives a natural N-sample crossfade between processed and dry.
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

pub const N: usize = 1024;
pub const HOP: usize = 256;
pub const HALF: usize = N / 2 + 1;

/// Sum over the 4 overlapping frames of w²[n] for a periodic Hann window.
const COLA_SUM: f32 = 1.5;

pub struct StreamingStft {
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    /// Last N input samples, oldest first.
    frame: Vec<f32>,
    /// New input samples collected for the next hop.
    pending: Vec<f32>,
    /// Overlap-add accumulator aligned to the start of the current frame.
    out_acc: Vec<f32>,
    /// Finished output samples played out during the next hop.
    ready: Vec<f32>,
    hop_pos: usize,
    spec: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl StreamingStft {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N);
        let ifft = planner.plan_fft_inverse(N);
        let scratch_len = fft
            .get_inplace_scratch_len()
            .max(ifft.get_inplace_scratch_len())
            .max(1);

        StreamingStft {
            fft,
            ifft,
            window: periodic_hann(N),
            frame: vec![0.0; N],
            pending: vec![0.0; HOP],
            out_acc: vec![0.0; N],
            ready: vec![0.0; HOP],
            hop_pos: 0,
            spec: vec![Complex32::new(0.0, 0.0); N],
            scratch: vec![Complex32::new(0.0, 0.0); scratch_len],
        }
    }

    /// Fixed processing latency in samples.
    pub fn latency_samples(&self) -> usize {
        N
    }

    /// Process `buf` in place.
    ///
    /// `shape` receives the full N-bin spectrum of each frame and must modify
    /// bins `0..HALF` only; the engine restores Hermitian symmetry afterwards.
    /// It is only called when `active` is true.
    pub fn process<F>(&mut self, buf: &mut [f32], active: bool, mut shape: F)
    where
        F: FnMut(&mut [Complex32]),
    {
        for s in buf.iter_mut() {
            let x = *s;
            *s = self.ready[self.hop_pos];
            self.pending[self.hop_pos] = x;
            self.hop_pos += 1;
            if self.hop_pos == HOP {
                self.hop_pos = 0;
                self.run_frame(active, &mut shape);
            }
        }
    }

    fn run_frame<F>(&mut self, active: bool, shape: &mut F)
    where
        F: FnMut(&mut [Complex32]),
    {
        // Slide the analysis frame by one hop and append the new samples.
        self.frame.copy_within(HOP.., 0);
        self.frame[N - HOP..].copy_from_slice(&self.pending[..]);

        if active {
            for i in 0..N {
                self.spec[i] = Complex32::new(self.frame[i] * self.window[i], 0.0);
            }
            self.fft
                .process_with_scratch(&mut self.spec[..], &mut self.scratch[..]);

            shape(&mut self.spec[..]);

            // Real signal: keep the spectrum Hermitian.
            for k in 1..HALF - 1 {
                self.spec[N - k] = self.spec[k].conj();
            }
            self.spec[0].im = 0.0;
            self.spec[HALF - 1].im = 0.0;

            self.ifft
                .process_with_scratch(&mut self.spec[..], &mut self.scratch[..]);

            let scale = 1.0 / (N as f32 * COLA_SUM);
            for i in 0..N {
                self.out_acc[i] += self.spec[i].re * self.window[i] * scale;
            }
        } else {
            let scale = 1.0 / COLA_SUM;
            for i in 0..N {
                let w = self.window[i];
                self.out_acc[i] += self.frame[i] * w * w * scale;
            }
        }

        // The first hop of the accumulator has received all 4 overlapping
        // frames — hand it to the output and slide the accumulator.
        self.ready.copy_from_slice(&self.out_acc[..HOP]);
        self.out_acc.copy_within(HOP.., 0);
        for v in self.out_acc[N - HOP..].iter_mut() {
            *v = 0.0;
        }
    }
}

impl Default for StreamingStft {
    fn default() -> Self {
        Self::new()
    }
}

/// Periodic Hann window (denominator N, not N − 1) — COLA-exact for w² at 75%.
pub fn periodic_hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let phase = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            (0.5 - 0.5 * phase.cos()) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_hann_squared_is_cola_at_75_percent() {
        let w = periodic_hann(N);
        for n in 0..HOP {
            let sum: f32 = (0..N / HOP).map(|k| w[n + k * HOP].powi(2)).sum();
            assert!((sum - COLA_SUM).abs() < 1e-5, "sum {sum} at {n}");
        }
    }

    #[test]
    fn identity_shape_reconstructs_with_latency_n() {
        let input = crate::dsp::test_util::test_signal(48_000, 1);
        let mut out = input.clone();
        let mut stft = StreamingStft::new();
        stft.process(&mut out, true, |_spec| {});
        for t in 0..out.len() {
            let expected = if t >= N { input[t - N] } else { 0.0 };
            assert!((out[t] - expected).abs() < 1e-4, "t={t}");
        }
    }

    #[test]
    fn inactive_path_reconstructs_with_same_latency() {
        let input = crate::dsp::test_util::test_signal(48_000, 2);
        let mut out = input.clone();
        let mut stft = StreamingStft::new();
        stft.process(&mut out, false, |_spec| {});
        for t in 0..out.len() {
            let expected = if t >= N { input[t - N] } else { 0.0 };
            assert!((out[t] - expected).abs() < 1e-5, "t={t}");
        }
    }
}
