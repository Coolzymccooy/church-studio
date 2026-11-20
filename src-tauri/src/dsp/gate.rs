use super::hypergate::{Fingerprint, HyperDetector, HyperMode, LearnOutcome};
use super::stage_switch::StageSwitch;

/// What decides when the gate opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateMode {
    /// Plain RMS level gate (mixer strips).
    Level,
    /// Hyper gate, balanced: opens for anything above the threshold that is
    /// not clearly noise.
    Balanced,
    /// Hyper gate, speech only: opens only for speech.
    Speech,
}

/// Hyper gate envelope: fast open, quick release after its holds (the
/// worklet's per-sample coefficients, ~4 ms and ~26 ms time constants).
const HYPER_ATTACK_MS: f64 = 9.0;
const HYPER_RELEASE_MS: f64 = 57.0;

/// Lookahead noise gate.
/// Delays the signal by `lookahead_ms` so the gate opens *before* speech arrives.
/// `GateMode::Level` uses RMS detection with attack / hold / release; the
/// Hyper modes (`Gate::new_hyper`) use `HyperDetector` on the same lookahead.
pub struct Gate {
    threshold_lin: f32,   // linear amplitude threshold
    attack_coef: f32,
    release_coef: f32,
    hold_samples: usize,
    mode: GateMode,
    hyper: Option<HyperDetector>,
    hyper_attack_coef: f32,
    hyper_release_coef: f32,

    // State
    rms_buf: Vec<f32>,    // circular buffer for RMS window
    rms_pos: usize,
    rms_sum: f64,
    env: f32,             // current gain (0..1)
    hold_counter: usize,
    delay_buf: Vec<f32>,  // lookahead ring buffer
    delay_pos: usize,
}

impl Gate {
    pub fn new(sr: f64) -> Self {
        let lookahead_ms = 20.0_f64;
        let lookahead = (sr * lookahead_ms / 1000.0) as usize;
        let rms_window = (sr * 0.005) as usize; // 5ms RMS window
        Gate {
            threshold_lin: db_to_lin(-45.0),
            attack_coef: coef(2.0, sr),
            release_coef: coef(250.0, sr),
            hold_samples: (sr * 0.2) as usize, // 200ms hold
            mode: GateMode::Level,
            hyper: None,
            hyper_attack_coef: coef(HYPER_ATTACK_MS, sr),
            hyper_release_coef: coef(HYPER_RELEASE_MS, sr),
            rms_buf: vec![0.0; rms_window.max(1)],
            rms_pos: 0,
            rms_sum: 0.0,
            env: 0.0,
            hold_counter: 0,
            delay_buf: vec![0.0; lookahead + 1],
            delay_pos: 0,
        }
    }

    /// Gate that can also run the Hyper modes (allocates the detector).
    /// Starts in `GateMode::Balanced`.
    pub fn new_hyper(sr: f64) -> Self {
        let mut gate = Gate::new(sr);
        gate.hyper = Some(HyperDetector::new(sr));
        gate.set_mode(GateMode::Balanced);
        gate
    }

    /// Lookahead delay in samples.
    pub fn latency_samples(&self) -> usize {
        self.delay_buf.len() - 1
    }

    /// Switch mode. Hyper modes need `new_hyper`; without a detector the
    /// gate stays a level gate.
    pub fn set_mode(&mut self, mode: GateMode) {
        let Some(hyper) = self.hyper.as_mut() else {
            self.mode = GateMode::Level;
            return;
        };
        self.mode = mode;
        if mode == GateMode::Speech {
            hyper.mode = HyperMode::Speech;
        } else {
            hyper.mode = HyperMode::Balanced;
        }
    }

    #[cfg(test)]
    pub fn hyper(&self) -> Option<&HyperDetector> {
        self.hyper.as_ref()
    }

    pub fn set_fingerprint(&mut self, fp: Option<Fingerprint>) {
        if let Some(h) = self.hyper.as_mut() {
            h.set_fingerprint(fp);
        }
    }

    /// Start learning the mic fingerprint (Hyper gates only).
    pub fn start_learning(&mut self) {
        if let Some(h) = self.hyper.as_mut() {
            h.start_learning();
        }
    }

    pub fn take_learn_outcome(&mut self) -> Option<LearnOutcome> {
        self.hyper.as_mut().and_then(|h| h.take_learn_outcome())
    }

    pub fn is_learning(&self) -> bool {
        self.hyper.as_ref().is_some_and(|h| h.is_learning())
    }

    /// Stop learning without a result.
    pub fn cancel_learning(&mut self) {
        if let Some(h) = self.hyper.as_mut() {
            h.cancel_learning();
        }
    }

