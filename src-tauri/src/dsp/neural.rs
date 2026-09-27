/// Neural voice denoiser — RNNoise via the pure-Rust `nnnoiseless` port.
///
/// RNNoise works on fixed 480-sample frames at 48 kHz with samples scaled to
/// the i16 range. This wrapper keeps 480-sample input/output FIFOs so it
/// accepts blocks of ANY length and allocates only in `new()`.
///
/// Latency: 960 samples (20 ms) while enabled — 480 for collecting a frame
/// plus 480 inside RNNoise itself: `process_frame` returns the PREVIOUS
/// frame's audio (its synthesis overlap-adds a 960-sample window). The dry
/// path is delayed by the same frame so dry/wet mixing stays phase-aligned.
/// When disabled the stage is a
/// zero-latency passthrough (`latency_samples()` = 0) and the network is not
/// run. Enabling it resets the FIFOs and crossfades from dry to wet (see
/// `stage_switch.rs`); disabling crossfades back to dry. The RNN's hidden
/// state is NOT reset on re-enable (rebuilding `DenoiseState` would allocate
/// on the audio thread); it re-adapts within a few frames, which the dry fill
/// and crossfade cover. Moving `mix` is ramped linearly across each frame.
///
/// Sample rate: RNNoise is trained for 48 kHz only. At any other engine rate
/// the stage is inactive — `available()` is false, `process_block` leaves the
/// audio untouched and the latency is 0. Resampling is future work.
use super::stage_switch::StageSwitch;
use nnnoiseless::DenoiseState;

pub const FRAME: usize = 480;
/// Total stage latency: one frame of buffering + RNNoise's internal frame.
pub const LATENCY: usize = 2 * FRAME;
pub const REQUIRED_SAMPLE_RATE: f64 = 48_000.0;

const I16_SCALE: f32 = 32_768.0;

pub struct NeuralDenoiser {
    state: Option<Box<DenoiseState<'static>>>,
    switch: StageSwitch,
    /// Current input frame, already scaled to the i16 range.
    in_frame: Vec<f32>,
    /// Network output (i16 range). RNNoise delays by one frame, so this is
    /// the denoised version of `prev_frame`, not of `in_frame`.
    net_out: Vec<f32>,
    /// Previous input frame (i16 range): the dry signal aligned with `net_out`.
    prev_frame: Vec<f32>,
    /// Finished output samples played out during the next frame.
    ready: Vec<f32>,
    pos: usize,
    /// `mix` actually applied at the end of the previous frame.
    current_mix: f32,
    last_vad: f32,
    pub enabled: bool,
    /// Wet amount 0..1 (1 = fully denoised).
    pub mix: f32,
}

impl NeuralDenoiser {
    pub fn new(sample_rate: f64) -> Self {
        let available = (sample_rate - REQUIRED_SAMPLE_RATE).abs() < 0.5;
        NeuralDenoiser {
            state: if available { Some(DenoiseState::new()) } else { None },
            switch: StageSwitch::new(LATENCY),
            in_frame: vec![0.0; FRAME],
            net_out: vec![0.0; FRAME],
            prev_frame: vec![0.0; FRAME],
            ready: vec![0.0; FRAME],
            pos: 0,
            current_mix: 1.0,
            last_vad: 0.0,
            enabled: false,
            mix: 1.0,
        }
    }

    /// True when the engine runs at 48 kHz and the network can be used.
    pub fn available(&self) -> bool {
        self.state.is_some()
    }

    /// Current processing latency: `LATENCY` when enabled and available, else 0.
    pub fn latency_samples(&self) -> usize {
        if self.available() && self.enabled {
            LATENCY
        } else {
            0
        }
    }

    /// Voice-activity probability (0..1) of the most recent frame.
    pub fn last_vad(&self) -> f32 {
        self.last_vad
    }

    /// Drop the streaming state; the next enabled block fades in from dry.
    /// Used when the whole chain comes back from bypass.
    pub fn restart(&mut self) {
        self.switch.force_restart();
    }

