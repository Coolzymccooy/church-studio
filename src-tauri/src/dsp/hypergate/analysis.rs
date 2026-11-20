//! Per-frame spectral features for the Hyper gate (native port of
//! `public/worklets/hypergate-worklet.js`).
//!
//! Samples are pushed one at a time; every `frame_len()` samples (512 at
//! 44.1/48 kHz, about 10.7 ms) the frame is Hann-windowed, transformed and
//! reduced to `FrameFeatures`. Frames do not overlap, as in the worklet.
//! Allocation happens only in `new`; `push` is allocation- and lock-free.
//!
//! Units differ from the worklet on purpose: the worklet mixed raw FFT
//! magnitudes with thresholds tuned for the main thread's byte spectrum, so
//! here every level is in dBFS (a full-scale sine reads 0 dB in its bin),
//! which keeps the thresholds meaningful at any mic gain.
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

/// Fingerprint bands, Hz (same eight bands as the browser fingerprint).
pub const FP_BANDS: [(f32, f32); 8] = [
    (40.0, 180.0),
    (180.0, 400.0),
    (400.0, 900.0),
    (900.0, 2_000.0),
    (2_000.0, 3_500.0),
    (3_500.0, 5_000.0),
    (5_000.0, 8_000.0),
    (8_000.0, 12_000.0),
];
pub const NUM_FP_BANDS: usize = FP_BANDS.len();

const FLOOR_DB: f32 = -120.0;
const FORMANT_PEAK_DB: f32 = 3.0;
const FORMANT_SPREAD_DB: f32 = 30.0;

#[derive(Clone, Copy, Debug)]
pub struct FrameFeatures {
    /// Frame RMS, dBFS.
    pub level_db: f32,
    /// Mean power in the voice band (300–3400 Hz), dB.
    pub voice_db: f32,
    /// Voice band over the noise bands (40–180 Hz and 6–12 kHz), dB.
    pub voice_ratio_db: f32,
    /// Share of the 40 Hz–12 kHz power that sits in the voice band (0..1).
    pub voice_share: f32,
    /// Energy sits above 40 % of Nyquist: hiss, not a voice.
    pub high_only_noise: bool,
    /// Spectral flatness (magnitude, 200–5000 Hz): ~0.85 for white noise,
    /// well below 0.5 for voiced speech.
    pub flatness: f32,
    /// Zero crossings per sample.
    pub zcr: f32,
    /// Formant structure: 0 when absent, else peaks (2–3) + spread.
    pub formant_score: f32,
    /// Fingerprint band powers, dB.
    pub bands_db: [f32; NUM_FP_BANDS],
    /// Magnitude-weighted spectral centroid, 40 Hz–12 kHz, Hz.
    pub centroid_hz: f32,
}

impl FrameFeatures {
    pub fn silent() -> Self {
        FrameFeatures {
            level_db: FLOOR_DB,
            voice_db: FLOOR_DB,
            voice_ratio_db: 0.0,
            voice_share: 0.0,
            high_only_noise: false,
            flatness: 1.0,
            zcr: 0.0,
            formant_score: 0.0,
            bands_db: [FLOOR_DB; NUM_FP_BANDS],
            centroid_hz: 0.0,
        }
    }
}

pub struct FrameAnalyser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    frame: Vec<f32>,
    fill: usize,
    spec: Vec<Complex32>,
    scratch: Vec<Complex32>,
    /// |X| / (N/4): a sine of amplitude A reads A in its bin.
    mag: Vec<f32>,
    bin_hz: f32,
    features: FrameFeatures,
}

/// Frame length for a sample rate: ~10.7 ms rounded to a power of two
/// (512 at 44.1/48 kHz, 1024 at 88.2/96 kHz).
pub fn frame_len_for(sr: f64) -> usize {
    ((sr * 512.0 / 48_000.0).round() as usize).max(64).next_power_of_two()
}