    /// Clear detector, envelope and lookahead delay line. No allocation.
    pub fn reset(&mut self) {
        for v in self.rms_buf.iter_mut() {
            *v = 0.0;
        }
        for v in self.delay_buf.iter_mut() {
            *v = 0.0;
        }
        self.rms_pos = 0;
        self.rms_sum = 0.0;
        self.env = 0.0;
        self.hold_counter = 0;
        self.delay_pos = 0;
        if let Some(h) = self.hyper.as_mut() {
            h.reset();
        }
    }

    pub fn set_threshold_db(&mut self, db: f32) {
        let db = if db.is_finite() { db.clamp(-120.0, 0.0) } else { -45.0 };
        self.threshold_lin = db_to_lin(db);
        if let Some(h) = self.hyper.as_mut() {
            h.threshold_db = db;
        }
    }

    /// Process one sample. Returns (delayed_sample × gate_gain, gate_gain).
    pub fn tick(&mut self, x: f32) -> (f32, f32) {
        let (delayed, gain) = self.tick_raw(x);
        (delayed * gain, gain)
    }

    /// Process one sample without applying the gain. Returns
    /// (delayed_sample, gate_gain). Lets a caller bypass the gate while
    /// keeping its lookahead delay (constant latency).
    pub fn tick_raw(&mut self, x: f32) -> (f32, f32) {
        self.tick_raw_vad(x, None)
    }

    /// `tick_raw` with the RNNoise voice-activity probability for the
    /// Hyper modes (None when that stage is off; ignored in `Level`).
    pub fn tick_raw_vad(&mut self, x: f32, vad: Option<f32>) -> (f32, f32) {
        self.tick_raw_keyed(x, x, vad)
    }

    /// `tick_raw_vad` where the Hyper detector listens to `key` while `x`
    /// goes through the delay line (`Level` mode ignores `key`).
    pub fn tick_raw_keyed(&mut self, x: f32, key: f32, vad: Option<f32>) -> (f32, f32) {
        if self.mode != GateMode::Level {
            if let Some(h) = self.hyper.as_mut() {
                let target = h.tick(key, vad);
                let coef = if target > self.env { self.hyper_attack_coef } else { self.hyper_release_coef };
                self.env += coef * (target - self.env);
                return (self.delay(x), self.env);
            }
        }
        let len = self.rms_buf.len();

        // Update RMS
        let old = self.rms_buf[self.rms_pos] as f64;
        self.rms_sum -= old * old;
        self.rms_buf[self.rms_pos] = x;
        self.rms_sum = (self.rms_sum + (x as f64) * (x as f64)).max(0.0);
        self.rms_pos = (self.rms_pos + 1) % len;
        let rms = (self.rms_sum / len as f64).sqrt() as f32;

        // Gate decision on *current* (lookahead) sample
        let open = rms > self.threshold_lin;

        if open {
            self.hold_counter = self.hold_samples;
        }

        let target = if open || self.hold_counter > 0 {
            if self.hold_counter > 0 { self.hold_counter -= 1; }
            1.0_f32
        } else {
            0.0_f32
        };

        let coef = if target > self.env { self.attack_coef } else { self.release_coef };
        self.env += coef * (target - self.env);

        (self.delay(x), self.env)
    }

    /// Push `x` into the lookahead line and return the delayed sample.
    #[inline]
    fn delay(&mut self, x: f32) -> f32 {
        let read_pos = (self.delay_pos + 1) % self.delay_buf.len();
        let delayed = self.delay_buf[read_pos];
        self.delay_buf[self.delay_pos] = x;
        self.delay_pos = (self.delay_pos + 1) % self.delay_buf.len();
        delayed
    }

    #[allow(dead_code)] // the chain and the mixer strips use SwitchedGate
    pub fn process_block(&mut self, buf: &mut [f32]) -> f32 {
        let mut last_gain = 0.0;
        for s in buf.iter_mut() {
            let (out, g) = self.tick(*s);
            *s = out;
            last_gain = g;
        }
        last_gain
    }
}

/// Gate with live-safe on/off (see `stage_switch.rs`): when disabled it is a
/// zero-latency passthrough; enabling resets it, plays dry while the 20 ms
/// lookahead fills, then crossfades to the gated signal; disabling
/// crossfades back to dry.
pub struct SwitchedGate {
    gate: Gate,
    switch: StageSwitch,
    pub enabled: bool,
}

impl SwitchedGate {
    pub fn new(sr: f64) -> Self {
        let gate = Gate::new(sr);
        let switch = StageSwitch::new(gate.latency_samples());
        SwitchedGate { gate, switch, enabled: true }
    }

    /// Voice-chain gate with the Hyper modes (starts balanced).
    pub fn new_hyper(sr: f64) -> Self {
        let gate = Gate::new_hyper(sr);
        let switch = StageSwitch::new(gate.latency_samples());
        SwitchedGate { gate, switch, enabled: true }
    }

    pub fn set_threshold_db(&mut self, db: f32) {
        self.gate.set_threshold_db(db);
    }

