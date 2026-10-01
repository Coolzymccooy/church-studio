//! Everything the input callback does, as one Send struct that the cpal
//! closure owns. Allocation-free and lock-free per block (the only
//! allocation is the 128-bin spectrum Vec built ~50×/s for `audio-meters`,
//! exactly as before the mixer); all buffers are sized in `new`.
//!
//! Per chunk of at most `MAX_CALLBACK_FRAMES` frames:
//! 1. De-interleave every input channel (up to `MAX_STRIPS`) into its own
//!    buffer; strip i reads input channel i. No mono fold.
//! 2. The strip holding `voice_chain` runs the `DspChain` in place on its
//!    buffer (history for noise-profile capture, noise-profile install,
//!    params sync, spectrum and `audio-meters` all follow that strip).
//! 3. `Mixer::process` sums the strips into Main, Stream and Monitor; the
//!    strip and bus meters are published to lock-free slots.
//! 4. Each output device gets its bus as stereo (see `routing`); the
//!    Monitor bus is still scaled by `monitor_gain_db`.
//! 5. Each NDI output gets its bus as interleaved stereo at unity gain, in
//!    its own ring; the NDI sender thread does the rest (`crate::ndi`).
use super::{db_to_lin, EngineShared, MAX_CALLBACK_FRAMES};
use crate::dsp::mixer::{Mixer, BUS_MONITOR};
use crate::dsp::{DspChain, DspParams, MetersPayload};
use crate::mixer_control::{MixerLink, MAX_STRIPS};
use crate::mixer_meters::MixerMeterSlots;
use crate::routing::{deinterleave, push_stereo};
use ringbuf::traits::Producer;
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

const SPECTRUM_FFT: usize = 256;
const SPECTRUM_BINS: usize = 128;

/// One opened output device: its ring producer, channel count and the bus
/// it plays.
pub(super) struct OutputRoute<P> {
    pub producer: P,
    pub channels: usize,
    pub bus: usize,
}

/// Visualiser spectrum of the voice strip (same maths as before the mixer).
struct Spectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buf: Vec<Complex32>,
    scratch: Vec<Complex32>,
    acc: Vec<f32>,
    count: usize,
}

impl Spectrum {
    fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(SPECTRUM_FFT);
        let window = (0..SPECTRUM_FFT)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (SPECTRUM_FFT - 1) as f32).cos())
            })
            .collect();
        let scratch_len = fft.get_inplace_scratch_len().max(1);
        Spectrum {
            fft,
            window,
            buf: vec![Complex32::default(); SPECTRUM_FFT],
            scratch: vec![Complex32::default(); scratch_len],
            acc: vec![0.0; SPECTRUM_BINS],
            count: 0,
        }
    }

    /// Accumulate the magnitude spectrum of the first 256 samples of
    /// `block` × `gain` (zero-padded; an empty block adds silence).
    fn accumulate(&mut self, block: &[f32], gain: f32) {
        for (i, bin) in self.buf.iter_mut().enumerate() {
            let sample = block.get(i).copied().unwrap_or(0.0) * gain;
            *bin = Complex32::new(sample * self.window[i], 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buf[..], &mut self.scratch[..]);
        for (acc, bin) in self.acc.iter_mut().zip(self.buf.iter().take(SPECTRUM_BINS)) {
            *acc += bin.norm();
        }
        self.count += 1;
    }

    /// Averaged, normalised (0..1) bins; resets the accumulator. Allocates.
    fn take_bins(&mut self) -> Vec<f32> {
        let count = self.count.max(1) as f32;
        let mut bins = Vec::with_capacity(self.acc.len());
        for &value in self.acc.iter() {
            let db = 20.0 * (value / count / SPECTRUM_BINS as f32).max(1e-9).log10();
            bins.push(((db + 90.0) / 90.0).clamp(0.0, 1.0));
        }
        self.acc.fill(0.0);
        self.count = 0;
        bins
    }
}