    fn target_mix(&self) -> f32 {
        if self.mix.is_finite() {
            self.mix.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    fn reset_fifos(&mut self) {
        for v in self.in_frame.iter_mut() {
            *v = 0.0;
        }
        for v in self.prev_frame.iter_mut() {
            *v = 0.0;
        }
        for v in self.ready.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
        self.current_mix = self.target_mix();
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        if self.state.is_none() {
            return;
        }
        if self.switch.update(self.enabled) {
            self.reset_fifos();
        }
        if !self.switch.is_running() {
            self.last_vad = 0.0;
            return; // zero-latency passthrough
        }

        for s in buf.iter_mut() {
            if !self.switch.is_running() {
                self.last_vad = 0.0;
                break; // fade-out finished mid-block: rest stays dry
            }
            let x = *s;
            let wet = self.tick(x);
            let amount = self.switch.next_mix();
            *s = x + (wet - x) * amount;
        }
    }

    /// Push one sample, get the (mix-applied) output `LATENCY` samples behind it.
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.ready[self.pos];
        self.in_frame[self.pos] = x * I16_SCALE;
        self.pos += 1;
        if self.pos == FRAME {
            self.pos = 0;
            self.run_frame();
        }
        y
    }

    fn run_frame(&mut self) {
        let target_mix = self.target_mix();
        let start_mix = self.current_mix;

        if let Some(state) = self.state.as_mut() {
            let vad = state.process_frame(&mut self.net_out[..], &self.in_frame[..]);
            self.last_vad = if vad.is_finite() { vad } else { 0.0 };
        }

        let inv_scale = 1.0 / I16_SCALE;
        let step = (target_mix - start_mix) / FRAME as f32;
        for i in 0..FRAME {
            let dry = self.prev_frame[i] * inv_scale;
            let wet = self.net_out[i] * inv_scale;
            let wet = if wet.is_finite() { wet } else { 0.0 };
            let amount = start_mix + step * (i + 1) as f32;
            self.ready[i] = dry + (wet - dry) * amount;
        }
        self.current_mix = target_mix;
        self.prev_frame.copy_from_slice(&self.in_frame[..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::{max_abs_diff, rms, run_blocks, sine, test_signal, white_noise};

    const SR: usize = 48_000;

    fn enabled() -> NeuralDenoiser {
        let mut d = NeuralDenoiser::new(SR as f64);
        d.enabled = true;
        d
    }

    #[test]
    fn available_only_at_48k() {
        assert!(NeuralDenoiser::new(48_000.0).available());
        let mut d = NeuralDenoiser::new(44_100.0);
        assert!(!d.available());
        d.enabled = true;
        assert_eq!(d.latency_samples(), 0);
        let input = test_signal(4_000, 1);
        let out = run_blocks(&input, 256, |b| d.process_block(b));
        assert_eq!(out, input);
    }

    #[test]
    fn block_size_invariance() {
        let input = test_signal(SR * 2, 31);
        let outputs: Vec<Vec<f32>> = [64usize, 256, 480, 1000, 4096]
            .iter()
            .map(|&block| {
                let mut d = enabled();
                run_blocks(&input, block, |b| d.process_block(b))
            })
            .collect();
        for (i, out) in outputs.iter().enumerate().skip(1) {
            let diff = max_abs_diff(&outputs[0], out);
            assert!(diff < 1e-5, "run {i}: diff {diff}");
        }
    }

    #[test]
    fn disabled_is_exact_zero_latency_passthrough() {
        let input = test_signal(SR, 9);
        for &block in &[64usize, 256, 480, 1000, 4096] {
            let mut d = NeuralDenoiser::new(SR as f64);
            assert_eq!(d.latency_samples(), 0);
            let out = run_blocks(&input, block, |b| d.process_block(b));
            assert_eq!(out, input, "block {block}");
        }
        assert_eq!(enabled().latency_samples(), LATENCY);
    }

    #[test]
    fn enable_starts_dry_until_latency_is_filled() {
        let input = test_signal(SR / 2, 10);
        let mut d = enabled();
        let out = run_blocks(&input, 300, |b| d.process_block(b));
        assert_eq!(&out[..LATENCY], &input[..LATENCY]);
    }

    #[test]
    fn zero_mix_wet_path_is_input_delayed_by_latency() {
        // mix 0: the wet path is the aligned dry signal, so after the fill
        // and crossfade the output must be the input delayed by LATENCY.
        let input = test_signal(SR, 12);
        let mut d = enabled();
        d.mix = 0.0;
        let out = run_blocks(&input, 256, |b| d.process_block(b));
        let settle = LATENCY + crate::dsp::stage_switch::XFADE_SAMPLES;
        for t in settle..out.len() {
            assert!((out[t] - input[t - LATENCY]).abs() < 1e-5, "t={t}");
        }
    }

    #[test]
    fn toggling_does_not_click() {
        let input = sine(SR * 2, 440.0, SR as f32, 0.5);
        let mut d = NeuralDenoiser::new(SR as f64);
        // mix 0: "wet" is the delayed dry signal, so this measures only the
        // on/off switching (dry fill + crossfade), not RNNoise's own output.
        d.mix = 0.0;
        let mut out = Vec::with_capacity(input.len());
        for (i, chunk) in input.chunks(480).enumerate() {
            d.enabled = (i / 20) % 2 == 1;
            let mut block = chunk.to_vec();
            d.process_block(&mut block);
            out.extend_from_slice(&block);
        }
        // Sine slope < 0.03/sample; the crossfade adds ≤ 2 × 0.5 / 480.
        let max_step = out.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(max_step < 0.04, "step {max_step}");
    }

    // RNNoise is trained on real, non-stationary noise; loud synthetic white
    // noise (-25 dBFS) measured 3.5 dB of reduction on CI. The test proves the
    // network is running on the audio, not a quality target.
    #[test]
    fn white_noise_is_reduced() {
        let mut d = enabled();
        let noise = white_noise(SR * 3, 17, 0.1);
        let out = run_blocks(&noise, 512, |b| d.process_block(b));
        let skip = SR; // 1 s warm-up (covers fill + crossfade)
        let in_rms = rms(&noise[skip..]);
        let out_rms = rms(&out[skip..]);
        let reduction_db = 20.0 * (in_rms / out_rms.max(1e-12)).log10();
        assert!(reduction_db > 2.0, "only {reduction_db} dB");
    }

    #[test]
    fn no_nan_on_silence_or_full_scale() {
        let mut d = enabled();
        let silence = vec![0.0f32; SR];
        let out = run_blocks(&silence, 256, |b| d.process_block(b));
        assert!(out.iter().all(|s| s.is_finite()));
        let full: Vec<f32> = (0..SR).map(|i| if (i / 50) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = run_blocks(&full, 256, |b| d.process_block(b));
        assert!(out.iter().all(|s| s.is_finite()));
        assert!(d.last_vad().is_finite());
    }
}
