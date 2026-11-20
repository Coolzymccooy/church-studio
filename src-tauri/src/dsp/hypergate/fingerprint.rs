//! "Learn mic": a spectral fingerprint of the connected mic and voice.
//!
//! Port of the browser fingerprint (App.jsx): eight band levels plus the
//! spectral centroid, learned with Welford's running mean/variance, matched
//! by average deviation in standard deviations. Two changes make it useful
//! in a live room:
//! - Bands are a level-normalised SHAPE (each band in dB minus the mean of
//!   the eight), so moving closer to the mic or raising the trim is not a
//!   mismatch; a TV or a distant voice has a different shape.
//! - Only active, voiced frames are learned (the browser also averaged the
//!   silence between words, which made everything "match"); fricatives are
//!   skipped too, their spectrum says little about the mic.
//! - Empty bands are floored 60 dB under the frame's strongest band and the
//!   centroid is in octaves, so neither can blow up the learned spread.
//!
//! Fixed-size arrays only: learning and matching never allocate.
use super::analysis::{FrameFeatures, NUM_FP_BANDS};

/// Fingerprint vector: band shape (dB) then centroid (octaves re 1 kHz).
pub const FP_LEN: usize = NUM_FP_BANDS + 1;
const CENTROID: usize = NUM_FP_BANDS;

/// Smallest standard deviation used when matching, per element.
const MIN_STD_BAND_DB: f32 = 1.5;
const MIN_STD_CENTROID_OCT: f32 = 0.15;
/// The centroid counts double, as in the browser.
const CENTROID_WEIGHT: f32 = 2.0;
/// Bands more than this far under the frame's strongest band are clamped,
/// so an empty band (no energy at all) cannot dominate.
const BAND_RANGE_DB: f32 = 60.0;

/// Learned mean and standard deviation of the fingerprint vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fingerprint {
    pub mean: [f32; FP_LEN],
    pub std: [f32; FP_LEN],
}

/// Fingerprint vector of one frame.
pub fn vector(f: &FrameFeatures) -> [f32; FP_LEN] {
    let mut v = [0.0f32; FP_LEN];
    let strongest = f.bands_db.iter().fold(f32::MIN, |m, &b| m.max(b));
    let floor = strongest - BAND_RANGE_DB;
    let mut sum = 0.0f32;
    for (dst, &b) in v.iter_mut().zip(f.bands_db.iter()) {
        *dst = b.max(floor);
        sum += *dst;
    }
    let reference = sum / NUM_FP_BANDS as f32;
    for b in v[..NUM_FP_BANDS].iter_mut() {
        *b -= reference;
    }
    v[CENTROID] = (f.centroid_hz.max(20.0) / 1_000.0).log2();
    v
}

impl Fingerprint {
    /// Rebuild from a flat [means..., stds...] slice (the params store).
    /// None when the slice is short or holds a non-finite value.
    pub fn from_flat(values: &[f32]) -> Option<Self> {
        if values.len() < 2 * FP_LEN || values.iter().take(2 * FP_LEN).any(|v| !v.is_finite()) {
            return None;
        }
        let mut fp = Fingerprint { mean: [0.0; FP_LEN], std: [0.0; FP_LEN] };
        fp.mean.copy_from_slice(&values[..FP_LEN]);
        fp.std.copy_from_slice(&values[FP_LEN..2 * FP_LEN]);
        Some(fp)
    }

    /// Write as [means..., stds...] into `out` (at least 2 × FP_LEN long).
    pub fn to_flat(&self, out: &mut [f32]) {
        out[..FP_LEN].copy_from_slice(&self.mean);
        out[FP_LEN..2 * FP_LEN].copy_from_slice(&self.std);
    }

    /// 1 = sounds like the learned mic, 0 = clearly something else.
    /// Same mapping as the browser: average deviation ≤ 1 σ scores 1, and
    /// each further σ costs 0.3.
    pub fn match_score(&self, f: &FrameFeatures) -> f32 {
        let v = vector(f);
        let mut total = 0.0f32;
        for b in 0..NUM_FP_BANDS {
            total += (v[b] - self.mean[b]).abs() / self.std[b].max(MIN_STD_BAND_DB);
        }
        let c = (v[CENTROID] - self.mean[CENTROID]).abs()
            / self.std[CENTROID].max(MIN_STD_CENTROID_OCT);
        total += c * CENTROID_WEIGHT;
        let avg = total / (NUM_FP_BANDS as f32 + CENTROID_WEIGHT);
        (1.0 - (avg - 1.0) * 0.3).clamp(0.0, 1.0)
    }
}

/// Result of a learning window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LearnOutcome {
    Learned(Fingerprint),
    /// Too few voice frames were heard in the window.
    NotEnoughVoice,
    /// The gate was off or the chain bypassed, so nothing could be heard.
    GateOff,
}

