//! Received NDI® audio → interleaved stereo at the engine rate.
//!
//! Runs on the receiver thread (allocation is allowed there, but the buffers
//! are reused). NDI delivers planar float (FLTP) at the sender's rate and
//! channel count:
//! - channels: mono is duplicated to both sides; more than two keeps the
//!   first two;
//! - rate: linear interpolation when the source rate differs from the
//!   engine's, continuous across frames (`StreamResampler`).
use super::super::ffi::FOURCC_AUDIO_FLTP;
use ringbuf::traits::{Observer, Producer};

/// Stereo: two interleaved samples per frame.
pub const STEREO: usize = 2;

/// Header of a received audio frame, checked before its buffer is read.
#[derive(Clone, Copy, Debug)]
pub struct FrameHeader {
    pub sample_rate: i32,
    pub channels: i32,
    pub samples: i32,
    pub four_cc: u32,
    /// Address of `p_data` (0 = NULL).
    pub data_addr: usize,
    pub stride_bytes: i32,
}

/// How many planes (1 or 2) to read from a frame, or `None` when the frame
/// cannot be read safely: not FLTP, empty, NULL or misaligned data, or a
/// channel stride shorter than one plane.
pub fn planes_to_read(header: &FrameHeader) -> Option<usize> {
    let float = std::mem::size_of::<f32>();
    if header.four_cc != FOURCC_AUDIO_FLTP
        || header.sample_rate <= 0
        || header.channels <= 0
        || header.samples <= 0
        || header.data_addr == 0
        || header.data_addr % std::mem::align_of::<f32>() != 0
        || header.stride_bytes <= 0
    {
        return None;
    }
    let stride = header.stride_bytes as usize;
    if stride % float != 0 || stride < header.samples as usize * float {
        return None;
    }
    Some((header.channels as usize).min(STEREO))
}

/// Interleave one or two planes as stereo into `out` (appended). One plane
/// is duplicated; planes of unequal length use the shorter.
pub fn interleave_stereo(planes: &[&[f32]], out: &mut Vec<f32>) {
    let (left, right) = match planes {
        [] => return,
        [mono] => (*mono, *mono),
        [left, right, ..] => (*left, *right),
    };
    out.reserve(left.len().min(right.len()) * STEREO);
    for (&l, &r) in left.iter().zip(right.iter()) {
        out.push(l);
        out.push(r);
    }
}

/// Streaming linear-interpolation resampler for interleaved stereo. The
/// last input frame of each chunk is kept, so a stream cut into chunks
/// resamples exactly as it would in one piece.
#[derive(Debug, Default)]
pub struct StreamResampler {
    from_rate: u32,
    to_rate: u32,
    /// Last input frame of the previous chunk (position −1 of the next one).
    last: [f32; STEREO],
    /// Position of the next output, in input frames after `last`.
    frac: f64,
    primed: bool,
}

impl StreamResampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resample `input` (interleaved stereo at `from_rate`) to `to_rate`,
    /// appending to `out`. A rate change restarts the interpolation.
    pub fn process(&mut self, from_rate: u32, to_rate: u32, input: &[f32], out: &mut Vec<f32>) {
        let frames = input.len() / STEREO;
        if frames == 0 || from_rate == 0 || to_rate == 0 {
            return;
        }
        if from_rate == to_rate {
            out.extend_from_slice(&input[..frames * STEREO]);
            return;
        }
        if !self.primed || from_rate != self.from_rate || to_rate != self.to_rate {
            self.from_rate = from_rate;
            self.to_rate = to_rate;
            self.last = [input[0], input[1]];
            self.frac = 1.0; // start exactly on the first input frame
            self.primed = true;
        }
        let step = from_rate as f64 / to_rate as f64;
        let frame = |i: isize| -> [f32; STEREO] {
            if i < 0 {
                self.last
            } else {
                let at = i as usize * STEREO;
                [input[at], input[at + 1]]
            }
        };
        // Positions are relative to `last` (0) and input frame k (k + 1).
        let end = frames as f64; // position of the last input frame
        let mut pos = self.frac;
        out.reserve(((end - pos) / step).max(0.0) as usize * STEREO + STEREO);
        while pos <= end {
            let base = pos.floor();
            let t = (pos - base) as f32;
            let a = frame(base as isize - 1);
            let b = if pos >= end { a } else { frame(base as isize) };
            out.push(a[0] + (b[0] - a[0]) * t);
            out.push(a[1] + (b[1] - a[1]) * t);
            pos += step;
        }
        let last = frame(frames as isize - 1);
        self.last = last;
        self.frac = pos - end;
    }
}

/// Converts each received frame to interleaved stereo at the engine rate,
/// reusing its buffers.
pub struct SourceConverter {
    engine_rate: u32,
    stereo: Vec<f32>,
    out: Vec<f32>,
    resampler: StreamResampler,
}

impl SourceConverter {
    pub fn new(engine_rate: u32) -> Self {
        SourceConverter {
            engine_rate,
            stereo: Vec::new(),
            out: Vec::new(),
            resampler: StreamResampler::new(),
        }
    }

