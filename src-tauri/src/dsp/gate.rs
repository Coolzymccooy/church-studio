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

    pub fn set_threshold_db(&mut self, db: f32) {
        self.threshold_lin = db_to_lin(db);
    }

    /// Process one sample. Returns (delayed_sample × gate_gain, gate_gain).
    pub fn tick(&mut self, x: f32) -> (f32, f32) {
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

        (delayed * self.env, self.env)
    }

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

#[inline] fn db_to_lin(db: f32) -> f32 { 10f32.powf(db / 20.0) }
#[inline] fn coef(ms: f64, sr: f64) -> f32 {
    (1.0 - (-2.2 / (ms * 0.001 * sr)).exp()) as f32
}
