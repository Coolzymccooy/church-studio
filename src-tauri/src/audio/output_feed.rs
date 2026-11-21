//! What an output device callback plays (`docs/specs/2026-11-20-usb-interfaces.md`
//! §4 and §5).
//!
//! The input callback pushes one ring per output device; each ring frame is
//! (L, R) for every bus on that device. The output callback reads whole
//! frames only (a short ring can never swap channels) and maps each bus to
//! its pair on the device; every other channel is silent.
//!
//! - `ClockMode::Shared`: the device is the input device, so both run on one
//!   clock. Frames are read directly, as before.
//! - `ClockMode::Corrected`: another device, with its own crystal. A small
//!   buffer (3 engine blocks) is held and read at a ratio nudged by at most
//!   ±0.1 % with linear interpolation, the way NDI inputs are
//!   (`crate::ndi::receive::drift`).
//!
//! Real-time safe: `fill` only does ring operations and arithmetic.
use crate::ndi::receive::drift::{frames_for_ms, DriftController, MAX_NUDGE};
use ringbuf::traits::Consumer;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;

/// Most samples in one ring frame: five buses × (L, R).
pub const MAX_WIDTH: usize = 10;
/// A corrected feed holds this many engine blocks.
pub const TARGET_BLOCKS: usize = 3;
/// Above target + this, a corrected feed trims back to the target.
const OVERRUN_MARGIN_MS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMode {
    Shared,
    Corrected,
}

impl ClockMode {
    pub fn word(self) -> &'static str {
        match self {
            ClockMode::Shared => "shared",
            ClockMode::Corrected => "corrected",
        }
    }
}

/// Counters the status reads (control thread) while the callback writes.
#[derive(Debug, Default)]
pub struct OutputStats {
    underruns: AtomicU64,
    drift_ppm: AtomicI32,
}

impl OutputStats {
    /// Blocks that ran short after the feed had started playing.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// The current read-ratio nudge in parts per million (0 when shared).
    pub fn drift_ppm(&self) -> i32 {
        self.drift_ppm.load(Ordering::Relaxed)
    }
}

/// The extra delay a feed adds before its device (ms): none on the input's
/// clock, the held buffer (`TARGET_BLOCKS` engine blocks) on its own clock.
/// Reported so lip-sync downstream can account for it.
pub fn clock_buffer_ms(mode: ClockMode, block_frames: usize, sample_rate: u32) -> f32 {
    match mode {
        ClockMode::Shared => 0.0,
        ClockMode::Corrected if sample_rate > 0 => {
            (block_frames * TARGET_BLOCKS) as f32 * 1000.0 / sample_rate as f32
        }
        ClockMode::Corrected => 0.0,
    }
}

/// Whether pair `pair` (0 = 1-2) fits a `channels`-channel device. A mono
/// device takes pair 0 only.
pub fn pair_fits(pair: usize, channels: usize) -> bool {
    if channels <= 1 {
        return pair == 0;
    }
    pair * 2 + 1 < channels
}

/// Map one ring frame (`src`: L, R per member) onto a device frame. `map`
/// gives each member's first channel, or `None` when its pair does not fit.
/// A 1-channel device gets (L + R) / 2 of every member that fits.
pub fn map_frame(src: &[f32], map: &[Option<usize>], channels: usize, out: &mut [f32]) {
    out.fill(0.0);
    for (member, first) in map.iter().enumerate() {
        let (Some(first), Some(lr)) = (first, src.get(member * 2..member * 2 + 2)) else {
            continue;
        };
        if channels <= 1 {
            if let Some(mono) = out.first_mut() {
                *mono += (lr[0] + lr[1]) * 0.5;
            }
        } else if let Some(pair) = out.get_mut(*first..*first + 2) {
            pair[0] = lr[0];
            pair[1] = lr[1];
        }
    }
}

fn pop_frame<C: Consumer<Item = f32>>(ring: &mut C, width: usize, frame: &mut [f32; MAX_WIDTH]) -> bool {
    if ring.occupied_len() < width {
        return false;
    }
    ring.pop_slice(&mut frame[..width]) == width
}

/// A drift-corrected reader for frames of `width` samples.
struct WideDrift {
    controller: DriftController,
    width: usize,
    target: usize,
    high: usize,
    buffering: bool,
    current: [f32; MAX_WIDTH],
    next: [f32; MAX_WIDTH],
    frac: f64,
}

impl WideDrift {
    fn new(width: usize, target: usize, high: usize) -> Self {
        let target = target.max(2);
        WideDrift {
            controller: DriftController::with_nudge(target, MAX_NUDGE),
            width,
            target,
            high: high.max(target + 1),
            buffering: true,
            current: [0.0; MAX_WIDTH],
            next: [0.0; MAX_WIDTH],
            frac: 0.0,
        }
    }

    fn restart(&mut self) {
        self.buffering = true;
        self.frac = 0.0;
    }