    /// `planes` (1 or 2, at `source_rate`) → interleaved stereo at the
    /// engine rate. The slice is valid until the next call.
    pub fn convert(&mut self, source_rate: u32, planes: &[&[f32]]) -> &[f32] {
        self.stereo.clear();
        self.out.clear();
        interleave_stereo(planes, &mut self.stereo);
        self.resampler
            .process(source_rate, self.engine_rate, &self.stereo, &mut self.out);
        &self.out
    }
}

/// Push as many whole stereo frames of `samples` as fit, in one
/// `push_slice` (so the consumer never sees half a frame). Returns the
/// number of samples that did not fit and were dropped.
pub fn push_stereo_frames<P: Producer<Item = f32>>(producer: &mut P, samples: &[f32]) -> u64 {
    let whole = samples.len() - samples.len() % STEREO;
    let room = producer.vacant_len() - producer.vacant_len() % STEREO;
    let take = whole.min(room);
    let pushed = producer.push_slice(&samples[..take]);
    (samples.len() - pushed) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Consumer, Observer, Split};
    use ringbuf::HeapRb;

    fn header() -> FrameHeader {
        FrameHeader {
            sample_rate: 48_000,
            channels: 2,
            samples: 480,
            four_cc: FOURCC_AUDIO_FLTP,
            data_addr: 0x1000,
            stride_bytes: 480 * 4,
        }
    }

    #[test]
    fn readable_frames_give_one_or_two_planes() {
        assert_eq!(planes_to_read(&header()), Some(2));
        assert_eq!(planes_to_read(&FrameHeader { channels: 1, ..header() }), Some(1));
        assert_eq!(planes_to_read(&FrameHeader { channels: 6, ..header() }), Some(2));
    }

    #[test]
    fn unreadable_frames_are_rejected() {
        let bad = [
            FrameHeader { four_cc: 0, ..header() },
            FrameHeader { samples: 0, ..header() },
            FrameHeader { channels: 0, ..header() },
            FrameHeader { sample_rate: 0, ..header() },
            FrameHeader { data_addr: 0, ..header() },
            FrameHeader { data_addr: 0x1001, ..header() },
            FrameHeader { stride_bytes: 479 * 4, ..header() },
            FrameHeader { stride_bytes: 480 * 4 + 2, ..header() },
        ];
        for h in bad {
            assert_eq!(planes_to_read(&h), None, "{h:?}");
        }
    }

    #[test]
    fn mono_is_duplicated_and_extra_channels_dropped() {
        let mut out = Vec::new();
        interleave_stereo(&[&[1.0, 2.0]], &mut out);
        assert_eq!(out, vec![1.0, 1.0, 2.0, 2.0]);
        out.clear();
        interleave_stereo(&[&[1.0, 2.0], &[-1.0, -2.0], &[9.0, 9.0]], &mut out);
        assert_eq!(out, vec![1.0, -1.0, 2.0, -2.0]);
        out.clear();
        interleave_stereo(&[], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn same_rate_passes_through() {
        let mut r = StreamResampler::new();
        let mut out = Vec::new();
        r.process(48_000, 48_000, &[0.1, 0.2, 0.3, 0.4, 0.5], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    fn ramp(frames: usize) -> Vec<f32> {
        (0..frames).flat_map(|i| [i as f32, -(i as f32)]).collect()
    }

    #[test]
    fn upsampling_doubles_and_interpolates() {
        let mut r = StreamResampler::new();
        let mut out = Vec::new();
        r.process(24_000, 48_000, &ramp(100), &mut out);
        let frames = out.len() / 2;
        assert!((198..=200).contains(&frames), "{frames}");
        assert_eq!(out[0], 0.0);
        assert!((out[2] - 0.5).abs() < 1e-6);
        assert!((out[3] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn downsampling_halves() {
        let mut r = StreamResampler::new();
        let mut out = Vec::new();
        r.process(96_000, 48_000, &ramp(200), &mut out);
        let frames = out.len() / 2;
        assert!((99..=101).contains(&frames), "{frames}");
        assert!((out[2] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn chunked_stream_matches_one_piece() {
        let input = ramp(1_000);
        let mut whole = Vec::new();
        StreamResampler::new().process(44_100, 48_000, &input, &mut whole);
        let mut pieces = Vec::new();
        let mut r = StreamResampler::new();
        for chunk in input.chunks(2 * 137) {
            r.process(44_100, 48_000, chunk, &mut pieces);
        }
        assert_eq!(whole.len(), pieces.len());
        for (a, b) in whole.iter().zip(pieces.iter()) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn converter_handles_mono_at_another_rate() {
        let mut conv = SourceConverter::new(48_000);
        let plane = vec![0.5f32; 441];
        let out = conv.convert(44_100, &[&plane]).to_vec();
        assert!((478..=482).contains(&(out.len() / 2)), "{}", out.len());
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn push_keeps_whole_frames_and_counts_the_rest() {
        let (mut producer, mut consumer) = HeapRb::<f32>::new(5).split();
        let dropped = push_stereo_frames(&mut producer, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(dropped, 2);
        assert_eq!(consumer.occupied_len(), 4);
        assert_eq!(consumer.try_pop(), Some(1.0));
    }
}
