//! The record ring's frame layout and the pure, allocation-free helpers on
//! both sides of it (design decision 4).
//!
//! One ring frame holds every record channel: the raw strips in order, then
//! Stream L/R, then Main L/R. Main is always in the ring so "Include Main"
//! can change without restarting the engine; the writer skips it when off.
use ringbuf::traits::Producer;

/// Stream L/R + Main L/R after the strips.
pub const BUS_CHANNELS: usize = 4;

/// Channel layout of one ring frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordLayout {
    pub strips: usize,
}

impl RecordLayout {
    pub fn channels(&self) -> usize {
        self.strips + BUS_CHANNELS
    }

    /// First ring channel of the Stream bus (L; R follows).
    pub fn stream_channel(&self) -> usize {
        self.strips
    }

    /// First ring channel of the Main bus (L; R follows).
    pub fn main_channel(&self) -> usize {
        self.strips + 2
    }

    /// Ring capacity in samples for `seconds` of audio at `sample_rate`.
    pub fn ring_samples(&self, sample_rate: u32, seconds: usize) -> usize {
        (sample_rate as usize * seconds).max(1) * self.channels()
    }
}

/// Interleave the first `frames` samples of each source into `out`
/// (`sources.len()` channels per frame). Returns the frames written, which
/// is limited by `out` and the shortest source. Never allocates.
pub fn interleave(sources: &[&[f32]], frames: usize, out: &mut [f32]) -> usize {
    let channels = sources.len();
    if channels == 0 {
        return 0;
    }
    let shortest = sources.iter().map(|s| s.len()).min().unwrap_or(0);
    let frames = frames.min(shortest).min(out.len() / channels);
    for (f, frame) in out.chunks_exact_mut(channels).take(frames).enumerate() {
        for (slot, source) in frame.iter_mut().zip(sources.iter()) {
            *slot = source[f];
        }
    }
    frames
}

/// Whole frames of `frames` that fit in `vacant` free samples.
pub fn frames_that_fit(vacant: usize, channels: usize, frames: usize) -> usize {
    (vacant / channels.max(1)).min(frames)
}

/// Push `frames` interleaved frames of `channels` in one `push_slice`, as
/// many whole frames as fit (so the consumer never sees half a frame).
/// Returns the frames dropped.
pub fn push_whole_frames<P: Producer<Item = f32>>(
    producer: &mut P,
    interleaved: &[f32],
    channels: usize,
    frames: usize,
) -> u64 {
    let channels = channels.max(1);
    let frames = frames.min(interleaved.len() / channels);
    let fit = frames_that_fit(producer.vacant_len(), channels, frames);
    let pushed = producer.push_slice(&interleaved[..fit * channels]) / channels;
    (frames - pushed) as u64
}

/// Copy `count` adjacent channels starting at `first` out of interleaved
/// `data` (`stride` channels per frame) into `out`, interleaved. `out` is
/// cleared first and keeps its capacity.
pub fn extract_channels(data: &[f32], stride: usize, first: usize, count: usize, out: &mut Vec<f32>) {
    out.clear();
    if stride == 0 || first + count > stride {
        return;
    }
    for frame in data.chunks_exact(stride) {
        out.extend_from_slice(&frame[first..first + count]);
    }
}

/// Recording time of `frames` at `sample_rate`, in seconds.
pub fn frames_to_seconds(frames: u64, sample_rate: u32) -> f64 {
    if sample_rate == 0 {
        return 0.0;
    }
    frames as f64 / sample_rate as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Consumer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn layout_puts_buses_after_the_strips() {
        let layout = RecordLayout { strips: 3 };
        assert_eq!(layout.channels(), 7);
        assert_eq!(layout.stream_channel(), 3);
        assert_eq!(layout.main_channel(), 5);
        assert_eq!(layout.ring_samples(48_000, 4), 48_000 * 4 * 7);
    }

    #[test]
    fn interleave_then_extract_round_trips() {
        let a = [1.0f32, 2.0, 3.0];
        let b = [10.0f32, 20.0, 30.0];
        let c = [100.0f32, 200.0, 300.0];
        let mut out = [0.0f32; 9];
        let frames = interleave(&[&a[..], &b[..], &c[..]], 3, &mut out);
        assert_eq!(frames, 3);
        assert_eq!(out, [1.0, 10.0, 100.0, 2.0, 20.0, 200.0, 3.0, 30.0, 300.0]);

        let mut track = Vec::new();
        extract_channels(&out, 3, 0, 1, &mut track);
        assert_eq!(track, vec![1.0, 2.0, 3.0]);
        extract_channels(&out, 3, 1, 2, &mut track);
        assert_eq!(track, vec![10.0, 100.0, 20.0, 200.0, 30.0, 300.0]);
    }

    #[test]
    fn interleave_is_limited_by_the_output_and_sources() {
        let a = [1.0f32, 2.0, 3.0];
        let b = [4.0f32, 5.0];
        let mut out = [0.0f32; 8];
        assert_eq!(interleave(&[&a[..], &b[..]], 3, &mut out), 2);
        let mut small = [0.0f32; 3];
        assert_eq!(interleave(&[&a[..], &a[..]], 3, &mut small), 1);
        assert_eq!(interleave(&[], 3, &mut out), 0);
    }

    #[test]
    fn extract_rejects_channels_outside_the_frame() {
        let mut track = vec![9.0];
        extract_channels(&[1.0, 2.0], 2, 1, 2, &mut track);
        assert!(track.is_empty());
    }

    #[test]
    fn whole_frames_are_pushed_or_dropped_and_counted() {
        // Room for 7 samples = 2 whole frames of 3.
        let (mut producer, mut consumer) = HeapRb::<f32>::new(7).split();
        let data = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let dropped = push_whole_frames(&mut producer, &data, 3, 4);
        assert_eq!(dropped, 2);
        let mut popped = [0.0f32; 7];
        assert_eq!(consumer.pop_slice(&mut popped), 6, "no partial frame");
        assert_eq!(&popped[..6], &data[..6]);
    }

    #[test]
    fn frames_that_fit_rounds_down() {
        assert_eq!(frames_that_fit(7, 3, 10), 2);
        assert_eq!(frames_that_fit(100, 3, 10), 10);
        assert_eq!(frames_that_fit(2, 3, 10), 0);
    }

    #[test]
    fn marker_time_is_frames_over_rate() {
        assert_eq!(frames_to_seconds(48_000, 48_000), 1.0);
        assert_eq!(frames_to_seconds(66_150, 44_100), 1.5);
        assert_eq!(frames_to_seconds(10, 0), 0.0);
    }
}
