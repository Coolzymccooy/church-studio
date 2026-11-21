//! "Play" for the last recording: plays a recorded track into the Monitor
//! (mon 1) output while the engine runs, so the operator can listen back
//! without leaving the app.
//!
//! - `preview_ring` (engine start) preallocates a 1 s stereo ring. The audio
//!   callback owns the `PreviewTap`; the engine keeps the `PreviewPlayer`.
//! - `PreviewPlayer::play` streams the track's WAV parts from a reader thread
//!   into the ring; `stop` ends it and the tap drops whatever is queued.
//! - The tap adds its frames to the Monitor output only, after the mixer,
//!   so a preview is never recorded, streamed or sent to NDI. The monitor
//!   key still applies (monitor off → silent).
//!
//! The callback side is allocation-free and lock-free.
use crate::export::wav_read::WavReader;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::io::{Read, Seek};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const RING_SECONDS: usize = 1;
const READ_FRAMES: usize = 2048;

/// State the UI polls.
#[derive(Default)]
pub struct PreviewState {
    pub playing: AtomicBool,
    stop: AtomicBool,
    /// Frames handed to the ring so far, and the track length, in frames.
    pub position_frames: AtomicU64,
    pub total_frames: AtomicU64,
    /// Set by `stop`: the tap empties the ring at the start of its next block.
    flush: AtomicBool,
}

/// Audio-thread side.
pub struct PreviewTap {
    consumer: HeapCons<f32>,
    state: Arc<PreviewState>,
}

/// Control side, owned by the running engine.
pub struct PreviewPlayer {
    producer: Arc<Mutex<Option<HeapProd<f32>>>>,
    state: Arc<PreviewState>,
    sample_rate: u32,
}

pub fn preview_ring(sample_rate: u32) -> (PreviewTap, PreviewPlayer) {
    let capacity = (sample_rate as usize).max(1) * 2 * RING_SECONDS;
    let (producer, consumer) = HeapRb::<f32>::new(capacity).split();
    let state = Arc::new(PreviewState::default());
    (
        PreviewTap { consumer, state: state.clone() },
        PreviewPlayer { producer: Arc::new(Mutex::new(Some(producer))), state, sample_rate },
    )
}

impl PreviewTap {
    /// Fill `left`/`right` with the next queued frames (zeros after them).
    /// Returns how many frames came from the preview.
    pub fn pop_into(&mut self, left: &mut [f32], right: &mut [f32]) -> usize {
        // While stopped, drop anything queued (including a last push the
        // reader made just after `stop`) and stay silent until the next play.
        let stopped = self.state.stop.load(Ordering::Acquire);
        if self.state.flush.swap(false, Ordering::AcqRel) || stopped {
            let queued = self.consumer.occupied_len();
            self.consumer.skip(queued);
        }
        let n = left.len().min(right.len());
        if stopped {
            left[..n].fill(0.0);
            right[..n].fill(0.0);
            return 0;
        }
        let frames = (self.consumer.occupied_len() / 2).min(n);
        for i in 0..frames {
            left[i] = self.consumer.try_pop().unwrap_or(0.0);
            right[i] = self.consumer.try_pop().unwrap_or(0.0);
        }
        left[frames..n].fill(0.0);
        right[frames..n].fill(0.0);
        frames
    }
}

/// What the UI shows.
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
    pub playing: bool,
    pub position_seconds: f64,
    pub duration_seconds: f64,
}

impl PreviewPlayer {
    pub fn status(&self) -> PreviewStatus {
        let sr = self.sample_rate.max(1) as f64;
        PreviewStatus {
            playing: self.state.playing.load(Ordering::Acquire),
            position_seconds: self.state.position_frames.load(Ordering::Relaxed) as f64 / sr,
            duration_seconds: self.state.total_frames.load(Ordering::Relaxed) as f64 / sr,
        }
    }

    /// Stop playing and drop whatever is queued.
    pub fn stop(&self) {
        self.state.stop.store(true, Ordering::Release);
        self.state.flush.store(true, Ordering::Release);
    }

