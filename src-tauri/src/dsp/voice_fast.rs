//! The live (fast) voice path: what the room hears.
//!
//! The full `DspChain` sounds best but waits ahead to work: RNNoise (20 ms),
//! two spectral stages (about 21 ms each), a 20 ms lookahead gate and a
//! 5 ms limiter. That suits the stream and the recording, where AetherCast's
//! lip-sync delay absorbs it, but a voice that comes back 80 ms late in the
//! PA or in someone's ears is an echo.
//!
//! This path runs the same settings without the stages that look ahead:
//! input gain → HPF → Hyper gate (3 ms lookahead) → EQ → de-esser →
//! compressor → the full chain's auto-gain level. It has no limiter of its
//! own: Main and the monitors have their bus limiters. Its latency is the
//! 3 ms gate lookahead while the gate is on, and 0 while it is off.
//!
//! Allocation-free and lock-free per block; buffers are sized in `new`.
use super::{
    biquad::Biquad,
    compressor::Compressor,
    desser::DeEsser,
    gate::{GateMode, SwitchedGate},
    DspParams,
};
use std::sync::atomic::Ordering::Relaxed;

/// Gate lookahead on the live path, in ms.
pub const FAST_GATE_LOOKAHEAD_MS: f64 = 3.0;

pub struct FastVoiceChain {
    input_gain_db: f32,
    compressor_enabled: bool,
    hpf: Biquad,
    gate: SwitchedGate,
    eq_warmth: Biquad,
    eq_clarity: Biquad,
    desser: DeEsser,
    compressor: Compressor,
    fp_generation: Option<u32>,
    bypassed: bool,
}

impl FastVoiceChain {
    pub fn new(sr: f64) -> Self {
        FastVoiceChain {
            input_gain_db: 0.0,
            compressor_enabled: true,
            // Same fixed filters as `DspChain`, so both paths sound alike.
            hpf: Biquad::hpf(80.0, sr),
            gate: SwitchedGate::new_hyper_with_lookahead_ms(sr, FAST_GATE_LOOKAHEAD_MS),
            eq_warmth: Biquad::low_shelf(200.0, 3.0, sr),
            eq_clarity: Biquad::peaking(3000.0, 0.8, 2.0, sr),
            desser: DeEsser::new(sr),
            compressor: Compressor::new(sr),
            fp_generation: None,
            bypassed: false,
        }
    }

    /// Follow the voice settings. Learning the mic stays with the full chain;
    /// this path only picks up the learned fingerprint.
    pub fn sync_params(&mut self, p: &DspParams) {
        self.input_gain_db = p.gain_db.load(Relaxed);
        self.gate.enabled = p.gate_enabled.load(Relaxed);
        self.gate.set_threshold_db(p.gate_threshold_db.load(Relaxed));
        let speech = p.hyper.speech_only.load(Relaxed);
        self.gate.set_mode(if speech { GateMode::Speech } else { GateMode::Balanced });
        let generation = p.hyper.fingerprint_generation();
        if self.fp_generation != Some(generation) {
            self.fp_generation = Some(generation);
            self.gate.set_fingerprint(p.hyper.fingerprint());
        }
        self.compressor_enabled = p.comp_enabled.load(Relaxed);
        self.compressor.set_threshold(p.comp_threshold_db.load(Relaxed));
        self.compressor.set_ratio(p.comp_ratio.load(Relaxed));
        self.desser.enabled = p.deess_enabled.load(Relaxed);
        self.desser.set_threshold_db(p.deess_threshold_db.load(Relaxed));
    }

    /// Process one block in place. `auto_gain` is the full chain's current
    /// level rider (a slow, 500 ms rider, so sharing it costs no latency and
    /// keeps both paths at the same level).
    pub fn process_block(&mut self, buf: &mut [f32], bypass: bool, auto_gain: f32) {
        if bypass {
            self.bypassed = true;
            return;
        }
        if self.bypassed {
            self.bypassed = false;
            self.gate.restart();
        }
        if buf.is_empty() {
            return;
        }
        let gain = 10f32.powf(self.input_gain_db / 20.0);
        for s in buf.iter_mut() {
            *s = self.hpf.tick(*s * gain);
        }
        self.gate.process_block(buf);
        for s in buf.iter_mut() {
            *s = self.eq_clarity.tick(self.eq_warmth.tick(*s));
        }
        self.desser.process_block(buf);
        if self.compressor_enabled {
            self.compressor.process_block(buf);
        }
        let ag = if auto_gain.is_finite() { auto_gain.clamp(0.0, 10.0) } else { 1.0 };
        for s in buf.iter_mut() {
            *s *= ag;
        }
    }

    /// Latency of this path in samples (the gate lookahead while it is on).
    pub fn latency_samples(&self) -> usize {
        self.gate.latency_samples()
    }
}

#[cfg(test)]
#[path = "voice_fast_tests.rs"]
mod tests;
