//! Everything the input callback does, as one Send struct that the cpal
//! closure owns. Allocation-free and lock-free per block (the only
//! allocation is the 128-bin spectrum Vec built ~50×/s for `audio-meters`,
//! exactly as before the mixer); all buffers are sized in `new`.
//!
//! Per chunk of at most `MAX_CALLBACK_FRAMES` frames:
//! 1. De-interleave the chosen input channels (up to `MAX_STRIPS`) into
//!    their own buffers; strip i reads input channel `first + i`. No mono
//!    fold. The Remote strips
//!    of network audio come next, then computer audio fills two strips
//!    (left, right), and each NDI input the strip after those, all read from
//!    their rings with drift correction (`crate::netaudio`,
//!    `crate::computer_audio`, `crate::ndi::receive`).
//! 2. The strip holding `voice_chain` runs the `DspChain` in place on its
//!    buffer (history for noise-profile capture, noise-profile install,
//!    params sync, spectrum and `audio-meters` all follow that strip).
//! 3. `Mixer::process` sums the strips into Main, Stream, Monitor (mon 1),
//!    Monitor 2 and Monitor 3; the strip and bus meters are published to
//!    lock-free slots.
//! 4. Each output device gets one ring with (L, R) of every bus on it (see
//!    `routing::push_bus_frames`); the output callback puts each bus on its
//!    pair. The Monitor (mon 1) bus alone is still scaled by
//!    `monitor_gain_db`.
//! 5. Each NDI output gets its bus as interleaved stereo at unity gain, in
//!    its own ring; the NDI sender thread does the rest (`crate::ndi`). The
//!    network send gets its bus the same way (`crate::netaudio::sender`).
//! 6. While a recording is active, the raw strips (copied before the voice
//!    chain works on its strip) plus Stream and Main go to the record ring
//!    as whole frames (`crate::recorder::tap`).
use super::health::{LinkState, StreamHealth};
use super::{db_to_lin, EngineShared, MAX_CALLBACK_FRAMES};
use crate::computer_audio::ComputerAudioFeed;
use crate::dsp::mixer::{Mixer, BUS_MAIN, BUS_MONITOR, BUS_STREAM};
use crate::mixer_layout::COMPUTER_STRIPS;
use crate::dsp::{DspChain, DspParams, MetersPayload};
use crate::mixer_control::{MixerLink, MAX_STRIPS};
use crate::mixer_meters::MixerMeterSlots;
use crate::ndi::receive::NdiInputFeed;
use crate::netaudio::{NetInputFeed, NetSendRoute};
use crate::recorder::tap::RecordTap;
use crate::routing::{deinterleave_from, push_bus_frames, push_stereo, BusBlock};
use ringbuf::traits::Producer;
use ringbuf::HeapCons;
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

const SPECTRUM_FFT: usize = 256;
const SPECTRUM_BINS: usize = 128;

/// One NDI output: its ring producer, channel count and the bus it plays.
pub(super) struct OutputRoute<P> {
    pub producer: P,
    pub channels: usize,
    pub bus: usize,
}

/// Most buses one output device can carry (every bus on one interface).
pub(super) const MAX_DEVICE_BUSES: usize = 5;

/// One output device: its ring producer and the buses on it, in ring order.
/// Each ring frame is (L, R) per bus; the output callback puts each bus on
/// its pair (`super::output_feed`).
pub(super) struct DeviceRoute<P> {
    pub producer: P,
    pub buses: Vec<usize>,
    /// Only a device that is playing is fed: a waiting or lost one would
    /// fill its ring and count every block as dropped.
    pub health: Arc<StreamHealth>,
}

/// Which input channels become strips: strip i reads channel `first + i`
/// of `stride`-channel frames, for `count` strips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HardwareInput {
    pub stride: usize,
    pub first: usize,
    pub count: usize,
}

