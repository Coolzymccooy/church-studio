//! Mixer DSP core: mono input strips summed into five stereo buses
//! (Main = room/PA, Stream = livestream/record, Monitor = mon 1, the
//! engineer's stage/headphones, and Monitor 2 / Monitor 3 = mon 2 / mon 3,
//! extra pre-fader monitor mixes).
//!
//! Pure DSP. The engine (`audio::input_proc`) owns the `Mixer` on the audio
//! thread; the control side (`mixer_control`) owns the shared
//! `Arc<MixerParams>` (atomics only).
//!
//! API (allocation-free after `new`):
//!   `mixer.process(&inputs, frames)` where `inputs[i]` feeds strip `i`
//!   (missing or short inputs are treated as silence) and `frames` is clamped
//!   to `max_block`; returns the number of frames processed. The bus results
//!   are then read with `mixer.output(BUS_MAIN)` etc. as `(&left, &right)`
//!   slices of that length. Owning the bus buffers inside the mixer keeps the
//!   signature simple and avoids juggling six `&mut` slices per call.
//!
//! Latency: 5 ms bus limiter lookahead (always present) plus 20 ms on a strip
//! only while its gate is on (switched gate, zero latency when off). See
//! `latency_samples` / `path_latency_samples`.
//! Smoothing: all gains, pans, sends and mutes ramp linearly over 10 ms.
//! Automix and auto-ducking (`automix.rs`) add per-sample post-fader gains
//! and no latency. The bus loudness leveler (`leveler.rs`, off by default)
//! adds no latency either.
#![allow(dead_code, unused_imports)]

pub mod automix;
pub mod bus;
pub mod channel_proc;
mod fx;
pub mod fx_params;
mod fx_rack;
pub mod leveler;
pub mod params;
pub mod scene;
pub mod smooth;
pub mod strip;
pub mod groups;
mod group_publication;

#[cfg(test)]
mod automix_tests;
#[cfg(test)]
mod leveler_tests;
#[cfg(test)]
mod monitor_mix_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod split_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod fx_tests;

pub use automix::{AutoMix, AutoReadout};
pub use bus::{Bus, BusMeters};
pub use params::{
    BusParams, MixerParams, StripParams, BUS_COUNT, BUS_MAIN, BUS_MONITOR, BUS_MONITOR2,
    BUS_MONITOR3, BUS_STREAM, NUM_BUSES,
};
pub use scene::MixerScene;
pub use strip::{Strip, StripMeters};

use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

/// Preallocated stereo buffer for one bus.
pub struct StereoBuffer {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl StereoBuffer {
    fn new(len: usize) -> Self {
        StereoBuffer {
            left: vec![0.0; len],
            right: vec![0.0; len],
        }
    }

    fn clear(&mut self, frames: usize) {
        for v in self.left[..frames].iter_mut() {
            *v = 0.0;
        }
        for v in self.right[..frames].iter_mut() {
            *v = 0.0;
        }
    }
}

/// Snapshot of every meter (allocates — take it on the UI side, e.g. every
/// few blocks, or use `strip_meters()` / `bus_meters()` which borrow).
#[derive(Clone, Debug, serde::Serialize)]
pub struct MixerMeters {
    pub strips: Vec<StripMeters>,
    pub buses: Vec<BusMeters>,
}

pub struct Mixer {
    params: Arc<MixerParams>,
    strips: Vec<Strip>,
    automix: AutoMix,
    group_snapshot: group_publication::GroupSnapshot,
    dca_peaks: [f32; groups::GROUP_COUNT],
    strip_meters: Vec<StripMeters>,
    buses: [Bus; NUM_BUSES],
    bus_meters: [BusMeters; NUM_BUSES],
    bufs: [StereoBuffer; NUM_BUSES],
    fx: fx_rack::FxRack,
    max_block: usize,
    last_frames: usize,
    primed: bool,
}

impl Mixer {
    /// `num_strips` is capped at `params.strips.len()`.
    pub fn new(
        sample_rate: f64,
        max_block: usize,
        num_strips: usize,
        params: Arc<MixerParams>,
    ) -> Self {
        let max_block = max_block.max(1);
        let ramp = smooth::ramp_samples(sample_rate);
        let num_strips = num_strips.min(params.strips.len());
        let strips: Vec<Strip> = (0..num_strips)
            .map(|_| Strip::new(sample_rate, max_block, ramp))
            .collect();
        let strip_meters: Vec<StripMeters> = strips.iter().map(|s| s.meters()).collect();
        let automix = AutoMix::new(sample_rate, max_block, num_strips);
        let buses: [Bus; NUM_BUSES] = std::array::from_fn(|_| Bus::new(sample_rate, ramp));
        let bus_meters = std::array::from_fn(|b| buses[b].meters());

        Mixer {
            params,
            strips,
            automix,
            group_snapshot: group_publication::GroupSnapshot::default(),
            dca_peaks: [-120.0; groups::GROUP_COUNT],
            strip_meters,
            buses,
            bus_meters,
            bufs: std::array::from_fn(|_| StereoBuffer::new(max_block)),
            fx: fx_rack::FxRack::new(sample_rate as f32, max_block, ramp),
            max_block,
            last_frames: 0,
            primed: false,
        }
    }

    pub fn num_strips(&self) -> usize {
        self.strips.len()
    }

    pub fn max_block(&self) -> usize {
        self.max_block
    }

