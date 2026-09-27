//! On/off control for a block-latency stage (spectral STFT stages, RNNoise).
//!
//! Live-sound rule: a DISABLED stage is a zero-latency passthrough — the
//! caller leaves the audio untouched and does no work. When the stage is
//! switched on, the caller resets its FIFOs (`update` returns true) and the
//! switch then:
//!   1. outputs dry (undelayed) audio for `fill` samples while the stage's
//!      FIFOs fill with valid data (wet amount 0), then
//!   2. crossfades linearly from dry to wet over `XFADE_SAMPLES`.
//! Switching off crossfades wet → dry over `XFADE_SAMPLES` and then stops the
//! stage. The output therefore jumps in time by the stage latency when the
//! operator toggles it, but never clicks.
//!
//! Per-sample state only, so the result does not depend on block size.

/// Crossfade length: 10 ms at 48 kHz (≈ 10.9 ms at 44.1 kHz).
pub const XFADE_SAMPLES: usize = 480;

pub struct StageSwitch {
    running: bool,
    target: bool,
    fill: usize,
    fill_remaining: usize,
    wet: f32,
    step: f32,
}

impl StageSwitch {
    /// `fill` = the stage's latency in samples.
    pub fn new(fill: usize) -> Self {
        StageSwitch {
            running: false,
            target: false,
            fill,
            fill_remaining: 0,
            wet: 0.0,
            step: 1.0 / XFADE_SAMPLES as f32,
        }
    }

    /// Set the wanted state at the start of a block. Returns true when the
    /// stage has just been started and the caller must reset its FIFOs /
    /// overlap-add state before processing.
    pub fn update(&mut self, want: bool) -> bool {
        self.target = want;
        if want && !self.running {
            self.running = true;
            self.fill_remaining = self.fill;
            self.wet = 0.0;
            return true;
        }
        false
    }

    /// Stop immediately (no fade). The next `update(true)` resets the stage
    /// and fades it back in — used after the global chain bypass.
    pub fn force_restart(&mut self) {
        self.running = false;
        self.fill_remaining = 0;
        self.wet = 0.0;
    }

    /// True while the stage must be fed audio (on, filling or fading).
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Wet amount (0..1) for the next sample. Call once per sample while
    /// `is_running()`; it may stop the switch when a fade-out completes.
    #[inline]
    pub fn next_mix(&mut self) -> f32 {
        if self.fill_remaining > 0 {
            if !self.target {
                // Switched off before any wet audio was heard: stop now.
                self.fill_remaining = 0;
                self.running = false;
                return 0.0;
            }
            self.fill_remaining -= 1;
            return 0.0;
        }
        if self.target {
            self.wet = (self.wet + self.step).min(1.0);
        } else {
            self.wet = (self.wet - self.step).max(0.0);
            if self.wet <= 0.0 {
                self.running = false;
            }
        }
        self.wet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_then_fade_in_then_fade_out() {
        let mut sw = StageSwitch::new(4);
        assert!(!sw.is_running());
        assert!(sw.update(true));
        for _ in 0..4 {
            assert_eq!(sw.next_mix(), 0.0);
        }
        let mut last = 0.0;
        for _ in 0..XFADE_SAMPLES {
            last = sw.next_mix();
        }
        assert!((last - 1.0).abs() < 1e-4);
        assert!(!sw.update(true));
        assert_eq!(sw.next_mix(), 1.0);

        assert!(!sw.update(false));
        let mut n = 0;
        while sw.is_running() {
            sw.next_mix();
            n += 1;
            assert!(n <= XFADE_SAMPLES + 1);
        }
        assert!(n >= XFADE_SAMPLES - 1);
    }
}
