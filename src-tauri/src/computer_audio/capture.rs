//! Capture side of the computer-audio input: whatever the platform delivers
//! (interleaved with any channel count, or one buffer per channel) →
//! interleaved stereo at the engine rate → the ring the audio callback reads.
//!
//! Runs on the platform's capture thread (Core Audio IOProc, WASAPI thread),
//! so it is real-time safe: every buffer is sized in `new`, input is taken
//! in chunks that fit them, and a full ring only drops and counts.
use super::ComputerAudioStats;
use crate::ndi::receive::convert::{push_stereo_frames, StreamResampler, STEREO};
use ringbuf::traits::Producer;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Input frames handled per pass; larger deliveries are split.
pub const CHUNK_FRAMES: usize = 1_024;

pub struct CaptureWriter<P> {
    producer: P,
    source_rate: u32,
    engine_rate: u32,
    resampler: StreamResampler,
    /// One chunk of interleaved stereo at the source rate.
    stereo: Vec<f32>,
    /// The same chunk at the engine rate.
    out: Vec<f32>,
    stats: Arc<ComputerAudioStats>,
}

/// Output samples one input chunk can produce, with headroom for rounding.
fn out_capacity(source_rate: u32, engine_rate: u32) -> usize {
    let ratio = engine_rate.max(1) as f64 / source_rate.max(1) as f64;
    ((CHUNK_FRAMES as f64 * ratio).ceil() as usize + 4) * STEREO
}

impl<P: Producer<Item = f32>> CaptureWriter<P> {
    pub fn new(producer: P, source_rate: u32, engine_rate: u32, stats: Arc<ComputerAudioStats>) -> Self {
        CaptureWriter {
            producer,
            source_rate,
            engine_rate,
            resampler: StreamResampler::new(),
            stereo: Vec::with_capacity(CHUNK_FRAMES * STEREO),
            out: Vec::with_capacity(out_capacity(source_rate, engine_rate)),
            stats,
        }
    }

    /// Interleaved frames with `channels` channels: mono is duplicated,
    /// more than two keeps the first two. A trailing partial frame is
    /// ignored.
    pub fn write_interleaved(&mut self, data: &[f32], channels: usize) {
        let channels = channels.max(1);
        for chunk in data.chunks(CHUNK_FRAMES * channels) {
            self.stereo.clear();
            for frame in chunk.chunks_exact(channels) {
                let left = frame[0];
                let right = if channels > 1 { frame[1] } else { left };
                self.stereo.push(left);
                self.stereo.push(right);
            }
            self.flush();
        }
    }

    /// One buffer per side (pass the same slice twice for mono). Unequal
    /// lengths use the shorter.
    pub fn write_planar(&mut self, left: &[f32], right: &[f32]) {
        let frames = left.len().min(right.len());
        for start in (0..frames).step_by(CHUNK_FRAMES) {
            let end = (start + CHUNK_FRAMES).min(frames);
            self.stereo.clear();
            for (&l, &r) in left[start..end].iter().zip(right[start..end].iter()) {
                self.stereo.push(l);
                self.stereo.push(r);
            }
            self.flush();
        }
    }

    /// Resample `stereo` into `out` and push it; count what does not fit.
    fn flush(&mut self) {
        if self.stereo.is_empty() {
            return;
        }
        self.out.clear();
        self.resampler
            .process(self.source_rate, self.engine_rate, &self.stereo, &mut self.out);
        let dropped = push_stereo_frames(&mut self.producer, &self.out);
        self.stats.captured_blocks.fetch_add(1, Ordering::Relaxed);
        if dropped > 0 {
            self.stats.dropped_samples.fetch_add(dropped, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Consumer, Observer, Split};
    use ringbuf::HeapRb;

    fn writer(capacity: usize, from: u32, to: u32) -> (CaptureWriter<ringbuf::HeapProd<f32>>, ringbuf::HeapCons<f32>, Arc<ComputerAudioStats>) {
        let (producer, consumer) = HeapRb::<f32>::new(capacity).split();
        let stats = Arc::new(ComputerAudioStats::default());
        (CaptureWriter::new(producer, from, to, stats.clone()), consumer, stats)
    }

    #[test]
    fn interleaved_keeps_left_and_right_and_folds_extra_channels() {
        let (mut w, mut c, stats) = writer(64, 48_000, 48_000);
        // Four channels: only the first two are kept.
        w.write_interleaved(&[0.1, 0.2, 9.0, 9.0, 0.3, 0.4, 9.0, 9.0], 4);
        let mut got = [0.0f32; 4];
        assert_eq!(c.pop_slice(&mut got), 4);
        assert_eq!(got, [0.1, 0.2, 0.3, 0.4]);
        // Mono is duplicated.
        w.write_interleaved(&[0.5, 0.6], 1);
        assert_eq!(c.pop_slice(&mut got), 4);
        assert_eq!(got, [0.5, 0.5, 0.6, 0.6]);
        assert_eq!(stats.captured_blocks.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn planar_is_interleaved() {
        let (mut w, mut c, _) = writer(64, 48_000, 48_000);
        w.write_planar(&[0.1, 0.3], &[0.2, 0.4, 0.9]);
        let mut got = [0.0f32; 4];
        assert_eq!(c.pop_slice(&mut got), 4);
        assert_eq!(got, [0.1, 0.2, 0.3, 0.4]);
        assert!(c.is_empty());
    }

    #[test]
    fn a_full_ring_drops_whole_frames_and_counts_them() {
        let (mut w, c, stats) = writer(6, 48_000, 48_000);
        w.write_interleaved(&[0.1; 10], 2);
        assert_eq!(c.occupied_len(), 6);
        assert_eq!(stats.dropped_samples.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn source_rate_is_converted_to_the_engine_rate() {
        let (mut w, c, _) = writer(48_000 * 4, 44_100, 48_000);
        // One second in uneven deliveries larger than a chunk.
        let mut sent = 0;
        for size in [3_000usize, 441, 10_000].iter().cycle() {
            let frames = (*size).min(44_100 - sent);
            w.write_interleaved(&vec![0.25; frames * 2], 2);
            sent += frames;
            if sent == 44_100 {
                break;
            }
        }
        let frames = c.occupied_len() / STEREO;
        assert!((47_990..=48_010).contains(&frames), "{frames}");
    }

    #[test]
    fn buffers_are_sized_for_one_chunk() {
        let (w, _, _) = writer(16, 8_000, 48_000);
        assert!(w.out.capacity() >= CHUNK_FRAMES * 6 * STEREO);
        assert!(w.stereo.capacity() >= CHUNK_FRAMES * STEREO);
    }
}