    /// Bus latency in samples (limiter lookahead; the same on every bus).
    pub fn bus_latency_samples(&self) -> usize {
        self.buses[0].latency_samples()
    }

    /// Input-to-bus latency in samples for what is active right now: the
    /// slowest strip (strips run in parallel) plus the bus limiter.
    /// `insert_latency` is added to strip `insert_strip` for an external
    /// insert run before the mixer (the voice chain); pass 0 for none.
    pub fn path_latency_samples(&self, insert_strip: usize, insert_latency: usize) -> usize {
        let slowest_strip = self
            .strips
            .iter()
            .enumerate()
            .map(|(i, strip)| {
                let insert = if i == insert_strip { insert_latency } else { 0 };
                strip.latency_samples() + insert
            })
            .max()
            .unwrap_or(0);
        slowest_strip + self.bus_latency_samples()
    }

    /// Process one block. See the module docs for the contract.
    pub fn process(&mut self, inputs: &[&[f32]], frames: usize) -> usize {
        self.process_split(inputs, None, frames)
    }

    /// `process` with a Stream-only signal for one strip: `(strip, signal)`.
    /// The voice strip uses it so Main and the monitors hear its live (fast)
    /// path while Stream hears the full voice chain.
    pub fn process_split(
        &mut self,
        inputs: &[&[f32]],
        stream_alt: Option<(usize, &[f32])>,
        frames: usize,
    ) -> usize {
        let n = frames.min(self.max_block);
        let snap = !self.primed;
        self.primed = true;

        for buf in self.bufs.iter_mut() {
            buf.clear(n);
        }

        let params = &*self.params;
        self.fx.clear(n);
        let any_solo = params.any_solo(self.strips.len())
            || params.fx.returns.iter().any(|ret| ret.solo.load(Relaxed));
        let empty: &[f32] = &[];

        // Pass 1: every strip up to the pre-fader point.
        params.groups.refresh_audio(&mut self.group_snapshot);
        let groups = &self.group_snapshot;
        self.dca_peaks.fill(-120.0);
        for (i, strip) in self.strips.iter_mut().enumerate() {
            let input = inputs.get(i).copied().unwrap_or(empty);
            let params = &params.strips[i];
            strip.set_group_gain(groups.state.strip_gain(groups.members[i]));
            let alt = stream_alt.and_then(|(s, sig)| (s == i).then_some(sig));
            strip.process_pre_split(input, alt, n, params, any_solo, snap);
        }

        // Automix and ducking gains across the strips, sample by sample.
        let num = self.strips.len();
        let strips = &self.strips;
        self.automix
            .process(&params.strips[..num], n, |i| strips[i].detection(), groups);

        // Pass 2: fader × automix × duck, pan, sends → buses.
        for (i, strip) in self.strips.iter_mut().enumerate() {
            strip.process_out(
                self.automix.gains(i, n),
                &mut self.bufs,
                Some(&mut self.fx.sends),
            );
            let auto = self.automix.readout(i);
            strip.set_auto_meters(auto.automix_gr_db, auto.duck_gr_db, auto.speech);
            self.strip_meters[i] = strip.meters();
            let member = groups.members[i];
            if member.dca != 0 && !groups.state.muted(member) && !params.strips[i].mute.load(Relaxed) {
                for (dca, peak) in self.dca_peaks.iter_mut().enumerate() {
                    if member.dca & (1 << dca) != 0 {
                        *peak = peak.max(self.strip_meters[i].post_fader_peak_db);
                    }
                }
            }
            let live = &params.strips[i].live;
            live.automix_gr_db.store(-auto.automix_gr_db, Relaxed);
            live.duck_gr_db.store(-auto.duck_gr_db, Relaxed);
        }

        self.fx.process(&mut self.bufs, n, &params.fx, any_solo, snap);

        for b in 0..NUM_BUSES {
            self.buses[b].process(&mut self.bufs[b], n, &params.buses[b], snap);
            self.bus_meters[b] = self.buses[b].meters();
        }

        self.last_frames = n;
        n
    }

    /// Bus output of the last `process` call: (left, right).
    /// Returns empty slices for an invalid bus index.
    pub fn output(&self, bus: usize) -> (&[f32], &[f32]) {
        match self.bufs.get(bus) {
            Some(buf) => (&buf.left[..self.last_frames], &buf.right[..self.last_frames]),
            None => {
                let empty: &[f32] = &[];
                (empty, empty)
            }
        }
    }

    /// Test hook: force strip `index`'s speech probability (`None` = the
    /// RNNoise VAD at 48 kHz, or level only at other rates).
    pub fn set_vad_override(&mut self, index: usize, prob: Option<f32>) {
        self.automix.set_vad_override(index, prob);
    }

    /// True once the first block has warmed the VAD FFT on this thread.
    pub fn automix_warmed(&self) -> bool {
        self.automix.warmed()
    }

    pub fn strip_meters(&self) -> &[StripMeters] {
        &self.strip_meters
    }

    pub fn bus_meters(&self) -> &[BusMeters; NUM_BUSES] {
        &self.bus_meters
    }

    pub fn fx_peaks(&self) -> [[f32; 2]; fx_params::FX_COUNT] {
        self.fx.peaks()
    }
    pub fn dca_peaks(&self) -> &[f32; groups::GROUP_COUNT] { &self.dca_peaks }

    pub fn meters(&self) -> MixerMeters {
        MixerMeters {
            strips: self.strip_meters.clone(),
            buses: self.bus_meters.to_vec(),
        }
    }
}
