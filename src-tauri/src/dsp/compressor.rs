/// Soft-knee feed-forward compressor + true-peak brick-wall limiter.
pub struct Compressor {
    sr: f64,
    threshold: f32,   // dBFS
    ratio: f32,
    knee: f32,        // knee width in dB (soft knee)
    attack_coef: f32,
    release_coef: f32,
    // State
    env_db: f32,      // detected level in dB
}

impl Compressor {
    pub fn new(sr: f64) -> Self {
        Compressor {
            sr,
            threshold: -18.0,
            ratio: 3.0,
            knee: 6.0,
            attack_coef: coef(5.0, sr),
            release_coef: coef(80.0, sr),
            env_db: -96.0,
        }
    }

    pub fn set_threshold(&mut self, db: f32) { self.threshold = db; }
    pub fn set_ratio(&mut self, r: f32) { self.ratio = r.max(1.0); }
    pub fn set_attack_ms(&mut self, ms: f64) { self.attack_coef = coef(ms, self.sr); }
    pub fn set_release_ms(&mut self, ms: f64) { self.release_coef = coef(ms, self.sr); }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let x = *s;
            let x_db = lin_to_db(x.abs().max(1e-9));

            // Envelope follower in dB domain
            let coef = if x_db > self.env_db { self.attack_coef } else { self.release_coef };
            self.env_db += coef * (x_db - self.env_db);

            // Soft-knee gain computation
            let gr = self.gain_reduction(self.env_db);
            *s = x * db_to_lin(gr);
        }
    }

    fn gain_reduction(&self, x_db: f32) -> f32 {
        let t = self.threshold;
        let k = self.knee;
        let r = self.ratio;
        if 2.0 * (x_db - t) < -k {
            0.0 // below knee: no reduction
        } else if 2.0 * (x_db - t).abs() <= k {
            // in the knee
            (1.0/r - 1.0) * (x_db - t + k/2.0).powi(2) / (2.0 * k)
        } else {
            // above threshold
            (t + (x_db - t) / r) - x_db
        }
    }
}

/// True peak lookahead limiter
pub struct Limiter {
    threshold: f32,
    lookahead: usize,
    buf: Vec<f32>,
    pos: usize,
    env: f32,
    release_coef: f32,
}

impl Limiter {
    pub fn new(sr: f64) -> Self {
        let la = (sr * 0.005) as usize; // 5ms lookahead
        Limiter {
            threshold: db_to_lin(-1.0),
            lookahead: la,
            buf: vec![0.0; la + 1],
            pos: 0,
            env: 0.0,
            release_coef: coef(100.0, sr),
        }
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        let la = self.lookahead;
        for s in buf.iter_mut() {
            // Write to delay buffer
            self.buf[self.pos] = *s;

            // Look ahead: find peak in next `la` samples
            let peak = (0..la)
                .map(|i| self.buf[(self.pos + i) % self.buf.len()].abs())
                .fold(0.0f32, f32::max);

            // Gain reduction needed?
            let needed = if peak > self.threshold {
                self.threshold / peak
            } else {
                1.0
            };

            // Attack instant, release smooth
            self.env = if needed < self.env {
                needed // instant attack
            } else {
                self.env + self.release_coef * (needed - self.env)
            };

            let read_pos = (self.pos + 1) % self.buf.len();
            *s = self.buf[read_pos] * self.env;

            self.pos = (self.pos + 1) % self.buf.len();
        }
    }
}

#[inline] fn db_to_lin(db: f32) -> f32 { 10f32.powf(db / 20.0) }
#[inline] fn lin_to_db(lin: f32) -> f32 { 20.0 * lin.log10() }
#[inline] fn coef(ms: f64, sr: f64) -> f32 {
    (1.0 - (-2.2 / (ms * 0.001 * sr)).exp()) as f32
}