pub(super) struct InputProcessor<P> {
    /// Channels per interleaved input frame (the device's channel count).
    stride: usize,
    /// One buffer per strip (min(stride, MAX_STRIPS)), MAX_CALLBACK_FRAMES long.
    chan_bufs: Vec<Vec<f32>>,
    dsp: DspChain,
    params: Arc<DspParams>,
    shared: Arc<EngineShared>,
    voice_strip: Arc<AtomicUsize>,
    mixer: Mixer,
    /// Mixer meters for the meters thread (`mixer-meters`).
    meter_slots: Arc<MixerMeterSlots>,
    outputs: Vec<OutputRoute<P>>,
    /// NDI rings (2 channels each), read by the NDI sender threads.
    ndi_outputs: Vec<OutputRoute<P>>,
    spectrum: Spectrum,
    meters_tx: mpsc::SyncSender<MetersPayload>,
    meter_interval_samples: usize,
    samples_since_meter: usize,
}

impl<P: Producer<Item = f32>> InputProcessor<P> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sample_rate: u32,
        stride: usize,
        dsp: DspChain,
        params: Arc<DspParams>,
        shared: Arc<EngineShared>,
        mixer: MixerLink,
        meter_slots: Arc<MixerMeterSlots>,
        outputs: Vec<OutputRoute<P>>,
        ndi_outputs: Vec<OutputRoute<P>>,
        meters_tx: mpsc::SyncSender<MetersPayload>,
    ) -> Self {
        let stride = stride.max(1);
        let strips = stride.min(MAX_STRIPS);
        let chan_bufs = (0..strips)
            .map(|_| vec![0.0f32; MAX_CALLBACK_FRAMES])
            .collect();
        let engine_mixer = Mixer::new(
            sample_rate as f64,
            MAX_CALLBACK_FRAMES,
            strips,
            mixer.params.clone(),
        );
        let processor = InputProcessor {
            stride,
            chan_bufs,
            dsp,
            params,
            shared,
            voice_strip: mixer.voice_strip,
            mixer: engine_mixer,
            meter_slots,
            outputs,
            ndi_outputs,
            spectrum: Spectrum::new(),
            meters_tx,
            // Meters are sent ~50 times per second, as before.
            meter_interval_samples: (sample_rate as usize / 50).max(1),
            samples_since_meter: 0,
        };
        // Report a sensible latency before the first callback.
        processor.publish_path_latency();
        processor
    }

    /// Process one device callback of interleaved samples of any length,
    /// in chunks of at most `MAX_CALLBACK_FRAMES` frames.
    pub(super) fn process<T: Copy>(&mut self, data: &[T], convert: impl Fn(T) -> f32) {
        let started = Instant::now();
        let chunk_samples = self.stride * MAX_CALLBACK_FRAMES;
        for chunk in data.chunks(chunk_samples) {
            let frames = deinterleave(chunk, self.stride, &mut self.chan_bufs[..], &convert);
            if frames > 0 {
                self.process_frames(frames);
            }
        }
        self.shared.record_callback_time(started.elapsed());
    }

    fn process_frames(&mut self, n: usize) {
        let monitor_gain = db_to_lin(self.params.monitor_gain_db.load(Ordering::Relaxed));
        self.run_voice_chain(n, monitor_gain);

        // Mixer: strip i reads input channel i.
        let empty: &[f32] = &[];
        let mut inputs: [&[f32]; MAX_STRIPS] = [empty; MAX_STRIPS];
        for (slot, buf) in inputs.iter_mut().zip(self.chan_bufs.iter()) {
            *slot = &buf[..n];
        }
        let strips = self.chan_bufs.len();
        self.mixer.process(&inputs[..strips], n);
        self.publish_path_latency();
        self.meter_slots
            .publish(self.mixer.strip_meters(), &self.mixer.bus_meters()[..]);

        // Outputs: one stereo bus per device.
        let mut dropped = 0u64;
        for route in self.outputs.iter_mut() {
            let (left, right) = self.mixer.output(route.bus);
            let gain = if route.bus == BUS_MONITOR { monitor_gain } else { 1.0 };
            dropped += push_stereo(&mut route.producer, left, right, route.channels, gain);
        }
        // No logging on the audio thread: just count. The meters thread logs
        // the running total when it changes.
        if dropped > 0 {
            self.shared
                .dropped_output_samples
                .fetch_add(dropped, Ordering::Relaxed);
        }
        self.push_ndi();

        self.send_voice_meters(n);
    }

    /// Copy each NDI bus into its ring at unity gain. Drops what does not
    /// fit and only counts it: no logging, locking or allocation here.
    fn push_ndi(&mut self) {
        let mut dropped = 0u64;
        for route in self.ndi_outputs.iter_mut() {
            let (left, right) = self.mixer.output(route.bus);
            dropped += push_stereo(&mut route.producer, left, right, route.channels, 1.0);
        }
        if dropped > 0 {
            self.shared
                .ndi_dropped_samples
                .fetch_add(dropped, Ordering::Relaxed);
        }
    }

    /// Publish the latency of what is active now (see
    /// `EngineShared::path_latency_samples`).
    fn publish_path_latency(&self) {
        let voice = self.voice_strip.load(Ordering::Relaxed);
        let voice_latency = if voice < self.chan_bufs.len() {
            self.dsp.total_latency_samples()
        } else {
            0
        };
        let path = self.mixer.path_latency_samples(voice, voice_latency);
        self.shared
            .path_latency_samples
            .store(path as u64, Ordering::Relaxed);
    }

    /// Run the voice chain in place on the strip that holds `voice_chain`.
    /// If no strip holds it (`NO_VOICE_STRIP`) or that strip has no input
    /// channel on this device, the chain idles.
    fn run_voice_chain(&mut self, n: usize, monitor_gain: f32) {
        let voice = self.voice_strip.load(Ordering::Relaxed);
        let Some(channel) = self.chan_bufs.get_mut(voice) else {
            self.shared.dsp_latency_samples.store(0, Ordering::Relaxed);
            self.spectrum.accumulate(&[], monitor_gain);
            return;
        };
        let buf = &mut channel[..n];

        self.shared.push_recent_input(&*buf);
        self.shared.poll_noise_profile(&mut self.dsp);

        self.dsp.sync_params(&self.params);
        let bypass = self.params.bypass.load(Ordering::Relaxed);
        self.dsp.process_block(&mut *buf, bypass);
        self.shared
            .dsp_latency_samples
            .store(self.dsp.total_latency_samples() as u64, Ordering::Relaxed);

        // The spectrum shows what the old engine sent to the monitor.
        self.spectrum.accumulate(&*buf, monitor_gain);
    }

    /// Emit `audio-meters` data every ~20 ms. The spectrum Vec is the
    /// callback's only allocation and happens only here; the channel is
    /// drained every 20 ms, so a rejected try_send is not the common path.
    fn send_voice_meters(&mut self, n: usize) {
        self.samples_since_meter += n;
        if self.samples_since_meter < self.meter_interval_samples || self.spectrum.count == 0 {
            return;
        }
        self.samples_since_meter = 0;
        let spectrum = self.spectrum.take_bins();
        let dsp = &self.dsp;
        let _ = self.meters_tx.try_send(MetersPayload {
            input_db: dsp.last_input_db,
            output_db: dsp.last_output_db,
            gate_gain: dsp.last_gate_gain,
            lufs_m: dsp.last_lufs.momentary,
            lufs_st: dsp.last_lufs.short_term,
            lufs_i: dsp.last_lufs.integrated,
            deess_gr_db: dsp.last_deess_gr,
            auto_gain_db: dsp.last_auto_gain_db,
            neural_vad: dsp.neural_vad(),
            spectrum,
        });
    }
}