impl HardwareInput {
    /// Every channel of a `stride`-channel device.
    #[cfg(test)]
    pub fn all(stride: usize) -> Self {
        HardwareInput { stride, first: 0, count: stride }
    }
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

/// Network audio for the callback: the Remote strips' feeds and the send
/// ring.
pub(super) struct NetIo<P> {
    pub inputs: Vec<NetInputFeed<HeapCons<f32>>>,
    pub send: Option<NetSendRoute<P>>,
}

pub(super) struct InputProcessor<P> {
    /// Channels per interleaved input frame (the device's channel count).
    stride: usize,
    /// The first device channel read (strip 0).
    first: usize,
    /// Strips fed by input channels: min(count, MAX_STRIPS).
    hardware: usize,
    /// One buffer per strip (hardware, Remote, computer, then NDI),
    /// MAX_CALLBACK_FRAMES long.
    chan_bufs: Vec<Vec<f32>>,
    /// Network audio peers, feeding the strips right after the hardware ones.
    net_inputs: Vec<NetInputFeed<HeapCons<f32>>>,
    /// The network send ring (interleaved stereo of one bus).
    net_send: Option<NetSendRoute<P>>,
    /// Computer audio, feeding the two strips after the Remote ones.
    computer: Option<ComputerAudioFeed<HeapCons<f32>>>,
    /// NDI inputs, feeding the strips after the hardware and computer ones.
    ndi_inputs: Vec<NdiInputFeed<HeapCons<f32>>>,
    dsp: DspChain,
    params: Arc<DspParams>,
    shared: Arc<EngineShared>,
    voice_strip: Arc<AtomicUsize>,
    mixer: Mixer,
    /// Mixer meters for the meters thread (`mixer-meters`).
    meter_slots: Arc<MixerMeterSlots>,
    outputs: Vec<DeviceRoute<P>>,
    /// NDI rings (2 channels each), read by the NDI sender threads.
    ndi_outputs: Vec<OutputRoute<P>>,
    /// Multitrack recording tap (idle unless a recording is active).
    recorder: RecordTap<P>,
    spectrum: Spectrum,
    meters_tx: mpsc::SyncSender<MetersPayload>,
    meter_interval_samples: usize,
    samples_since_meter: usize,
}

impl<P: Producer<Item = f32>> InputProcessor<P> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        sample_rate: u32,
        input: HardwareInput,
        dsp: DspChain,
        params: Arc<DspParams>,
        shared: Arc<EngineShared>,
        mixer: MixerLink,
        meter_slots: Arc<MixerMeterSlots>,
        outputs: Vec<DeviceRoute<P>>,
        ndi_outputs: Vec<OutputRoute<P>>,
        computer: Option<ComputerAudioFeed<HeapCons<f32>>>,
        mut ndi_inputs: Vec<NdiInputFeed<HeapCons<f32>>>,
        net: NetIo<P>,
        recorder: RecordTap<P>,
        meters_tx: mpsc::SyncSender<MetersPayload>,
    ) -> Self {
        let stride = input.stride.max(1);
        let first = input.first.min(stride - 1);
        let hardware = input.count.clamp(1, stride - first).min(MAX_STRIPS);
        let computer = computer.filter(|_| MAX_STRIPS - hardware >= COMPUTER_STRIPS);
        let computer_strips = if computer.is_some() { COMPUTER_STRIPS } else { 0 };
        let mut net_inputs = net.inputs;
        net_inputs.truncate(MAX_STRIPS - hardware - computer_strips);
        ndi_inputs.truncate(MAX_STRIPS - hardware - computer_strips - net_inputs.len());
        let strips = hardware + net_inputs.len() + computer_strips + ndi_inputs.len();
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
            first,
            hardware,
            chan_bufs,
            net_inputs,
            net_send: net.send,
            computer,
            ndi_inputs,
            dsp,
            params,
            shared,
            voice_strip: mixer.voice_strip,
            mixer: engine_mixer,
            meter_slots,
            outputs,
            ndi_outputs,
            recorder,
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
            let frames = deinterleave_from(
                chunk,
                self.stride,
                self.first,
                &mut self.chan_bufs[..self.hardware],
                &convert,
            );
            if frames > 0 {
                self.process_frames(frames);
            }
        }
        self.shared.record_callback_time(started.elapsed());
    }

    /// The reopened input device's channel count (control thread, while no
    /// stream plays this processor). `Err` when the chosen channels are no
    /// longer all on it.
    pub(super) fn set_stride(&mut self, stride: usize) -> Result<(), usize> {
        let needed = self.first + self.hardware;
        if stride < needed {
            return Err(needed);
        }
        self.stride = stride;
        Ok(())
    }

    fn process_frames(&mut self, n: usize) {
        let monitor_gain = db_to_lin(self.params.monitor_gain_db.load(Ordering::Relaxed));
        self.pull_net_inputs(n);
        self.pull_computer(n);
        self.pull_ndi_inputs(n);
        // Raw strips are recorded before the voice chain works in place.
        let voice = self.voice_strip.load(Ordering::Relaxed);
        self.recorder.capture_raw(&self.chan_bufs, voice, n);
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

        // Outputs: one ring per device, (L, R) per bus on it.
        let mut dropped = 0u64;
        let empty: &[f32] = &[];
        for route in self.outputs.iter_mut() {
            if route.health.state() != LinkState::Ok {
                continue;
            }
            let mut blocks: [BusBlock<'_>; MAX_DEVICE_BUSES] = [(empty, empty, 0.0); MAX_DEVICE_BUSES];
            let count = route.buses.len().min(MAX_DEVICE_BUSES);
            for (block, &bus) in blocks.iter_mut().zip(route.buses.iter()) {
                let (left, right) = self.mixer.output(bus);
                let gain = if bus == BUS_MONITOR { monitor_gain } else { 1.0 };
                *block = (left, right, gain);
            }
            dropped += push_bus_frames(&mut route.producer, &blocks[..count], n);
        }
        // No logging on the audio thread: just count. The meters thread logs
        // the running total when it changes.
        if dropped > 0 {
            self.shared
                .dropped_output_samples
                .fetch_add(dropped, Ordering::Relaxed);
        }
        self.push_ndi();
        self.push_net();
        self.push_record(n);

        self.send_voice_meters(n);
    }

    /// Fill the computer-audio pair from its ring (silence while it
    /// buffers). Lock- and allocation-free.
    fn pull_computer(&mut self, n: usize) {
        let Some(feed) = self.computer.as_mut() else {
            return;
        };
        let first = self.hardware + self.net_inputs.len();
        let (left, right) = self.chan_bufs[first..].split_at_mut(1);
        feed.pull(&mut left[0][..n], &mut right[0][..n]);
    }

    /// Fill each Remote strip's buffer from its peer ring (silence while it
    /// buffers or no peer holds it). Lock- and allocation-free.
    fn pull_net_inputs(&mut self, n: usize) {
        let net_bufs = &mut self.chan_bufs[self.hardware..];
        for (feed, buf) in self.net_inputs.iter_mut().zip(net_bufs.iter_mut()) {
            feed.pull(&mut buf[..n]);
        }
    }

    /// First strip after the hardware, Remote and computer ones.
    fn first_ndi_strip(&self) -> usize {
        self.hardware
            + self.net_inputs.len()
            + if self.computer.is_some() { COMPUTER_STRIPS } else { 0 }
    }

    /// Fill each NDI strip's buffer from its input ring (silence while it
    /// buffers). Lock- and allocation-free.
    fn pull_ndi_inputs(&mut self, n: usize) {
        let first = self.first_ndi_strip();
        let ndi_bufs = &mut self.chan_bufs[first..];
        for (feed, buf) in self.ndi_inputs.iter_mut().zip(ndi_bufs.iter_mut()) {
            feed.pull(&mut buf[..n]);
        }
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

    /// Copy the network send bus into its ring at unity gain, counting (not
    /// logging) what does not fit.
    fn push_net(&mut self) {
        let Some(route) = self.net_send.as_mut() else {
            return;
        };
        let (left, right) = self.mixer.output(route.bus);
        let dropped = push_stereo(&mut route.producer, left, right, 2, 1.0);
        if dropped > 0 {
            route.stats.dropped_samples.fetch_add(dropped, Ordering::Relaxed);
        }
    }

    /// Push this block to the record ring (a no-op unless recording).
    fn push_record(&mut self, n: usize) {
        let stream = self.mixer.output(BUS_STREAM);
        let main = self.mixer.output(BUS_MAIN);
        self.recorder.push(&self.chan_bufs, stream, main, n);
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
