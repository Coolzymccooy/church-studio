//! Mixer DSP core: mono input strips summed into three stereo buses
//! (Main = room/PA, Stream = livestream/record, Monitor = stage/headphones).
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
#![allow(dead_code, unused_imports)]

pub mod bus;
pub mod params;
pub mod scene;
pub mod smooth;
pub mod strip;

#[cfg(test)]
mod tests;

pub use bus::{Bus, BusMeters};
pub use params::{
    BusParams, MixerParams, StripParams, BUS_MAIN, BUS_MONITOR, BUS_STREAM, NUM_BUSES,
};
pub use scene::MixerScene;
pub use strip::{Strip, StripMeters};

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
    strip_meters: Vec<StripMeters>,
    buses: [Bus; NUM_BUSES],
    bus_meters: [BusMeters; NUM_BUSES],
    bufs: [StereoBuffer; NUM_BUSES],
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
        let buses = [
            Bus::new(sample_rate, ramp),
            Bus::new(sample_rate, ramp),
            Bus::new(sample_rate, ramp),
        ];
        let bus_meters = [buses[0].meters(), buses[1].meters(), buses[2].meters()];

        Mixer {
            params,
            strips,
            strip_meters,
            buses,
            bus_meters,
            bufs: [
                StereoBuffer::new(max_block),
                StereoBuffer::new(max_block),
                StereoBuffer::new(max_block),
            ],
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
        let n = frames.min(self.max_block);
        let snap = !self.primed;
        self.primed = true;

        for buf in self.bufs.iter_mut() {
            buf.clear(n);
        }

        let params = &*self.params;
        let any_solo = params.any_solo();
        let empty: &[f32] = &[];

        for (i, strip) in self.strips.iter_mut().enumerate() {
            let input = inputs.get(i).copied().unwrap_or(empty);
            strip.process(input, n, &params.strips[i], any_solo, snap, &mut self.bufs);
            self.strip_meters[i] = strip.meters();
        }

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

    pub fn strip_meters(&self) -> &[StripMeters] {
        &self.strip_meters
    }

    pub fn bus_meters(&self) -> &[BusMeters; NUM_BUSES] {
        &self.bus_meters
    }

    pub fn meters(&self) -> MixerMeters {
        MixerMeters {
            strips: self.strip_meters.clone(),
            buses: self.bus_meters.to_vec(),
        }
    }
}