/// Welford learner over a fixed window of samples.
pub struct Learner {
    active: bool,
    remaining: usize,
    min_frames: usize,
    count: usize,
    mean: [f64; FP_LEN],
    m2: [f64; FP_LEN],
    outcome: Option<LearnOutcome>,
}

impl Learner {
    pub fn new() -> Self {
        Learner {
            active: false,
            remaining: 0,
            min_frames: 1,
            count: 0,
            mean: [0.0; FP_LEN],
            m2: [0.0; FP_LEN],
            outcome: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Stop without an outcome.
    pub fn cancel(&mut self) {
        self.active = false;
        self.outcome = None;
    }

    /// Learn over the next `window` samples; at least `min_frames` voice
    /// frames are needed for a fingerprint.
    pub fn start(&mut self, window: usize, min_frames: usize) {
        self.active = true;
        self.remaining = window.max(1);
        self.min_frames = min_frames.max(2);
        self.count = 0;
        self.mean = [0.0; FP_LEN];
        self.m2 = [0.0; FP_LEN];
        self.outcome = None;
    }

    /// Feed one analysed frame of `frame_len` samples; `voice` says whether
    /// it counts as the mic's voice.
    pub fn frame(&mut self, f: &FrameFeatures, voice: bool, frame_len: usize) {
        if !self.active {
            return;
        }
        if voice {
            self.count += 1;
            let v = vector(f);
            let n = self.count as f64;
            for i in 0..FP_LEN {
                let x = v[i] as f64;
                let delta = x - self.mean[i];
                self.mean[i] += delta / n;
                self.m2[i] += delta * (x - self.mean[i]);
            }
        }
        self.remaining = self.remaining.saturating_sub(frame_len);
        if self.remaining == 0 {
            self.finish();
        }
    }

    fn finish(&mut self) {
        self.active = false;
        self.outcome = Some(if self.count >= self.min_frames {
            let mut fp = Fingerprint { mean: [0.0; FP_LEN], std: [0.0; FP_LEN] };
            let n = (self.count - 1) as f64;
            for i in 0..FP_LEN {
                fp.mean[i] = self.mean[i] as f32;
                fp.std[i] = (self.m2[i] / n).sqrt() as f32;
            }
            LearnOutcome::Learned(fp)
        } else {
            LearnOutcome::NotEnoughVoice
        });
    }

    /// The finished window's result, once.
    pub fn take_outcome(&mut self) -> Option<LearnOutcome> {
        self.outcome.take()
    }
}

impl Default for Learner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(bands: [f32; NUM_FP_BANDS], centroid: f32) -> FrameFeatures {
        FrameFeatures { bands_db: bands, centroid_hz: centroid, ..FrameFeatures::silent() }
    }

    const VOICE: [f32; 8] = [-40.0, -30.0, -24.0, -28.0, -34.0, -42.0, -50.0, -60.0];

    #[test]
    fn learns_the_mean_shape_and_matches_it_at_any_level() {
        let mut l = Learner::new();
        l.start(100 * 512, 10);
        for i in 0..100 {
            let wobble = if i % 2 == 0 { 1.0 } else { -1.0 };
            let mut b = VOICE;
            b[2] += wobble;
            l.frame(&frame(b, 900.0 * (1.0 + 0.05 * wobble)), true, 512);
        }
        let Some(LearnOutcome::Learned(fp)) = l.take_outcome() else { panic!("not learned") };
        assert!(l.take_outcome().is_none(), "outcome is taken once");
        // The band wobbles ±1 dB; normalising by the mean of the eight leaves ±0.875.
        assert!((fp.std[2] - 0.875).abs() < 0.05, "std {}", fp.std[2]);

        assert!(fp.match_score(&frame(VOICE, 900.0)) > 0.95);
        // Same shape 20 dB louder: still the same mic.
        let louder = VOICE.map(|b| b + 20.0);
        assert!(fp.match_score(&frame(louder, 900.0)) > 0.95);
        // Flat, bright spectrum (a TV across the room): no match.
        let tv = [-50.0; 8];
        assert!(fp.match_score(&frame(tv, 3_500.0)) < 0.3);
    }

    #[test]
    fn too_little_voice_is_reported() {
        let mut l = Learner::new();
        l.start(10 * 512, 5);
        for i in 0..10 {
            l.frame(&frame(VOICE, 900.0), i < 3, 512);
        }
        assert!(!l.is_active());
        assert_eq!(l.take_outcome(), Some(LearnOutcome::NotEnoughVoice));
    }

    #[test]
    fn flat_round_trip_and_rejects_nan() {
        let fp = Fingerprint { mean: [1.0; FP_LEN], std: [2.0; FP_LEN] };
        let mut flat = [0.0f32; 2 * FP_LEN];
        fp.to_flat(&mut flat);
        assert_eq!(Fingerprint::from_flat(&flat), Some(fp));
        flat[3] = f32::NAN;
        assert_eq!(Fingerprint::from_flat(&flat), None);
        assert_eq!(Fingerprint::from_flat(&flat[..4]), None);
    }
}
