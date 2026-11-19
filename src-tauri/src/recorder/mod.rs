//! Multitrack service recording (issue #22, design in
//! `docs/specs/2026-10-01-multitrack-recording-design.md`).
//!
//! One WAV per mixer strip (raw: after input/NDI receive, before the voice
//! chain, EQ and fader), plus the Stream mix and optionally the Main mix.
//!
//! - `tap`: the audio-callback side; pushes whole frames into the ring.
//! - `writer`: the writer thread; de-interleaves and writes the files.
//! - `engine`: the control side kept in `RunningEngine` (start, stop,
//!   markers, status).
//! - `wav`, `names`, `session`, `frames`, `plan`: pure helpers.
//! - `config`: `recorder.json` in app data.
pub mod config;
pub mod engine;
pub mod frames;
pub mod names;
pub mod plan;
pub mod session;
pub mod status;
pub mod tap;
pub mod wav;
pub mod writer;
#[cfg(test)]
mod writer_tests;

use frames::RecordLayout;
use parking_lot::Mutex;
use ringbuf::traits::Split;
use ringbuf::{HeapCons, HeapProd, HeapRb};
use session::Marker;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tap::RecordTap;

/// Seconds of audio the record ring holds.
pub const RING_SECONDS: usize = 4;

/// Shared by the audio callback (`RecordTap`), the writer thread and the
/// control thread for the life of one engine.
pub struct RecordShared {
    /// Set while a recording is capturing; the callback pushes only then.
    pub active: AtomicBool,
    /// Frames seen by the callback since the recording started (pushed or
    /// dropped): the recording clock for elapsed time and markers.
    pub frames_captured: AtomicU64,
    /// Whole frames the ring could not take.
    pub dropped_frames: AtomicU64,
}

impl RecordShared {
    pub fn new() -> Self {
        RecordShared {
            active: AtomicBool::new(false),
            frames_captured: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
        }
    }

    pub fn reset_counters(&self) {
        self.frames_captured.store(0, Ordering::Relaxed);
        self.dropped_frames.store(0, Ordering::Relaxed);
    }
}

impl Default for RecordShared {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-recording state shared by the control thread and the writer.
pub struct SessionControl {
    /// Set by the control thread to end the recording.
    pub stop: AtomicBool,
    markers: Mutex<Vec<Marker>>,
}

impl SessionControl {
    pub fn new() -> Self {
        SessionControl {
            stop: AtomicBool::new(false),
            markers: Mutex::new(Vec::new()),
        }
    }

    pub fn add_marker(&self, marker: Marker) {
        self.markers.lock().push(marker);
    }

    pub fn markers_snapshot(&self) -> Vec<Marker> {
        self.markers.lock().clone()
    }

    pub fn marker_count(&self) -> usize {
        self.markers.lock().len()
    }
}

impl Default for SessionControl {
    fn default() -> Self {
        Self::new()
    }
}

/// The control side of the record ring, built at engine start.
pub struct RecordPort {
    pub consumer: HeapCons<f32>,
    pub shared: Arc<RecordShared>,
    pub layout: RecordLayout,
    pub sample_rate: u32,
}

/// Preallocate the ring (`RING_SECONDS` of every record channel) and split
/// it into the callback's tap and the control side's port.
pub fn record_ring(
    strips: usize,
    sample_rate: u32,
    max_block_frames: usize,
) -> (RecordTap<HeapProd<f32>>, RecordPort) {
    let layout = RecordLayout { strips };
    let shared = Arc::new(RecordShared::new());
    let capacity = layout.ring_samples(sample_rate, RING_SECONDS);
    let (producer, consumer) = HeapRb::<f32>::new(capacity).split();
    let tap = RecordTap::new(producer, shared.clone(), layout, max_block_frames);
    let port = RecordPort {
        consumer,
        shared,
        layout,
        sample_rate,
    };
    (tap, port)
}