    /// Read `frames` frames, handing each to `write`. Returns (underrun,
    /// ratio used).
    fn read<C: Consumer<Item = f32>>(
        &mut self,
        ring: &mut C,
        frames: usize,
        mut write: impl FnMut(usize, &[f32]),
    ) -> (bool, f64) {
        let silent = [0.0f32; MAX_WIDTH];
        let w = self.width;
        let mut fill = ring.occupied_len() / w;
        if fill > self.high {
            let skipped = ring.skip((fill - self.target) * w);
            fill -= skipped / w;
            self.controller.reset(fill);
        }
        if self.buffering {
            if fill < self.target
                || !pop_frame(ring, w, &mut self.current)
                || !pop_frame(ring, w, &mut self.next)
            {
                (0..frames).for_each(|i| write(i, &silent[..w]));
                return (false, 1.0);
            }
            self.buffering = false;
            self.frac = 0.0;
            self.controller.reset(fill);
        }
        let ratio = self.controller.update(fill);
        let needed = (self.frac + frames as f64 * ratio).ceil() as usize;
        if ring.occupied_len() / w < needed {
            (0..frames).for_each(|i| write(i, &silent[..w]));
            self.buffering = true;
            return (true, ratio);
        }
        let mut frame = [0.0f32; MAX_WIDTH];
        for index in 0..frames {
            let t = self.frac as f32;
            for k in 0..w {
                frame[k] = self.current[k] + (self.next[k] - self.current[k]) * t;
            }
            write(index, &frame[..w]);
            self.frac += ratio;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                self.current = self.next;
                if !pop_frame(ring, w, &mut self.next) {
                    (index + 1..frames).for_each(|i| write(i, &silent[..w]));
                    self.buffering = true;
                    return (true, ratio);
                }
            }
        }
        (false, ratio)
    }
}

/// One output device's feed: its ring, the pair of each bus on it, and the
/// clock mode. Lives in an `Arc<Mutex<_>>` so a reopened stream (after a
/// re-plug) continues from the same ring.
pub struct OutputFeed<C> {
    ring: C,
    width: usize,
    pairs: Vec<usize>,
    map: Vec<Option<usize>>,
    channels: usize,
    dev_frame: Vec<f32>,
    drift: Option<WideDrift>,
    stats: Arc<OutputStats>,
    primed: bool,
}

impl<C: Consumer<Item = f32>> OutputFeed<C> {
    /// `pairs`: one per bus on the device, in ring order. `block_frames`:
    /// the engine block (the corrected buffer holds `TARGET_BLOCKS` of them).
    pub fn new(
        ring: C,
        pairs: Vec<usize>,
        mode: ClockMode,
        sample_rate: u32,
        block_frames: usize,
        stats: Arc<OutputStats>,
    ) -> Self {
        let width = (pairs.len() * 2).clamp(2, MAX_WIDTH);
        let drift = match mode {
            ClockMode::Shared => None,
            ClockMode::Corrected => {
                let target = block_frames.max(1) * TARGET_BLOCKS;
                let high = target + frames_for_ms(sample_rate, OVERRUN_MARGIN_MS);
                Some(WideDrift::new(width, target, high))
            }
        };
        let mut feed = OutputFeed {
            ring,
            width,
            pairs,
            map: Vec::new(),
            channels: 2,
            dev_frame: vec![0.0; 2],
            drift,
            stats,
            primed: false,
        };
        feed.set_device_channels(2);
        feed
    }

    /// Set the opened device's channel count (control thread, while no
    /// stream plays this feed). Returns the members whose pair does not fit.
    pub fn set_device_channels(&mut self, channels: usize) -> Vec<usize> {
        self.channels = channels.max(1);
        self.dev_frame = vec![0.0; self.channels];
        self.map = self
            .pairs
            .iter()
            .map(|&pair| pair_fits(pair, self.channels).then_some(pair * 2))
            .collect();
        self.map
            .iter()
            .enumerate()
            .filter(|(_, first)| first.is_none())
            .map(|(member, _)| member)
            .collect()
    }

    /// Drop what is buffered (it is stale after a re-plug) and start again.
    pub fn restart(&mut self) {
        let whole = self.ring.occupied_len() / self.width * self.width;
        self.ring.skip(whole);
        if let Some(drift) = self.drift.as_mut() {
            drift.restart();
        }
        self.primed = false;
    }

    /// Fill one interleaved device buffer.
    pub fn fill<T>(&mut self, data: &mut [T], convert: impl Fn(f32) -> T) {
        let channels = self.channels;
        let frames = data.len() / channels;
        let Self { ring, width, map, dev_frame, drift, stats, primed, .. } = self;
        let width = *width;
        let mut write = |index: usize, src: &[f32]| {
            map_frame(src, map, channels, dev_frame);
            let out = &mut data[index * channels..(index + 1) * channels];
            for (slot, &sample) in out.iter_mut().zip(dev_frame.iter()) {
                *slot = convert(sample);
            }
        };
        let underrun = match drift.as_mut() {
            Some(drift) => {
                let (underrun, ratio) = drift.read(ring, frames, &mut write);
                stats
                    .drift_ppm
                    .store(((ratio - 1.0) * 1e6).round() as i32, Ordering::Relaxed);
                *primed |= !drift.buffering || underrun;
                underrun
            }
            None => {
                let mut src = [0.0f32; MAX_WIDTH];
                let silent = [0.0f32; MAX_WIDTH];
                let mut short = false;
                for index in 0..frames {
                    if pop_frame(ring, width, &mut src) {
                        *primed = true;
                        write(index, &src[..width]);
                    } else {
                        short |= *primed;
                        write(index, &silent[..width]);
                    }
                }
                short
            }
        };
        if underrun {
            stats.underruns.fetch_add(1, Ordering::Relaxed);
        }
        // Samples past the last whole frame (never expected) stay silent.
        for slot in data[frames * channels..].iter_mut() {
            *slot = convert(0.0);
        }
    }
}
