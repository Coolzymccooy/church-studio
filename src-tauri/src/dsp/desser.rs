/// Dynamic de-esser.
/// Bandpass sidechain (default: 7500Hz center, 3kHz BW) detects sibilance.
/// Full-signal gain reduction applied only during sibilant events.
use super::biquad::Biquad;

pub struct DeEsser {
    sidechain_bp: Biquad,   // bandpass for detection
    threshold_lin: f32,
    ratio: f32,
    attack_coef: f32,
    release_coef: f32,
    env: f32,
    pub enabled: bool,
    pub gr_db: f32,         // last gain reduction in dB (for metering)
}

impl DeEsser {
    pub fn new(sr: f64) -> Self {
        DeEsser {
            sidechain_bp: Biquad::bpf(7500.0, 3000.0, sr),
            threshold_lin: db_to_lin(-24.0),
            ratio: 8.0,
            attack_coef: coef(1.0, sr),
            release_coef: coef(40.0, sr),
            env: 0.0,
            enabled: true,
            gr_db: 0.0,
        }
    }

    pub fn set_threshold_db(&mut self, db: f32) { self.threshold_lin = db_to_lin(db); }

    /// Process block, returns peak GR (dB, negative = gain reduction)
    pub fn process_block(&mut self, buf: &mut [f32]) -> f32 {
        if !self.enabled { self.gr_db = 0.0; return 0.0; }

        let mut max_gr = 0.0f32;
        for s in buf.iter_mut() {
            // Sidechain detection
            let sc = self.sidechain_bp.tick(*s).abs();

            // Peak envelope follower
            let coef = if sc > self.env { self.attack_coef } else { self.release_coef };
            self.env += coef * (sc - self.env);

            // Gain reduction
            let gr = if self.env > self.threshold_lin {
                let excess = lin_to_db(self.env) - lin_to_db(self.threshold_lin);
                -(excess * (1.0 - 1.0 / self.ratio))
            } else {
                0.0
            };

            *s *= db_to_lin(gr);
            if gr < max_gr { max_gr = gr; }
        }

        self.gr_db = max_gr;
        max_gr
    }
}

#[inline] fn db_to_lin(db: f32) -> f32 { 10f32.powf(db / 20.0) }
#[inline] fn lin_to_db(lin: f32) -> f32 { 20.0 * lin.max(1e-9).log10() }
#[inline] fn coef(ms: f64, sr: f64) -> f32 {
    (1.0 - (-2.2 / (ms * 0.001 * sr)).exp()) as f32
}
