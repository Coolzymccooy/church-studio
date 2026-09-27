/// Complete DSP processing pipeline.
/// Chain order:
///   input → HPF(80Hz) → Neural denoise (RNNoise) → Gate → Noise Reduction → EQ Warmth → EQ Clarity
///         → De-esser → Compressor → Auto Gain → Limiter → LUFS meter → output
use super::{
    biquad::Biquad,
    compressor::{Compressor, Limiter},
    desser::DeEsser,
    dereverb::SpectralDereverb,
    gate::Gate,
    lufs::{LufsMeter, LufsReadings},
    neural::NeuralDenoiser,
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
    neural: NeuralDenoiser,
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

    /// True while the global bypass is engaged (block-latency stages are
    /// restarted with a crossfade when it is released).
    bypassed: bool,
}

impl DspChain {
    pub fn new(sr: f64) -> Self {
        DspChain {
            sr,
            input_gain_db: 0.0,
            gate_enabled: true,
            compressor_enabled: true,
            hpf: Biquad::hpf(80.0, sr),
            neural: NeuralDenoiser::new(sr),
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
            bypassed: false,
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

        // Neural denoise
        self.neural.enabled = p.neural_enabled.load(Relaxed);
        self.neural.mix = p.neural_mix.load(Relaxed);

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
    pub fn process_block(&mut self, buf: &mut [f32], bypass: bool) {
        if bypass {
            self.bypassed = true;
            return;
        }
        if self.bypassed {
            // Coming back from bypass: the neural/spectral FIFOs hold stale
            // audio. Restart them so they refill and crossfade in from dry.
            self.bypassed = false;
            self.neural.restart();
            self.noise.restart();
            self.dereverb.restart();
        }
        if buf.is_empty() { return; }

        let gain_lin = db_to_lin(self.input_gain_db);

        // ── Input level ──────────────────────────────────────────────────────
        let in_rms = rms(buf);
        self.last_input_db = lin_to_db(in_rms.max(1e-9));

        // 1. Apply input gain
        for s in buf.iter_mut() { *s *= gain_lin; }

        // 2. HPF
        for s in buf.iter_mut() { *s = self.hpf.tick(*s); }

        // 2b. Neural denoise (RNNoise; inactive unless the engine runs at 48 kHz)
        self.neural.process_block(buf);

        // 3. Gate
        let gate_gain = if self.gate_enabled {
            self.gate.process_block(buf)
        } else { 1.0 };
        self.last_gate_gain = gate_gain;

        // 4. Noise reduction (streaming STFT, fixed 1024-sample latency)
        self.noise.process_block(buf);

        // 5. Dereverb (streaming STFT, fixed 1024-sample latency)
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

    /// True when the RNNoise stage can run (engine at 48 kHz).
    pub fn neural_available(&self) -> bool {
        self.neural.available()
    }

    /// Latest RNNoise voice-activity probability.
    pub fn neural_vad(&self) -> f32 {
        self.neural.last_vad()
    }

    /// Processing latency of the chain as currently configured, in samples:
    /// the sum over the ACTIVE stages (neural 960, noise 1024, dereverb 1024,
    /// gate lookahead when the gate is on, limiter lookahead). Disabled
    /// stages are zero-latency passthroughs, and the global bypass is 0.
    /// Reflects the params from the most recent `sync_params`/`process_block`.
    pub fn total_latency_samples(&self) -> usize {
        if self.bypassed {
            return 0;
        }
        let gate = if self.gate_enabled { self.gate.latency_samples() } else { 0 };
        self.neural.latency_samples()
            + self.noise.latency_samples()
            + self.dereverb.latency_samples()
            + gate
            + self.limiter.latency_samples()
    }

    /// Install a noise power spectrum computed off the audio thread by
    /// `noise::compute_noise_profile`. Copies only — safe in the callback.
    pub fn set_noise_profile(&mut self, noise_pow: &[f32]) -> bool {
        self.noise.set_profile(noise_pow)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::test_signal;

    const SR: f64 = 48_000.0;

    #[test]
    fn total_latency_counts_only_active_stages() {
        let params = Arc::new(DspParams::defaults());
        let mut chain = DspChain::new(SR);
        chain.sync_params(&params);
        // Defaults: gate on (20 ms lookahead) + limiter (5 ms); spectral and
        // neural stages off, so they add nothing.
        let base = chain.gate.latency_samples() + chain.limiter.latency_samples();
        assert_eq!(chain.total_latency_samples(), base);

        params.dereverb_enabled.store(true, Ordering::Relaxed);
        params.neural_enabled.store(true, Ordering::Relaxed);
        chain.sync_params(&params);
        assert_eq!(chain.total_latency_samples(), base + 1024 + crate::dsp::neural::LATENCY);

        let mut block = test_signal(512, 1);
        chain.process_block(&mut block, true);
        assert_eq!(chain.total_latency_samples(), 0);
        chain.process_block(&mut block, false);
        assert_eq!(chain.total_latency_samples(), base + 1024 + crate::dsp::neural::LATENCY);
    }

    #[test]
    fn unbypass_restarts_block_stages_without_nan() {
        let params = Arc::new(DspParams::defaults());
        params.dereverb_enabled.store(true, Ordering::Relaxed);
        let mut chain = DspChain::new(SR);
        let input = test_signal(48_000, 2);
        let mut out = Vec::with_capacity(input.len());
        for (i, chunk) in input.chunks(480).enumerate() {
            chain.sync_params(&params);
            let mut block = chunk.to_vec();
            chain.process_block(&mut block, (i / 25) % 2 == 1);
            out.extend_from_slice(&block);
        }
        assert!(out.iter().all(|s| s.is_finite()));
    }
}