impl FrameAnalyser {
    pub fn new(sr: f64) -> Self {
        let n = frame_len_for(sr);
        let fft = FftPlanner::<f32>::new().plan_fft_forward(n);
        let scratch_len = fft.get_inplace_scratch_len().max(1);
        let window = (0..n)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos()))
            .collect();
        FrameAnalyser {
            fft,
            window,
            frame: vec![0.0; n],
            fill: 0,
            spec: vec![Complex32::default(); n],
            scratch: vec![Complex32::default(); scratch_len],
            mag: vec![0.0; n / 2],
            bin_hz: (sr / n as f64) as f32,
            features: FrameFeatures::silent(),
        }
    }

    pub fn frame_len(&self) -> usize {
        self.frame.len()
    }

    pub fn reset(&mut self) {
        self.frame.fill(0.0);
        self.fill = 0;
        self.features = FrameFeatures::silent();
    }

    /// Push one sample. Returns the features when this sample completes a frame.
    #[inline]
    pub fn push(&mut self, x: f32) -> Option<&FrameFeatures> {
        self.frame[self.fill] = if x.is_finite() { x } else { 0.0 };
        self.fill += 1;
        if self.fill < self.frame.len() {
            return None;
        }
        self.fill = 0;
        self.analyse();
        Some(&self.features)
    }

    fn analyse(&mut self) {
        let n = self.frame.len();
        for ((bin, &x), &w) in self.spec.iter_mut().zip(self.frame.iter()).zip(self.window.iter()) {
            *bin = Complex32::new(x * w, 0.0);
        }
        self.fft.process_with_scratch(&mut self.spec[..], &mut self.scratch[..]);
        let norm = 4.0 / n as f32;
        for (m, bin) in self.mag.iter_mut().zip(self.spec.iter()) {
            *m = bin.norm() * norm;
        }

        let sum_sq: f64 = self.frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let level_db = to_db_amp((sum_sq / n as f64).sqrt() as f32);

        let voice_db = self.band_db(300.0, 3_400.0);
        let low_noise = self.band_power(40.0, 180.0);
        let high_noise = self.band_power(6_000.0, 12_000.0);
        let noise_db = to_db_pow(0.5 * (low_noise + high_noise));

        let half = self.mag.len();
        let split = (half as f32 * 0.4) as usize;
        let low_avg = mean_pow(&self.mag[..split]);
        let high_avg = mean_pow(&self.mag[split..]);
        let high_bias_db = to_db_pow(high_avg) - to_db_pow(low_avg);

        let mut bands_db = [FLOOR_DB; NUM_FP_BANDS];
        for (dst, &(lo, hi)) in bands_db.iter_mut().zip(FP_BANDS.iter()) {
            *dst = self.band_db(lo, hi);
        }

        self.features = FrameFeatures {
            level_db,
            voice_db,
            voice_ratio_db: voice_db - noise_db,
            voice_share: self.power_share(300.0, 3_400.0, 40.0, 12_000.0),
            high_only_noise: high_bias_db > 5.0,
            flatness: self.flatness(200.0, 5_000.0),
            zcr: zero_crossing_rate(&self.frame),
            formant_score: self.formant_score(),
            bands_db,
            centroid_hz: self.centroid(40.0, 12_000.0),
        };
    }

    /// Inclusive bin range for [lo, hi] Hz, clipped to the spectrum.
    fn bins(&self, lo: f32, hi: f32) -> (usize, usize) {
        let last = self.mag.len() - 1;
        let s = ((lo / self.bin_hz).floor() as usize).min(last);
        let e = ((hi / self.bin_hz).ceil() as usize).min(last);
        (s, e.max(s))
    }

    fn band_power(&self, lo: f32, hi: f32) -> f32 {
        let (s, e) = self.bins(lo, hi);
        mean_pow(&self.mag[s..=e])
    }

    /// Summed power in [lo, hi] over summed power in [all_lo, all_hi].
    fn power_share(&self, lo: f32, hi: f32, all_lo: f32, all_hi: f32) -> f32 {
        let sum = |(s, e): (usize, usize)| self.mag[s..=e].iter().map(|m| m * m).sum::<f32>();
        let total = sum(self.bins(all_lo, all_hi));
        if total > 1e-12 {
            (sum(self.bins(lo, hi)) / total).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn band_db(&self, lo: f32, hi: f32) -> f32 {
        to_db_pow(self.band_power(lo, hi))
    }

    fn flatness(&self, lo: f32, hi: f32) -> f32 {
        let (s, e) = self.bins(lo, hi);
        let slice = &self.mag[s..=e];
        let floor = 1e-7;
        let mut log_sum = 0.0f64;
        let mut lin_sum = 0.0f64;
        for &m in slice {
            let v = m.max(floor) as f64;
            log_sum += v.ln();
            lin_sum += v;
        }
        let count = slice.len() as f64;
        if lin_sum <= floor as f64 * count {
            return 1.0; // silence counts as flat (noise-like)
        }
        ((log_sum / count).exp() / (lin_sum / count)) as f32
    }

    /// Two or three of F1/F2/F3 stand at least `FORMANT_PEAK_DB` above the
    /// valleys and the weakest is within `FORMANT_SPREAD_DB` of the strongest
    /// (speech spreads across all three). Measured in dB, like the browser's
    /// byte spectrum the thresholds were tuned on.
    fn formant_score(&self) -> f32 {
        let f = [
            self.band_db(300.0, 900.0),
            self.band_db(900.0, 2_500.0),
            self.band_db(2_500.0, 3_500.0),
        ];
        let valley = 0.5 * (self.band_db(180.0, 300.0) + self.band_db(3_500.0, 5_000.0));
        let peaks = f.iter().filter(|&&b| b - valley > FORMANT_PEAK_DB).count();
        let max_f = f[0].max(f[1]).max(f[2]);
        let min_f = f[0].min(f[1]).min(f[2]);
        let spread_db = max_f - min_f;
        if peaks >= 2 && spread_db < FORMANT_SPREAD_DB {
            peaks as f32 + 1.0 - spread_db / FORMANT_SPREAD_DB
        } else {
            0.0
        }
    }

    fn centroid(&self, lo: f32, hi: f32) -> f32 {
        let (s, e) = self.bins(lo, hi);
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        for i in s..=e {
            num += self.mag[i] * i as f32 * self.bin_hz;
            den += self.mag[i];
        }
        if den > 0.0 {
            num / den
        } else {
            0.0
        }
    }
}

fn mean_pow(mag: &[f32]) -> f32 {
    if mag.is_empty() {
        return 0.0;
    }
    mag.iter().map(|m| m * m).sum::<f32>() / mag.len() as f32
}

fn zero_crossing_rate(frame: &[f32]) -> f32 {
    let crossings = frame
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    crossings as f32 / frame.len() as f32
}

#[inline]
fn to_db_amp(a: f32) -> f32 {
    if a > 1e-6 {
        20.0 * a.log10()
    } else {
        FLOOR_DB
    }
}

#[inline]
fn to_db_pow(p: f32) -> f32 {
    if p > 1e-12 {
        10.0 * p.log10()
    } else {
        FLOOR_DB
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::{sine, white_noise};

    fn last_features(a: &mut FrameAnalyser, input: &[f32]) -> FrameFeatures {
        let mut last = FrameFeatures::silent();
        for &x in input {
            if let Some(f) = a.push(x) {
                last = *f;
            }
        }
        last
    }

    #[test]
    fn frame_length_follows_sample_rate() {
        assert_eq!(frame_len_for(48_000.0), 512);
        assert_eq!(frame_len_for(44_100.0), 512);
        assert_eq!(frame_len_for(96_000.0), 1024);
    }

    #[test]
    fn sine_reads_its_level_and_is_not_flat() {
        let mut a = FrameAnalyser::new(48_000.0);
        let f = last_features(&mut a, &sine(4_096, 1_000.0, 48_000.0, 0.5));
        assert!((f.level_db - (-9.03)).abs() < 0.5, "level {}", f.level_db);
        assert!(f.flatness < 0.3, "flatness {}", f.flatness);
        assert!((f.centroid_hz - 1_000.0).abs() < 150.0, "centroid {}", f.centroid_hz);
        assert!(f.voice_ratio_db > 20.0, "voice ratio {}", f.voice_ratio_db);
    }

    #[test]
    fn white_noise_is_flat_and_crosses_often() {
        let mut a = FrameAnalyser::new(48_000.0);
        let f = last_features(&mut a, &white_noise(4_096, 3, 0.3));
        assert!(f.flatness > 0.75, "flatness {}", f.flatness);
        assert!(f.zcr > 0.35, "zcr {}", f.zcr);
        assert!(f.voice_ratio_db.abs() < 3.0, "voice ratio {}", f.voice_ratio_db);
    }

    #[test]
    fn silence_and_nan_are_safe() {
        let mut a = FrameAnalyser::new(48_000.0);
        let f = last_features(&mut a, &[0.0; 1_024]);
        assert_eq!(f.level_db, FLOOR_DB);
        let f = last_features(&mut a, &[f32::NAN; 1_024]);
        assert!(f.level_db.is_finite() && f.flatness.is_finite() && f.centroid_hz.is_finite());
    }
}
