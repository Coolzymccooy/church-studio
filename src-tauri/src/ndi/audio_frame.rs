//! Interleaved stereo → planar float (FLTP) frames for NDI.
//!
//! NDI's FLTP layout is one plane per channel: all of channel 0's samples,
//! then all of channel 1's, `channel_stride_in_bytes` apart. The sender
//! thread pops interleaved L/R samples of any length from the ring buffer;
//! `FltpFramer` scatters them into a preallocated planar buffer and hands out
//! each complete frame of `NDI_FRAME_SAMPLES` (10 ms at 48 kHz). A partial
//! frame, including half a sample frame (L without its R), is carried over
//! to the next push. Nothing allocates after `new`.

/// Samples per channel in one NDI audio frame (10 ms at 48 kHz).
pub const NDI_FRAME_SAMPLES: usize = 480;

/// NDI outputs carry one stereo bus.
pub const NDI_CHANNELS: usize = 2;

pub struct FltpFramer {
    /// `channels` planes of `frame_samples` each, channel-major.
    planar: Vec<f32>,
    channels: usize,
    frame_samples: usize,
    /// Complete sample frames already written to the current frame.
    filled: usize,
    /// Channel the next interleaved sample belongs to.
    next_channel: usize,
}

// Accessors and `push_stereo` are part of the framer's API (and tested)
// even where the sender does not need them.
#[allow(dead_code)]
impl FltpFramer {
    pub fn new(channels: usize, frame_samples: usize) -> Self {
        let channels = channels.max(1);
        let frame_samples = frame_samples.max(1);
        FltpFramer {
            planar: vec![0.0; channels * frame_samples],
            channels,
            frame_samples,
            filled: 0,
            next_channel: 0,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    /// Bytes between channel planes (`channel_stride_in_bytes`).
    pub fn channel_stride_bytes(&self) -> usize {
        self.frame_samples * std::mem::size_of::<f32>()
    }

    /// Interleaved samples waiting for the current frame to complete.
    pub fn pending_samples(&self) -> usize {
        self.filled * self.channels + self.next_channel
    }

    /// Drop any partial frame (e.g. after a stop).
    pub fn reset(&mut self) {
        self.filled = 0;
        self.next_channel = 0;
    }

    /// Append interleaved samples. `emit` receives the planar buffer each
    /// time a frame completes; the buffer is reused for the next frame, so
    /// `emit` must finish with it before returning. Returns the number of
    /// frames emitted.
    pub fn push_interleaved(&mut self, samples: &[f32], mut emit: impl FnMut(&[f32])) -> usize {
        let mut emitted = 0;
        for &sample in samples {
            self.planar[self.next_channel * self.frame_samples + self.filled] = sample;
            self.next_channel += 1;
            if self.next_channel < self.channels {
                continue;
            }
            self.next_channel = 0;
            self.filled += 1;
            if self.filled == self.frame_samples {
                emit(&self.planar[..]);
                self.filled = 0;
                emitted += 1;
            }
        }
        emitted
    }

    /// Append a stereo block given as separate L/R slices (a 2-channel
    /// framer only; any other framer interleaves the pairs in order).
    pub fn push_stereo(
        &mut self,
        left: &[f32],
        right: &[f32],
        mut emit: impl FnMut(&[f32]),
    ) -> usize {
        let mut emitted = 0;
        for (&l, &r) in left.iter().zip(right.iter()) {
            emitted += self.push_interleaved(&[l, r], &mut emit);
        }
        emitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Interleaved stereo where L = n and R = -n for sample frame n.
    fn ramp(frames: usize, start: usize) -> Vec<f32> {
        (start..start + frames)
            .flat_map(|n| [n as f32, -(n as f32)])
            .collect()
    }

    #[test]
    fn channel_order_and_stride() {
        let mut framer = FltpFramer::new(2, 4);
        assert_eq!(framer.channel_stride_bytes(), 16);
        let mut frames: Vec<Vec<f32>> = Vec::new();
        framer.push_interleaved(&ramp(4, 0), |planar| frames.push(planar.to_vec()));
        assert_eq!(frames.len(), 1);
        // Plane 0 = left, plane 1 (one stride later) = right.
        assert_eq!(frames[0], vec![0.0f32, 1.0, 2.0, 3.0, -0.0, -1.0, -2.0, -3.0]);
    }

    #[test]
    fn default_frame_is_480_stereo() {
        let framer = FltpFramer::new(NDI_CHANNELS, NDI_FRAME_SAMPLES);
        assert_eq!(framer.channels(), 2);
        assert_eq!(framer.frame_samples(), 480);
        assert_eq!(framer.channel_stride_bytes(), 1920);
    }

    #[test]
    fn partial_frames_carry_over_between_pushes() {
        let mut framer = FltpFramer::new(2, 4);
        let mut frames: Vec<Vec<f32>> = Vec::new();
        let data = ramp(10, 0);
        // Odd split: 3 samples (L0 R0 L1), then the rest.
        assert_eq!(framer.push_interleaved(&data[..3], |p| frames.push(p.to_vec())), 0);
        assert_eq!(framer.pending_samples(), 3);
        assert_eq!(framer.push_interleaved(&data[3..], |p| frames.push(p.to_vec())), 2);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0][..4], [0.0f32, 1.0, 2.0, 3.0]);
        assert_eq!(frames[1][..4], [4.0f32, 5.0, 6.0, 7.0]);
        assert_eq!(frames[1][4..], [-4.0f32, -5.0, -6.0, -7.0]);
        // Frames 8 and 9 wait for the next push.
        assert_eq!(framer.pending_samples(), 4);
    }

    #[test]
    fn push_stereo_matches_interleaved() {
        let mut a = FltpFramer::new(2, 3);
        let mut b = FltpFramer::new(2, 3);
        let left: Vec<f32> = (0..7).map(|n| n as f32).collect();
        let right: Vec<f32> = (0..7).map(|n| 100.0 + n as f32).collect();
        let interleaved: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .flat_map(|(&l, &r)| [l, r])
            .collect();
        let mut out_a: Vec<f32> = Vec::new();
        let mut out_b: Vec<f32> = Vec::new();
        let na = a.push_stereo(&left, &right, |p| out_a.extend_from_slice(p));
        let nb = b.push_interleaved(&interleaved, |p| out_b.extend_from_slice(p));
        assert_eq!(na, 2);
        assert_eq!(na, nb);
        assert_eq!(out_a, out_b);
    }

    #[test]
    fn buffer_is_reused_without_reallocating() {
        let mut framer = FltpFramer::new(2, NDI_FRAME_SAMPLES);
        let ptr_before = framer.planar.as_ptr();
        let cap_before = framer.planar.capacity();
        let mut seen: Vec<usize> = Vec::new();
        let block = ramp(333, 0);
        let mut total = 0;
        for _ in 0..20 {
            total += framer.push_interleaved(&block, |p| seen.push(p.as_ptr() as usize));
        }
        assert_eq!(total, 20 * 333 / NDI_FRAME_SAMPLES);
        assert_eq!(framer.planar.as_ptr(), ptr_before);
        assert_eq!(framer.planar.capacity(), cap_before);
        assert!(seen.iter().all(|&p| p == ptr_before as usize));
    }

    #[test]
    fn reset_drops_partial_frame() {
        let mut framer = FltpFramer::new(2, 4);
        framer.push_interleaved(&[1.0, 2.0, 3.0], |_| {});
        framer.reset();
        assert_eq!(framer.pending_samples(), 0);
    }
}
