//! The audio-callback side of the recorder (design decision 4). Owned by the
//! `InputProcessor`; everything is preallocated at engine start, and while
//! no recording is active it does one atomic load per block.
//!
//! Per block: `capture_raw` runs after the NDI pull and before the voice
//! chain (which works in place on its strip), saving a raw copy of that one
//! strip; `push` runs after `Mixer::process` and pushes every record channel
//! (raw strips, Stream, Main) as whole interleaved frames. A frame that does
//! not fit is dropped whole and counted. No locks, allocation or logging.
use super::frames::{interleave, push_whole_frames, RecordLayout, BUS_CHANNELS};
use super::RecordShared;
use crate::mixer_control::MAX_STRIPS;
use ringbuf::traits::Producer;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const NO_STRIP: usize = usize::MAX;
const MAX_RECORD_CHANNELS: usize = MAX_STRIPS + BUS_CHANNELS;

pub struct RecordTap<P> {
    producer: P,
    shared: Arc<RecordShared>,
    layout: RecordLayout,
    /// Raw copy of the voice-chain strip for this block.
    raw_voice: Vec<f32>,
    raw_voice_strip: usize,
    /// One block of interleaved record frames.
    interleaved: Vec<f32>,
    /// Recording state latched by `capture_raw` for this block.
    active: bool,
}

impl<P: Producer<Item = f32>> RecordTap<P> {
    /// Buffers for blocks of up to `max_frames` frames.
    pub fn new(producer: P, shared: Arc<RecordShared>, layout: RecordLayout, max_frames: usize) -> Self {
        let layout = RecordLayout {
            strips: layout.strips.min(MAX_STRIPS),
        };
        RecordTap {
            producer,
            shared,
            layout,
            raw_voice: vec![0.0; max_frames],
            raw_voice_strip: NO_STRIP,
            interleaved: vec![0.0; max_frames * layout.channels()],
            active: false,
        }
    }

    /// Before the voice chain: latch the recording flag and keep a raw copy
    /// of the strip the chain is about to process in place.
    pub fn capture_raw(&mut self, strips: &[Vec<f32>], voice_strip: usize, frames: usize) {
        self.active = self.shared.active.load(Ordering::Acquire);
        self.raw_voice_strip = NO_STRIP;
        if !self.active {
            return;
        }
        let frames = frames.min(self.raw_voice.len());
        if let Some(buf) = strips.get(voice_strip) {
            if voice_strip < self.layout.strips && buf.len() >= frames {
                self.raw_voice[..frames].copy_from_slice(&buf[..frames]);
                self.raw_voice_strip = voice_strip;
            }
        }
    }

    /// After the mixer: push raw strips + Stream + Main as whole frames.
    pub fn push(
        &mut self,
        strips: &[Vec<f32>],
        stream: (&[f32], &[f32]),
        main: (&[f32], &[f32]),
        frames: usize,
    ) {
        if !self.active {
            return;
        }
        let empty: &[f32] = &[];
        let mut sources: [&[f32]; MAX_RECORD_CHANNELS] = [empty; MAX_RECORD_CHANNELS];
        let strip_count = self.layout.strips;
        for (index, slot) in sources.iter_mut().take(strip_count).enumerate() {
            *slot = if index == self.raw_voice_strip {
                &self.raw_voice[..]
            } else {
                strips.get(index).map(|b| &b[..]).unwrap_or(empty)
            };
        }
        sources[strip_count] = stream.0;
        sources[strip_count + 1] = stream.1;
        sources[strip_count + 2] = main.0;
        sources[strip_count + 3] = main.1;

        let channels = self.layout.channels();
        let written = interleave(&sources[..channels], frames, &mut self.interleaved[..]);
        let dropped = push_whole_frames(&mut self.producer, &self.interleaved[..], channels, written);
        // A missing strip buffer (or a short block) counts as dropped too.
        let lost = dropped + (frames - written) as u64;
        self.shared
            .frames_captured
            .fetch_add(frames as u64, Ordering::Relaxed);
        if lost > 0 {
            self.shared.dropped_frames.fetch_add(lost, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Consumer, Split};
    use ringbuf::HeapRb;

    fn make_tap(capacity: usize, strips: usize) -> (RecordTap<ringbuf::HeapProd<f32>>, ringbuf::HeapCons<f32>, Arc<RecordShared>) {
        let (producer, consumer) = HeapRb::<f32>::new(capacity).split();
        let shared = Arc::new(RecordShared::new());
        let tap = RecordTap::new(producer, shared.clone(), RecordLayout { strips }, 8);
        (tap, consumer, shared)
    }

    #[test]
    fn idle_tap_pushes_nothing() {
        let (mut tap, consumer, shared) = make_tap(64, 1);
        let strips = vec![vec![1.0f32; 8]];
        tap.capture_raw(&strips, 0, 2);
        tap.push(&strips, (&[0.0; 2], &[0.0; 2]), (&[0.0; 2], &[0.0; 2]), 2);
        assert_eq!(consumer.occupied_len(), 0);
        assert_eq!(shared.frames_captured.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn records_the_raw_voice_strip_and_both_buses() {
        let (mut tap, mut consumer, shared) = make_tap(64, 2);
        shared.active.store(true, Ordering::Release);
        let mut strips = vec![vec![1.0f32, 2.0], vec![3.0f32, 4.0]];
        tap.capture_raw(&strips, 0, 2);
        // The voice chain changes strip 0 in place; the raw copy is recorded.
        strips[0][0] = 99.0;
        strips[0][1] = 99.0;
        tap.push(&strips, (&[5.0, 6.0], &[7.0, 8.0]), (&[9.0, 10.0], &[11.0, 12.0]), 2);
        let mut out = [0.0f32; 12];
        assert_eq!(consumer.pop_slice(&mut out), 12);
        assert_eq!(out, [1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0]);
        assert_eq!(shared.frames_captured.load(Ordering::Relaxed), 2);
        assert_eq!(shared.dropped_frames.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_full_ring_drops_and_counts_whole_frames() {
        // 5 channels per frame; room for one frame only.
        let (mut tap, consumer, shared) = make_tap(9, 1);
        shared.active.store(true, Ordering::Release);
        let strips = vec![vec![1.0f32; 3]];
        let bus = [0.5f32; 3];
        tap.capture_raw(&strips, 7, 3);
        tap.push(&strips, (&bus, &bus), (&bus, &bus), 3);
        assert_eq!(consumer.occupied_len(), 5);
        assert_eq!(shared.dropped_frames.load(Ordering::Relaxed), 2);
        assert_eq!(shared.frames_captured.load(Ordering::Relaxed), 3);
    }
}
