//! Engine-side read of an NDI® input ring, with clock-drift correction.
//!
//! The NDI sender and the sound card run on different clocks, so the ring
//! slowly fills or empties. `DriftController` watches a smoothed fill level
//! and nudges the read ratio by at most ±0.1 % to hold the ring near its
//! target (design decision 5); `DriftReader` reads at that ratio with linear
//! interpolation, which is inaudible at such small ratios.
//!
//! - Underrun: the block is silence and the reader re-buffers to the
//!   target before it plays again. It never waits.
//! - Large overrun (a stall, not drift): the ring is trimmed back to the
//!   target and the skipped samples are counted.
//!
//! Real-time safe: `read` runs on the audio callback and only does atomic
//! ring operations and arithmetic (no locks, allocation or logging).
use super::convert::STEREO;
use ringbuf::traits::{Consumer, Observer};

/// Fill the reader holds the ring at.
pub const TARGET_FILL_MS: usize = 40;
/// Above target + this, the ring is trimmed back to the target.
pub const OVERRUN_MARGIN_MS: usize = 200;
/// Largest ratio nudge: ±0.1 %.
pub const MAX_NUDGE: f64 = 0.001;
/// Per-block smoothing of the fill level (one-pole).
const FILL_SMOOTHING: f64 = 0.05;

/// Frames in `ms` milliseconds at `sample_rate` (at least one).
pub fn frames_for_ms(sample_rate: u32, ms: usize) -> usize {
    (sample_rate as usize * ms / 1000).max(1)
}

/// Read ratio for a (smoothed) fill level: 1.0 at the target, up to
/// `1 + MAX_NUDGE` (read faster) when fuller and down to `1 − MAX_NUDGE`
/// when emptier. The full nudge is reached 50 % away from the target.
pub fn ratio_for_fill(fill_frames: f64, target_frames: f64) -> f64 {
    if target_frames <= 0.0 || !fill_frames.is_finite() {
        return 1.0;
    }
    let error = (fill_frames - target_frames) / (target_frames * 0.5);
    1.0 + (error * MAX_NUDGE).clamp(-MAX_NUDGE, MAX_NUDGE)
}

/// Smooths the fill level and turns it into a read ratio.
#[derive(Debug, Clone)]
pub struct DriftController {
    target: f64,
    average: f64,
    primed: bool,
}

impl DriftController {
    pub fn new(target_frames: usize) -> Self {
        DriftController {
            target: target_frames as f64,
            average: target_frames as f64,
            primed: false,
        }
    }

    /// Start smoothing again from `fill_frames`.
    pub fn reset(&mut self, fill_frames: usize) {
        self.average = fill_frames as f64;
        self.primed = true;
    }

    /// Feed this block's fill level; returns the read ratio to use.
    pub fn update(&mut self, fill_frames: usize) -> f64 {
        let fill = fill_frames as f64;
        if self.primed {
            self.average += (fill - self.average) * FILL_SMOOTHING;
        } else {
            self.reset(fill_frames);
        }
        ratio_for_fill(self.average, self.target)
    }
}

/// What one `read` did, for the input's status counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadReport {
    /// Stereo frames in the ring when the block started (after any trim).
    pub fill_frames: usize,
    /// Samples skipped to recover from an overrun.
    pub dropped_samples: usize,
    /// The ring ran dry during playback (the block is silent).
    pub underrun: bool,
}

pub struct DriftReader {
    controller: DriftController,
    target: usize,
    high: usize,
    buffering: bool,
    current: [f32; STEREO],
    next: [f32; STEREO],
    /// Position between `current` (0) and `next` (1).
    frac: f64,
}

fn pop_frame<C: Consumer<Item = f32>>(ring: &mut C) -> Option<[f32; STEREO]> {
    if ring.occupied_len() < STEREO {
        return None;
    }
    let mut frame = [0.0f32; STEREO];
    if ring.pop_slice(&mut frame) == STEREO {
        Some(frame)
    } else {
        None
    }
}