    /// Play the WAV `readers` (the parts of one track, in order) from the
    /// start. Refuses while another preview plays, or when the recording's
    /// sample rate differs from the engine's.
    pub fn play<R: Read + Seek + Send + 'static>(&self, readers: Vec<WavReader<R>>) -> Result<(), String> {
        let first = readers.first().ok_or("nothing to play")?.format();
        if first.sample_rate != self.sample_rate {
            return Err(format!(
                "recorded at {} Hz; the engine runs at {} Hz",
                first.sample_rate, self.sample_rate
            ));
        }
        let producer = self
            .producer
            .lock()
            .map_err(|_| "the player is unavailable".to_string())?
            .take()
            .ok_or("already playing")?;
        let total: u64 = readers.iter().map(|r| r.total_frames()).sum();
        let state = self.state.clone();
        state.stop.store(false, Ordering::Release);
        state.position_frames.store(0, Ordering::Relaxed);
        state.total_frames.store(total, Ordering::Relaxed);
        state.playing.store(true, Ordering::Release);
        let slot = self.producer.clone();
        let spawned = thread::Builder::new()
            .name("record-preview".into())
            .spawn(move || {
                let producer = feed(readers, producer, &state);
                // Hand the producer back before reporting "stopped", so a
                // play pressed right after sees a ready player.
                if let Ok(mut guard) = slot.lock() {
                    *guard = Some(producer);
                }
                state.playing.store(false, Ordering::Release);
            });
        if let Err(err) = spawned {
            self.state.playing.store(false, Ordering::Release);
            return Err(format!("cannot start playback: {err}"));
        }
        Ok(())
    }
}

/// Reader thread: push stereo frames into the ring until done or stopped,
/// then wait for the tap to drain it. Returns the producer for the next play.
fn feed<R: Read + Seek>(
    readers: Vec<WavReader<R>>,
    mut producer: HeapProd<f32>,
    state: &PreviewState,
) -> HeapProd<f32> {
    let mut samples = Vec::with_capacity(READ_FRAMES * 8);
    let mut stereo = Vec::with_capacity(READ_FRAMES * 2);
    'parts: for mut reader in readers {
        let channels = reader.format().channels.max(1) as usize;
        loop {
            if state.stop.load(Ordering::Acquire) {
                break 'parts;
            }
            let frames = match reader.read_frames(READ_FRAMES, &mut samples) {
                Ok(0) => break,
                Ok(frames) => frames,
                Err(err) => {
                    log::warn!("preview: read failed: {err}");
                    break 'parts;
                }
            };
            to_stereo(&samples, channels, frames, &mut stereo);
            if !push_all(&mut producer, &stereo, state) {
                break 'parts;
            }
            state.position_frames.fetch_add(frames as u64, Ordering::Relaxed);
        }
    }
    // Let the queued tail play out unless stopped.
    while producer.occupied_len() > 0 && !state.stop.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(20));
    }
    producer
}

/// Interleaved `channels` → interleaved stereo (mono doubled, extras dropped).
pub fn to_stereo(samples: &[f32], channels: usize, frames: usize, out: &mut Vec<f32>) {
    out.clear();
    for f in 0..frames {
        let base = f * channels;
        let l = samples.get(base).copied().unwrap_or(0.0);
        let r = if channels > 1 { samples.get(base + 1).copied().unwrap_or(0.0) } else { l };
        out.push(l);
        out.push(r);
    }
}

/// Push everything, waiting while the ring is full. False if stopped.
fn push_all(producer: &mut HeapProd<f32>, data: &[f32], state: &PreviewState) -> bool {
    let mut done = 0;
    while done < data.len() {
        if state.stop.load(Ordering::Acquire) {
            return false;
        }
        let pushed = producer.push_slice(&data[done..]);
        done += pushed;
        if pushed == 0 {
            thread::sleep(Duration::from_millis(5));
        }
    }
    true
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
