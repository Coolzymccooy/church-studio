//! Pure, allocation-free helpers for the input → mixer → output path.
//!
//! - `deinterleave`: device-interleaved input → one buffer per channel/strip.
//! - `device_sample` / `push_stereo`: a stereo bus → an output device's
//!   interleaved channels. A 1-channel device gets (L + R) / 2; a device with
//!   2 or more channels gets L on channel 1, R on channel 2 and silence on
//!   the rest.
use ringbuf::traits::Producer;

/// Split interleaved `data` (`stride` channels per frame) into `outs`, one
/// buffer per channel starting at channel 0. Channels beyond `outs.len()`
/// are skipped; an `out` beyond `stride` is left untouched. Converts every
/// used sample with `convert`. Returns the number of frames written, which is
/// limited by the shortest `out` buffer. Never allocates.
pub fn deinterleave<T: Copy>(
    data: &[T],
    stride: usize,
    outs: &mut [Vec<f32>],
    convert: impl Fn(T) -> f32,
) -> usize {
    let stride = stride.max(1);
    let capacity = outs.iter().map(|b| b.len()).min().unwrap_or(0);
    let frames = (data.len() / stride).min(capacity);
    for (f, frame) in data.chunks_exact(stride).take(frames).enumerate() {
        for (out, &sample) in outs.iter_mut().zip(frame.iter()) {
            out[f] = convert(sample);
        }
    }
    frames
}

/// Sample for output channel `channel` of a `channels`-channel device.
#[inline]
pub fn device_sample(left: f32, right: f32, channels: usize, channel: usize) -> f32 {
    if channels <= 1 {
        (left + right) * 0.5
    } else {
        match channel {
            0 => left,
            1 => right,
            _ => 0.0,
        }
    }
}

/// Push a stereo block, scaled by `gain`, into an output ring as
/// `channels`-channel interleaved frames. Returns how many samples did not
/// fit (they are dropped, never waited for).
pub fn push_stereo<P: Producer<Item = f32>>(
    producer: &mut P,
    left: &[f32],
    right: &[f32],
    channels: usize,
    gain: f32,
) -> u64 {
    let channels = channels.max(1);
    let mut dropped = 0u64;
    for (&l, &r) in left.iter().zip(right.iter()) {
        for channel in 0..channels {
            let sample = device_sample(l, r, channels, channel) * gain;
            if producer.try_push(sample).is_err() {
                dropped += 1;
            }
        }
    }
    dropped
}
