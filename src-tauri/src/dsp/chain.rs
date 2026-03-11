/// Complete DSP processing pipeline.
/// Chain order:
///   input → HPF(80Hz) → Gate → Noise Reduction → EQ Warmth → EQ Clarity
///         → De-esser → Compressor → Auto Gain → Limiter → LUFS meter → output
use super::{
    biquad::Biquad,
    compressor::{Compressor, Limiter},
    desser::DeEsser,
    dereverb::SpectralDereverb,
    gate::Gate,
    lufs::{LufsMeter, LufsReadings},
    noise::SpectralDenoiser,
    DspParams,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct DspChain {
    sr: f64,
    input_gain_db: f32,
    gate_enabled: bool,
    compressor_enabled: bool,

    // Pre-processing
    hpf: Biquad,
    gate: Gate,
    noise: SpectralDenoiser,

    // EQ
    eq_warmth: Biquad,    // low shelf ~200Hz
    eq_clarity: Biquad,   // peaking ~3kHz

    // Dynamics
    desser: DeEsser,
    compressor: Compressor,
    limiter: Limiter,

    // Metering
    lufs: LufsMeter,

    // Post-processing
    dereverb: SpectralDereverb,

    // Auto gain state
    auto_gain: f32,

    // Output meter
    out_peak_env: f32,
    out_peak_coef: f32,

    // Last readings
    pub last_lufs: LufsReadings,
    pub last_gate_gain: f32,
    pub last_deess_gr: f32,
    pub last_input_db: f32,
    pub last_output_db: f32,
    pub last_auto_gain_db: f32,
}

impl DspChain {
    pub fn new(sr: f64) -> Self {
        DspChain {
            sr,
            input_gain_db: 0.0,
            gate_enabled: true,
            compressor_enabled: true,
            hpf: Biquad::hpf(80.0, sr),
            gate: Gate::new(sr),
            noise: SpectralDenoiser::new(),
            eq_warmth: Biquad::low_shelf(200.0, 3.0, sr),
            eq_clarity: Biquad::peaking(3000.0, 0.8, 2.0, sr),
            desser: DeEsser::new(sr),
            compressor: Compressor::new(sr),
            limiter: Limiter::new(sr),
            lufs: LufsMeter::new(sr),
            dereverb: SpectralDereverb::new(),
            auto_gain: 1.0,
            out_peak_env: 0.0,
            out_peak_coef: coef(300.0, sr),
            last_lufs: LufsReadings { momentary: -70.0, short_term: -70.0, integrated: -70.0 },
            last_gate_gain: 0.0,
            last_deess_gr: 0.0,
            last_input_db: -96.0,
            last_output_db: -96.0,
            last_auto_gain_db: 0.0,
        }
    }

    /// Sync all parameters from the shared atomic state.
    pub fn sync_params(&mut self, p: &Arc<DspParams>) {
        use Ordering::Relaxed;

        self.input_gain_db = p.gain_db.load(Relaxed);

        // Gate
        self.gate_enabled = p.gate_enabled.load(Relaxed);
        self.gate.set_threshold_db(p.gate_threshold_db.load(Relaxed));

        // Compressor
        self.compressor_enabled = p.comp_enabled.load(Relaxed);
        self.compressor.set_threshold(p.comp_threshold_db.load(Relaxed));
        self.compressor.set_ratio(p.comp_ratio.load(Relaxed));

        // Noise reduction
        self.noise.alpha = p.noise_alpha.load(Relaxed);
        self.noise.enabled = p.noise_enabled.load(Relaxed);

        // De-esser
        self.desser.enabled = p.deess_enabled.load(Relaxed);
        self.desser.set_threshold_db(p.deess_threshold_db.load(Relaxed));

        // Dereverb
        self.dereverb.enabled = p.dereverb_enabled.load(Relaxed);
        self.dereverb.strength = p.dereverb_strength.load(Relaxed);
    }

    /// Process one block of mono samples in-place.
    /// Returns the latest LUFS readings if a new 100ms block completed.
    pub fn process_block(&mut self, buf: &mut Vec<f32>, bypass: bool) {
        if bypass || buf.is_empty() { return; }

        let gain_lin = db_to_lin(self.input_gain_db);

        // ── Input level ──────────────────────────────────────────────────────
        let in_rms = rms(buf);
        self.last_input_db = lin_to_db(in_rms.max(1e-9));

        // 1. Apply input gain
        for s in buf.iter_mut() { *s *= gain_lin; }

        // 2. HPF
        for s in buf.iter_mut() { *s = self.hpf.tick(*s); }

        // 3. Gate
        let gate_gain = if self.gate_enabled {
            self.gate.process_block(buf)
        } else { 1.0 };
        self.last_gate_gain = gate_gain;

        // 4. Noise reduction (block-level OLA)
        self.noise.process_block(buf);

        // 5. Dereverb (block-level OLA)
        self.dereverb.process_block(buf);

        // 6. EQ
        for s in buf.iter_mut() {
            *s = self.eq_warmth.tick(*s);
            *s = self.eq_clarity.tick(*s);
        }

        // 7. De-esser
        let gr = self.desser.process_block(buf);
        self.last_deess_gr = gr;

        // 8. Compressor
        if self.compressor_enabled {
            self.compressor.process_block(buf);
        }

        // 9. Auto gain rider (keep RMS ~= -18 dBFS)
        self.update_auto_gain(rms(buf));
        let ag = self.auto_gain;
        for s in buf.iter_mut() { *s *= ag; }
        self.last_auto_gain_db = lin_to_db(ag);

        // 10. Limiter
        self.limiter.process_block(buf);

        // 11. LUFS metering
        for &s in buf.iter() {
            if let Some(readings) = self.lufs.tick(s) {
                self.last_lufs = readings;
            }
        }

        // ── Output level ─────────────────────────────────────────────────────
        let out_peak = buf.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        self.out_peak_env += self.out_peak_coef * (out_peak - self.out_peak_env);
        self.last_output_db = lin_to_db(self.out_peak_env.max(1e-9));
    }

    pub fn capture_noise_profile(&mut self, samples: &[f32]) {
        self.noise.capture_profile(samples);
    }

    fn update_auto_gain(&mut self, rms_in: f32) {
        if rms_in < 1e-6 { return; }
        let target_rms = db_to_lin(-18.0);
        let desired = target_rms / rms_in;
        let desired = desired.clamp(0.03, 10.0); // +/-30dB max
        let coef = coef(500.0, self.sr);
        self.auto_gain += coef * (desired - self.auto_gain);
    }
}

fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() { return 0.0; }
    let sum: f64 = buf.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

#[inline] fn db_to_lin(db: f32) -> f32 { 10f32.powf(db / 20.0) }
#[inline] fn lin_to_db(lin: f32) -> f32 { 20.0 * lin.max(1e-9).log10() }
#[inline] fn coef(ms: f64, sr: f64) -> f32 {
    (1.0 - (-2.2 / (ms * 0.001 * sr)).exp()) as f32
}
