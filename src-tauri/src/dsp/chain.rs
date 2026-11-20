/// Complete DSP processing pipeline.
/// Chain order:
///   input → HPF(80Hz) → Neural denoise (RNNoise) → Gate → Noise Reduction → EQ Warmth → EQ Clarity
///         → De-esser → Compressor → Auto Gain → Limiter → LUFS meter → output
use super::{
    biquad::Biquad,
    compressor::{Compressor, Limiter},
    desser::DeEsser,
    dereverb::SpectralDereverb,
    gate::{GateMode, SwitchedGate},
    hypergate::LearnOutcome,
    lufs::{LufsMeter, LufsReadings},
    neural::{NeuralDenoiser, FRAME as NEURAL_FRAME},
    noise::SpectralDenoiser,
    DspParams,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct DspChain {
    sr: f64,
    input_gain_db: f32,
    compressor_enabled: bool,

    // Pre-processing
    hpf: Biquad,
    neural: NeuralDenoiser,
    gate: SwitchedGate,
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

    // Hyper gate bookkeeping (see `hypergate::params`).
    /// Fingerprint generation installed in the gate (None before the first sync).
    fp_generation: Option<u32>,
    /// Last learn request seen (None before the first sync, so a new chain
    /// does not replay an old request).
    learn_seen: Option<u32>,
    /// Request being learned and its finished outcome, published at the
    /// next `sync_params`.
    learning_for: u32,
    pending_learn: Option<LearnOutcome>,
    /// Pre-RNNoise copy of the current chunk: the Hyper gate's key.
    gate_key: [f32; NEURAL_FRAME],
}

impl DspChain {
    pub fn new(sr: f64) -> Self {
        DspChain {
            sr,
            input_gain_db: 0.0,
            compressor_enabled: true,
            hpf: Biquad::hpf(80.0, sr),
            neural: NeuralDenoiser::new(sr),
            gate: SwitchedGate::new_hyper(sr),
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
            fp_generation: None,
            learn_seen: None,
            learning_for: 0,
            pending_learn: None,
            gate_key: [0.0; NEURAL_FRAME],
        }
    }

    /// Sync all parameters from the shared atomic state.
    pub fn sync_params(&mut self, p: &Arc<DspParams>) {
        use Ordering::Relaxed;

        self.input_gain_db = p.gain_db.load(Relaxed);

        // Gate
        self.gate.enabled = p.gate_enabled.load(Relaxed);
        self.gate.set_threshold_db(p.gate_threshold_db.load(Relaxed));
        let gate_running = self.gate.enabled && !p.bypass.load(Relaxed);
        self.sync_hyper(p, gate_running);

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

    /// Hyper gate mode, fingerprint and "learn mic" handshake. Atomics and
    /// fixed-size copies only. Learning needs audio through the gate, so with
    /// the gate off or the chain bypassed a request is answered `GateOff`.
    fn sync_hyper(&mut self, p: &DspParams, gate_running: bool) {
        let h = &p.hyper;
        let mode = if h.speech_only.load(Ordering::Relaxed) { GateMode::Speech } else { GateMode::Balanced };
        self.gate.set_mode(mode);

        if let Some(outcome) = self.pending_learn.take() {
            h.publish(self.learning_for, &outcome);
        }
        let generation = h.fingerprint_generation();
        if self.fp_generation != Some(generation) {
            self.fp_generation = Some(generation);
            self.gate.set_fingerprint(h.fingerprint());
        }
        let request = h.learn_request();
        if self.learn_seen.is_some_and(|seen| seen != request) && !h.is_abandoned(request) {
            self.learning_for = request;
            if gate_running {
                self.gate.start_learning();
            } else {
                h.publish(request, &LearnOutcome::GateOff);
            }
        }
        self.learn_seen = Some(request);
        if self.gate.is_learning() {
            if h.is_abandoned(self.learning_for) {
                self.gate.cancel_learning();
            } else if !gate_running {
                self.gate.cancel_learning();
                h.publish(self.learning_for, &LearnOutcome::GateOff);
            }
        }
    }

    /// Process one block of mono samples in-place.
    /// Returns the latest LUFS readings if a new 100ms block completed.
    pub fn process_block(&mut self, buf: &mut [f32], bypass: bool) {
        if bypass {
            self.bypassed = true;
            return;
        }
        if self.bypassed {
            // Coming back from bypass: the gate/neural/spectral delay lines
            // hold stale audio. Restart them so they refill and crossfade in.
            self.bypassed = false;
            self.gate.restart();
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

        // 2b. Neural denoise (RNNoise) and 3. Gate
        self.run_neural_and_gate(buf);

        // 4. Noise reduction (streaming STFT, 1024 samples when on, 0 when off)
        self.noise.process_block(buf);

        // 5. Dereverb (streaming STFT, 1024 samples when on, 0 when off)
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

    /// 2b. Neural denoise (RNNoise; inactive unless the engine runs at 48 kHz)
    /// 3. Gate (20 ms lookahead when on; zero-latency passthrough when off,
    ///    crossfaded on toggle).
    /// The Hyper gate uses RNNoise's VAD, which changes only when an RNNoise
    /// frame completes, so both stages run in chunks that end on frame
    /// boundaries: every sample then sees the same VAD whatever the device
    /// block size.
    /// The Hyper gate also LISTENS to the audio before RNNoise (its key):
    /// RNNoise chops music and noise into speech-like bursts, which would
    /// fool the syllable and noise checks. The gain still applies to the
    /// denoised audio. With RNNoise on, the key runs 20 ms ahead of the
    /// gated audio, which the 350 ms voice hold covers.
    fn run_neural_and_gate(&mut self, buf: &mut [f32]) {
        let mut start = 0;
        while start < buf.len() {
            // ≤ NEURAL_FRAME samples, so the key fits the fixed scratch.
            let end = (start + self.neural.samples_to_frame_end()).min(buf.len());
            let vad = self.neural.vad();
            let chunk = &mut buf[start..end];
            let key = &mut self.gate_key[..chunk.len()];
            key.copy_from_slice(chunk);
            self.neural.process_block(chunk);
            self.last_gate_gain = self.gate.process_block_keyed(chunk, Some(key), vad);
            start = end;
        }
        if let Some(outcome) = self.gate.take_learn_outcome() {
            self.pending_learn = Some(outcome);
        }
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
    /// Current auto-gain rider level (linear), shared with the live path.
    pub fn auto_gain(&self) -> f32 {
        self.auto_gain
    }

    pub fn total_latency_samples(&self) -> usize {
        if self.bypassed {
            return 0;
        }
        self.neural.latency_samples()
            + self.noise.latency_samples()
            + self.dereverb.latency_samples()
            + self.gate.latency_samples()
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

        params.gate_enabled.store(false, Ordering::Relaxed);
        chain.sync_params(&params);
        assert_eq!(chain.total_latency_samples(), chain.limiter.latency_samples());
        params.gate_enabled.store(true, Ordering::Relaxed);
        chain.sync_params(&params);

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

    fn hyper_chain(params: &Arc<DspParams>) -> DspChain {
        params.neural_enabled.store(true, Ordering::Relaxed);
        params.hyper.speech_only.store(true, Ordering::Relaxed);
        let mut chain = DspChain::new(SR);
        chain.sync_params(params);
        chain
    }

    // The whole chain is not block-size invariant (the auto-gain rider
    // updates once per block), so this checks the RNNoise → Hyper gate
    // section, where the VAD hand-over happens.
    #[test]
    fn hyper_gate_with_rnnoise_vad_is_block_size_invariant() {
        use crate::dsp::hypergate::signals::{mix, music, speech};
        let input = mix(&speech(48_000 * 3, 31, -20.0), &music(48_000 * 3, -30.0));
        let outputs: Vec<Vec<f32>> = [64usize, 256, 480, 1000, 4096]
            .iter()
            .map(|&block| {
                let params = Arc::new(DspParams::defaults());
                let mut chain = hyper_chain(&params);
                crate::dsp::test_util::run_blocks(&input, block, |b| {
                    chain.sync_params(&params);
                    chain.run_neural_and_gate(b);
                })
            })
            .collect();
        for (i, out) in outputs.iter().enumerate().skip(1) {
            let diff = crate::dsp::test_util::max_abs_diff(&outputs[0], out);
            assert!(diff < 1e-5, "run {i}: diff {diff}");
        }
    }

    #[test]
    fn learn_mic_through_params_and_reuse_on_a_new_chain() {
        use crate::dsp::hypergate::{signals::speech, LearnWait};
        let params = Arc::new(DspParams::defaults());
        let mut chain = hyper_chain(&params);
        let request = params.hyper.request_learn();
        let input = speech(48_000 * 6, 32, -20.0);
        for chunk in input.chunks(512) {
            chain.sync_params(&params);
            let mut block = chunk.to_vec();
            chain.process_block(&mut block, false);
            if params.hyper.learn_result(request).is_some() {
                break;
            }
        }
        chain.sync_params(&params); // publishes a result finished in the last block
        assert_eq!(params.hyper.learn_result(request), Some(LearnWait::Learned));
        let learned = params.hyper.fingerprint().expect("fingerprint stored");
        assert_eq!(chain.gate.gate().hyper().and_then(|h| h.fingerprint()), Some(&learned));

        // A restarted engine builds a new chain: it installs the stored
        // fingerprint and does not replay the old learn request.
        let mut fresh = DspChain::new(SR);
        fresh.sync_params(&params);
        assert_eq!(fresh.gate.gate().hyper().and_then(|h| h.fingerprint()), Some(&learned));
        assert!(!fresh.gate.gate().hyper().is_some_and(|h| h.is_learning()));
    }

    /// Whole chain as the app runs it with Reduction on (RNNoise feeds the
    /// gate): level of `input` after 2 s, dBFS.
    fn full_chain_db(input: &[f32], speech_only: bool) -> f32 {
        let params = Arc::new(DspParams::defaults());
        params.noise_enabled.store(true, Ordering::Relaxed);
        let mut chain = hyper_chain(&params);
        params.hyper.speech_only.store(speech_only, Ordering::Relaxed);
        let out = crate::dsp::test_util::run_blocks(input, 480, |b| {
            chain.sync_params(&params);
            chain.process_block(b, false);
        });
        20.0 * crate::dsp::test_util::rms(&out[96_000..]).max(1e-9).log10()
    }

    // Regression: keyed on the denoised audio, the gate took RNNoise's
    // chopped-up music for syllables and let it through in speech-only mode.
    #[test]
    fn full_chain_speech_only_holds_back_music_and_passes_speech() {
        use crate::dsp::hypergate::signals::{music, speech};
        let keys = music(48_000 * 6, -20.0);
        let (balanced, speech_only) = (full_chain_db(&keys, false), full_chain_db(&keys, true));
        assert!(balanced - speech_only > 15.0, "music: balanced {balanced:.1}, speech only {speech_only:.1}");

        let voice = speech(48_000 * 6, 33, -20.0);
        let (balanced, speech_only) = (full_chain_db(&voice, false), full_chain_db(&voice, true));
        assert!((balanced - speech_only).abs() < 3.0, "speech: balanced {balanced:.1}, speech only {speech_only:.1}");
    }

    #[test]
    fn learn_mic_answers_gate_off_and_drops_abandoned_requests() {
        use crate::dsp::hypergate::{signals::speech, LearnWait};
        let params = Arc::new(DspParams::defaults());
        let mut chain = hyper_chain(&params);
        let mut block = vec![0.0f32; 480];

        params.gate_enabled.store(false, Ordering::Relaxed);
        let off = params.hyper.request_learn();
        chain.sync_params(&params);
        assert_eq!(params.hyper.learn_result(off), Some(LearnWait::GateOff));

        // Bypass mid-learn cancels with GateOff too.
        params.gate_enabled.store(true, Ordering::Relaxed);
        let bypassed = params.hyper.request_learn();
        chain.sync_params(&params);
        chain.process_block(&mut block, false);
        params.bypass.store(true, Ordering::Relaxed);
        chain.sync_params(&params);
        assert_eq!(params.hyper.learn_result(bypassed), Some(LearnWait::GateOff));
        params.bypass.store(false, Ordering::Relaxed);

        // A request the command gave up on never learns, even with speech.
        let late = params.hyper.request_learn();
        params.hyper.abandon(late);
        for chunk in speech(48_000 * 6, 34, -20.0).chunks(480) {
            chain.sync_params(&params);
            let mut b = chunk.to_vec();
            chain.process_block(&mut b, false);
        }
        chain.sync_params(&params);
        assert_eq!(params.hyper.learn_result(late), None);
        assert_eq!(params.hyper.fingerprint(), None);
    }
}
