/// Neural voice denoiser — RNNoise via the pure-Rust `nnnoiseless` port.
///
/// RNNoise works on fixed 480-sample frames at 48 kHz with samples scaled to
/// the i16 range. This wrapper keeps 480-sample input/output FIFOs so it
/// accepts blocks of ANY length, allocates only in `new()`, and has a fixed
/// latency of 480 samples (10 ms).
///
/// Latency / bypass: consistent with the spectral stages, the stage keeps the
/// same 480-sample delay line when disabled (dry signal passes through the
/// FIFO), so toggling never jumps in time. The wet/dry amount is ramped
/// linearly across each frame, so toggling or moving `mix` does not click.
/// While fully dry the network is not run (no CPU cost).
///
/// Sample rate: RNNoise is trained for 48 kHz only. At any other engine rate
/// the stage is inactive — `available()` is false, `process_block` leaves the
/// audio untouched and the latency is 0. Resampling is future work.
use nnnoiseless::DenoiseState;

pub const FRAME: usize = 480;
pub const REQUIRED_SAMPLE_RATE: f64 = 48_000.0;

const I16_SCALE: f32 = 32_768.0;

pub struct NeuralDenoiser {
    state: Option<Box<DenoiseState<'static>>>,
    /// Current input frame, already scaled to the i16 range.
    in_frame: Vec<f32>,
    /// Network output for the current frame (i16 range).
    net_out: Vec<f32>,
    /// Finished output samples played out during the next frame.
    ready: Vec<f32>,
    pos: usize,
    /// Wet amount actually applied at the end of the previous frame.
    current_wet: f32,
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
            in_frame: vec![0.0; FRAME],
            net_out: vec![0.0; FRAME],
            ready: vec![0.0; FRAME],
            pos: 0,
            current_wet: 0.0,
            last_vad: 0.0,
            enabled: false,
            mix: 1.0,
        }
    }

    /// True when the engine runs at 48 kHz and the network can be used.
    pub fn available(&self) -> bool {
        self.state.is_some()
    }

    /// Fixed latency in samples (0 when unavailable).
    pub fn latency_samples(&self) -> usize {
        if self.available() {
            FRAME
        } else {
            0
        }
    }

    /// Voice-activity probability (0..1) of the most recent frame.
    pub fn last_vad(&self) -> f32 {
        self.last_vad
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        if self.state.is_none() {
            return;
        }
        for s in buf.iter_mut() {
            let x = *s;
            *s = self.ready[self.pos];
            self.in_frame[self.pos] = x * I16_SCALE;
            self.pos += 1;
            if self.pos == FRAME {
                self.pos = 0;
                self.run_frame();
            }
        }
    }

    fn run_frame(&mut self) {
        let target_wet = if self.enabled {
            if self.mix.is_finite() {
                self.mix.clamp(0.0, 1.0)
            } else {
                1.0
            }
        } else {
            0.0
        };
        let start_wet = self.current_wet;

        let run_network = start_wet > 0.0 || target_wet > 0.0;
        if run_network {
            if let Some(state) = self.state.as_mut() {
                let vad = state.process_frame(&mut self.net_out[..], &self.in_frame[..]);
                self.last_vad = if vad.is_finite() { vad } else { 0.0 };
            }
        } else {
            self.last_vad = 0.0;
        }

        let inv_scale = 1.0 / I16_SCALE;
        let step = (target_wet - start_wet) / FRAME as f32;
        for i in 0..FRAME {
            let dry = self.in_frame[i] * inv_scale;
            let out = if run_network {
                let wet_amount = start_wet + step * (i + 1) as f32;
                let wet = self.net_out[i] * inv_scale;
                let wet = if wet.is_finite() { wet } else { 0.0 };
                dry + (wet - dry) * wet_amount
            } else {
                dry
            };
            self.ready[i] = out;
        }
        self.current_wet = target_wet;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::test_util::{max_abs_diff, rms, run_blocks, test_signal, white_noise};

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
        assert_eq!(d.latency_samples(), 0);
        d.enabled = true;
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
    fn disabled_passes_through_with_same_latency() {
        let input = test_signal(SR, 9);
        let mut d = NeuralDenoiser::new(SR as f64);
        let lat = d.latency_samples();
        assert_eq!(lat, FRAME);
        let out = run_blocks(&input, 300, |b| d.process_block(b));
        for t in lat..out.len() {
            assert!((out[t] - input[t - lat]).abs() < 1e-6, "t={t}");
        }
        assert!(out[..lat].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn white_noise_is_reduced_by_more_than_6_db() {
        let mut d = enabled();
        let noise = white_noise(SR * 3, 17, 0.1);
        let out = run_blocks(&noise, 512, |b| d.process_block(b));
        let skip = SR + d.latency_samples(); // 1 s warm-up
        let in_rms = rms(&noise[skip..]);
        let out_rms = rms(&out[skip..]);
        let reduction_db = 20.0 * (in_rms / out_rms.max(1e-12)).log10();
        assert!(reduction_db > 6.0, "only {reduction_db} dB");
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