impl DriftReader {
    pub fn new(sample_rate: u32) -> Self {
        let target = frames_for_ms(sample_rate, TARGET_FILL_MS);
        Self::with_levels(target, target + frames_for_ms(sample_rate, OVERRUN_MARGIN_MS))
    }

    /// `target` and `high` in stereo frames (`high` > `target`).
    pub fn with_levels(target: usize, high: usize) -> Self {
        let target = target.max(2);
        DriftReader {
            controller: DriftController::new(target),
            target,
            high: high.max(target + 1),
            buffering: true,
            current: [0.0; STEREO],
            next: [0.0; STEREO],
            frac: 0.0,
        }
    }

    #[allow(dead_code)] // status helper, used by the tests
    pub fn is_buffering(&self) -> bool {
        self.buffering
    }

    /// Fill `out` (mono, one value per frame: (L + R) / 2) from `ring`.
    pub fn read<C: Consumer<Item = f32>>(&mut self, ring: &mut C, out: &mut [f32]) -> ReadReport {
        let mut report = ReadReport::default();
        let mut fill = ring.occupied_len() / STEREO;
        if fill > self.high {
            let skipped = ring.skip((fill - self.target) * STEREO);
            report.dropped_samples = skipped;
            fill -= skipped / STEREO;
            self.controller.reset(fill);
        }
        report.fill_frames = fill;

        if self.buffering {
            if fill < self.target || !self.prime(ring) {
                out.fill(0.0);
                return report;
            }
            self.controller.reset(fill);
        }

        let ratio = self.controller.update(fill);
        // Frames this block pops at most (one per whole step of `frac`).
        let needed = (self.frac + out.len() as f64 * ratio).ceil() as usize;
        if ring.occupied_len() / STEREO < needed {
            self.underrun(out, &mut report);
            return report;
        }
        for index in 0..out.len() {
            let t = self.frac as f32;
            let left = self.current[0] + (self.next[0] - self.current[0]) * t;
            let right = self.current[1] + (self.next[1] - self.current[1]) * t;
            out[index] = (left + right) * 0.5;
            self.frac += ratio;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                self.current = self.next;
                match pop_frame(ring) {
                    Some(frame) => self.next = frame,
                    None => {
                        self.underrun(&mut out[index + 1..], &mut report);
                        return report;
                    }
                }
            }
        }
        report
    }

    /// Take the first two frames after buffering.
    fn prime<C: Consumer<Item = f32>>(&mut self, ring: &mut C) -> bool {
        let (Some(current), Some(next)) = (pop_frame(ring), pop_frame(ring)) else {
            return false;
        };
        self.current = current;
        self.next = next;
        self.frac = 0.0;
        self.buffering = false;
        true
    }

    fn underrun(&mut self, rest: &mut [f32], report: &mut ReadReport) {
        rest.fill(0.0);
        self.buffering = true;
        report.underrun = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Producer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn ratio_is_one_at_target_and_bounded() {
        assert_eq!(ratio_for_fill(100.0, 100.0), 1.0);
        let fuller = ratio_for_fill(110.0, 100.0);
        assert!(fuller > 1.0 && fuller < 1.0 + MAX_NUDGE);
        let emptier = ratio_for_fill(90.0, 100.0);
        assert!(emptier < 1.0 && emptier > 1.0 - MAX_NUDGE);
        assert_eq!(ratio_for_fill(1_000.0, 100.0), 1.0 + MAX_NUDGE);
        assert_eq!(ratio_for_fill(0.0, 100.0), 1.0 - MAX_NUDGE);
        assert_eq!(ratio_for_fill(f64::NAN, 100.0), 1.0);
        assert_eq!(ratio_for_fill(5.0, 0.0), 1.0);
    }

    #[test]
    fn controller_smooths_a_single_spike() {
        let mut c = DriftController::new(100);
        assert_eq!(c.update(100), 1.0);
        let after_spike = c.update(1_000);
        assert!(after_spike > 1.0 && after_spike < 1.0 + MAX_NUDGE);
    }

    fn push_constant<P: Producer<Item = f32>>(p: &mut P, frames: usize, l: f32, r: f32) {
        for _ in 0..frames {
            p.try_push(l).unwrap();
            p.try_push(r).unwrap();
        }
    }

    #[test]
    fn buffers_until_target_then_plays_downmixed() {
        let (mut p, mut c) = HeapRb::<f32>::new(10_000).split();
        let mut reader = DriftReader::with_levels(100, 1_000);
        let mut out = vec![1.0f32; 32];
        push_constant(&mut p, 50, 0.2, 0.6);
        let report = reader.read(&mut c, &mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert!(!report.underrun && reader.is_buffering());
        push_constant(&mut p, 100, 0.2, 0.6);
        reader.read(&mut c, &mut out);
        assert!(!reader.is_buffering());
        assert!(out.iter().all(|&s| (s - 0.4).abs() < 1e-6), "{out:?}");
    }

    #[test]
    fn underrun_gives_silence_and_rebuffers() {
        let (mut p, mut c) = HeapRb::<f32>::new(10_000).split();
        let mut reader = DriftReader::with_levels(100, 1_000);
        push_constant(&mut p, 120, 0.5, 0.5);
        let mut out = vec![0.0f32; 64];
        reader.read(&mut c, &mut out);
        let report = reader.read(&mut c, &mut out);
        assert!(report.underrun);
        assert!(out.iter().all(|&s| s == 0.0));
        assert!(reader.is_buffering());
    }

    #[test]
    fn large_overrun_is_trimmed_and_counted() {
        let (mut p, mut c) = HeapRb::<f32>::new(10_000).split();
        let mut reader = DriftReader::with_levels(100, 1_000);
        // Alternate frames so a misaligned skip would swap L and R.
        for n in 0..3_000 {
            let v = if n % 2 == 0 { 1.0 } else { 0.0 };
            p.try_push(v).unwrap();
            p.try_push(-v).unwrap();
        }
        let mut out = vec![0.0f32; 16];
        let report = reader.read(&mut c, &mut out);
        assert_eq!(report.dropped_samples, (3_000 - 100) * STEREO);
        assert_eq!(report.fill_frames, 100);
        // L + R = 0 in every frame, so the downmix stays silent if aligned.
        assert!(out.iter().all(|s| s.abs() < 1e-6), "{out:?}");
    }

    /// Feed the ring at `source_ppm` away from the engine clock for a
    /// simulated minute: no underrun after start-up and the fill stays
    /// near the target.
    fn simulate_drift(source_ppm: f64) {
        let rate = 48_000u32;
        let (mut p, mut c) = HeapRb::<f32>::new(rate as usize * 2).split();
        let mut reader = DriftReader::new(rate);
        let target = frames_for_ms(rate, TARGET_FILL_MS) as f64;
        let block = 256usize;
        let per_block = block as f64 * (1.0 + source_ppm / 1e6);
        let mut owed = 0.0f64;
        let mut out = vec![0.0f32; block];
        let mut underruns = 0;
        let mut last_fill = 0usize;
        for step in 0..(60 * rate as usize / block) {
            owed += per_block;
            let frames = owed.floor() as usize;
            owed -= frames as f64;
            push_constant(&mut p, frames, 0.1, 0.1);
            let report = reader.read(&mut c, &mut out);
            if step > 100 && report.underrun {
                underruns += 1;
            }
            last_fill = report.fill_frames;
        }
        assert_eq!(underruns, 0, "ppm {source_ppm}");
        let fill = last_fill as f64;
        assert!(fill > target * 0.5 && fill < target * 1.5, "ppm {source_ppm}: fill {fill}");
    }

    #[test]
    fn drift_is_absorbed_both_ways() {
        simulate_drift(500.0);
        simulate_drift(-500.0);
        simulate_drift(0.0);
    }
}