    pub fn set_mode(&mut self, mode: GateMode) {
        self.gate.set_mode(mode);
    }

    #[cfg(test)]
    pub fn gate(&self) -> &Gate {
        &self.gate
    }

    pub fn set_fingerprint(&mut self, fp: Option<Fingerprint>) {
        self.gate.set_fingerprint(fp);
    }

    /// Learn the mic over the next five seconds of gated audio. Learning
    /// advances only while the gate runs (it is fed by the gate's detector).
    pub fn start_learning(&mut self) {
        self.gate.start_learning();
    }

    pub fn take_learn_outcome(&mut self) -> Option<LearnOutcome> {
        self.gate.take_learn_outcome()
    }

    pub fn is_learning(&self) -> bool {
        self.gate.is_learning()
    }

    pub fn cancel_learning(&mut self) {
        self.gate.cancel_learning();
    }

    /// Lookahead latency when enabled, 0 when disabled.
    pub fn latency_samples(&self) -> usize {
        if self.enabled {
            self.gate.latency_samples()
        } else {
            0
        }
    }

    /// Drop state; the next enabled block refills and fades in.
    pub fn restart(&mut self) {
        self.switch.force_restart();
    }

    /// Process in place. Returns the last gate gain (1.0 when disabled).
    pub fn process_block(&mut self, buf: &mut [f32]) -> f32 {
        self.process_block_vad(buf, None)
    }

    /// `process_block` with the RNNoise VAD that applies to the whole of
    /// `buf` (see `DspChain::run_neural_and_gate` for how the chain keeps
    /// that block-size independent).
    pub fn process_block_vad(&mut self, buf: &mut [f32], vad: Option<f32>) -> f32 {
        self.process_block_keyed(buf, None, vad)
    }

    /// `process_block_vad` with a key signal for the Hyper detector: the
    /// gate decides on `key` (same length as `buf`) and applies its gain to
    /// `buf`. None keys the detector on `buf` itself.
    pub fn process_block_keyed(&mut self, buf: &mut [f32], key: Option<&[f32]>, vad: Option<f32>) -> f32 {
        if self.switch.update(self.enabled) {
            self.gate.reset();
        }
        let mut last_gain = 1.0;
        for (i, s) in buf.iter_mut().enumerate() {
            if !self.switch.is_running() {
                break; // disabled (or fade-out done): rest stays dry
            }
            let x = *s;
            let k = key.and_then(|k| k.get(i).copied()).unwrap_or(x);
            let (delayed, g) = self.gate.tick_raw_keyed(x, k, vad);
            let gated = delayed * g;
            let mix = self.switch.next_mix();
            *s = x + (gated - x) * mix;
            last_gain = g;
        }
        last_gain
    }
}

#[inline] fn db_to_lin(db: f32) -> f32 { 10f32.powf(db / 20.0) }
#[inline] fn coef(ms: f64, sr: f64) -> f32 {
    (1.0 - (-2.2 / (ms * 0.001 * sr)).exp()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::{run_blocks, sine, test_signal};

    const SR: f64 = 48_000.0;

    #[test]
    fn disabled_gate_is_exact_zero_latency_passthrough() {
        let input = test_signal(48_000, 3);
        for &block in &[64usize, 480, 4096] {
            let mut g = SwitchedGate::new(SR);
            g.enabled = false;
            assert_eq!(g.latency_samples(), 0);
            let out = run_blocks(&input, block, |b| {
                g.process_block(b);
            });
            assert_eq!(out, input);
        }
    }

    #[test]
    fn enabled_gate_is_block_size_invariant() {
        let input = test_signal(48_000, 4);
        let mut reference: Vec<Vec<f32>> = Vec::new();
        for &block in &[64usize, 256, 480, 1000, 4096] {
            let mut g = SwitchedGate::new(SR);
            g.set_threshold_db(-30.0);
            reference.push(run_blocks(&input, block, |b| {
                g.process_block(b);
            }));
        }
        for out in reference.iter().skip(1) {
            assert_eq!(out, &reference[0]);
        }
    }

    #[test]
    fn toggling_gate_does_not_click() {
        // Threshold far below the signal: the gate is fully open, so "wet" is
        // the input delayed by the lookahead and only the switching is tested.
        let input = sine(96_000, 440.0, 48_000.0, 0.5);
        let mut g = SwitchedGate::new(SR);
        g.set_threshold_db(-100.0);
        let mut out = Vec::with_capacity(input.len());
        for (i, chunk) in input.chunks(480).enumerate() {
            g.enabled = (i / 20) % 2 == 1;
            let mut block = chunk.to_vec();
            g.process_block(&mut block);
            out.extend_from_slice(&block);
        }
        let max_step = out.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(max_step < 0.04, "step {max_step}");
    }
}
