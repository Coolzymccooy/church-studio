use super::stage_switch::StageSwitch;

/// Lookahead noise gate.
/// Delays the signal by `lookahead_ms` so the gate opens *before* speech arrives.
/// Uses RMS detection with attack / hold / release envelope.
pub struct Gate {
    threshold_lin: f32,   // linear amplitude threshold
    attack_coef: f32,
    release_coef: f32,
    hold_samples: usize,

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
            rms_buf: vec![0.0; rms_window.max(1)],
            rms_pos: 0,
            rms_sum: 0.0,
            env: 0.0,
            hold_counter: 0,
            delay_buf: vec![0.0; lookahead + 1],
            delay_pos: 0,
        }
    }

    /// Lookahead delay in samples.
    pub fn latency_samples(&self) -> usize {
        self.delay_buf.len() - 1
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
    }

    pub fn set_threshold_db(&mut self, db: f32) {
        self.threshold_lin = db_to_lin(db);
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

        // Read delayed sample
        let read_pos = (self.delay_pos + 1) % self.delay_buf.len();
        let delayed = self.delay_buf[read_pos];
        self.delay_buf[self.delay_pos] = x;
        self.delay_pos = (self.delay_pos + 1) % self.delay_buf.len();

        (delayed, self.env)
    }

    #[allow(dead_code)] // the chain uses SwitchedGate, the mixer tick_raw
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

    pub fn set_threshold_db(&mut self, db: f32) {
        self.gate.set_threshold_db(db);
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
        if self.switch.update(self.enabled) {
            self.gate.reset();
        }
        let mut last_gain = 1.0;
        for s in buf.iter_mut() {
            if !self.switch.is_running() {
                break; // disabled (or fade-out done): rest stays dry
            }
            let x = *s;
            let (gated, g) = self.gate.tick(x);
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
