//! One peer's packets → its ring, on the receiver thread (decision 5).
//!
//! Packets are written in sequence order as interleaved stereo (a mono
//! packet fills both sides), so the audio callback can read the ring with
//! the NDI `DriftReader`. A lost packet becomes `PACKET_FRAMES` frames of
//! silence, faded from the last sample, so the timing holds; the packet
//! after a gap fades in. Gaps longer than `MAX_FILL_PACKETS` are not filled:
//! the reader runs dry and re-buffers instead. Late and duplicate packets are
//! dropped. Jitter is measured as in RFC 3550 (§6.4.1).
//!
//! Allocation-free after `new`.
use super::protocol::{pcm16_decode, seq_distance, MAX_CHANNELS, PACKET_FRAMES};
use ringbuf::traits::Producer;

/// Longest gap filled with silence: 40 ms.
pub const MAX_FILL_PACKETS: usize = 8;
/// Frames over which a gap fades out and the next packet fades in (1 ms).
pub const FADE_FRAMES: usize = 48;
const STEREO: usize = 2;

/// What one packet did, for the peer's counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PushReport {
    /// Packets missing before this one.
    pub lost_packets: u32,
    /// The packet was older than one already played (dropped).
    pub late: bool,
    /// Samples the ring could not take.
    pub dropped_samples: usize,
}

/// RFC 3550 interarrival jitter, in microseconds.
#[derive(Debug, Clone, Default)]
pub struct JitterMeter {
    previous_transit: Option<f64>,
    jitter_us: f64,
}

impl JitterMeter {
    /// `arrival_us`: receiver clock; `sent_us`: when the packet was due by
    /// its sequence number (any fixed origin).
    pub fn update(&mut self, arrival_us: f64, sent_us: f64) {
        let transit = arrival_us - sent_us;
        if let Some(previous) = self.previous_transit {
            let d = (transit - previous).abs();
            self.jitter_us += (d - self.jitter_us) / 16.0;
        }
        self.previous_transit = Some(transit);
    }

    pub fn jitter_us(&self) -> f64 {
        self.jitter_us
    }
}

pub struct PeerStream {
    expected: Option<u32>,
    /// The last frame written (left, right).
    last: [f32; STEREO],
    fade_in: bool,
    decoded: [f32; PACKET_FRAMES * MAX_CHANNELS],
    stereo: [f32; PACKET_FRAMES * STEREO],
    packet_us: f64,
    jitter: JitterMeter,
}

impl PeerStream {
    pub fn new(sample_rate: u32) -> Self {
        PeerStream {
            expected: None,
            last: [0.0; STEREO],
            fade_in: false,
            decoded: [0.0; PACKET_FRAMES * MAX_CHANNELS],
            stereo: [0.0; PACKET_FRAMES * STEREO],
            packet_us: PACKET_FRAMES as f64 * 1e6 / sample_rate.max(1) as f64,
            jitter: JitterMeter::default(),
        }
    }

    pub fn jitter_us(&self) -> f64 {
        self.jitter.jitter_us()
    }

    /// Write one audio packet (`channels` 1 or 2, PCM16 `payload`) that
    /// arrived at `arrival_us`.
    pub fn push<P: Producer<Item = f32>>(
        &mut self,
        seq: u32,
        channels: usize,
        payload: &[u8],
        arrival_us: f64,
        ring: &mut P,
    ) -> PushReport {
        let mut report = PushReport::default();
        let gap = match self.expected {
            None => 0,
            Some(expected) => seq_distance(expected, seq),
        };
        if gap < 0 {
            report.late = true;
            return report;
        }
        self.expected = Some(seq.wrapping_add(1));
        self.jitter.update(arrival_us, seq as f64 * self.packet_us);
        if gap > 0 {
            report.lost_packets = gap.min(u32::MAX as i64) as u32;
            report.dropped_samples += self.fill_gap(gap as usize, ring);
        }
        let frames = self.to_stereo(channels, payload);
        report.dropped_samples += write_all(ring, &self.stereo[..frames * STEREO]);
        report
    }

    /// Decode into `stereo`, applying the fade-in after a gap. Returns frames.
    fn to_stereo(&mut self, channels: usize, payload: &[u8]) -> usize {
        let channels = channels.clamp(1, MAX_CHANNELS);
        let samples = pcm16_decode(payload, &mut self.decoded);
        let frames = samples / channels;
        for frame in 0..frames {
            let left = self.decoded[frame * channels];
            let right = self.decoded[frame * channels + channels - 1];
            let gain = if self.fade_in && frame < FADE_FRAMES {
                frame as f32 / FADE_FRAMES as f32
            } else {
                1.0
            };
            self.stereo[frame * STEREO] = left * gain;
            self.stereo[frame * STEREO + 1] = right * gain;
        }
        if frames > 0 {
            self.fade_in = false;
            let end = frames * STEREO;
            self.last = [self.stereo[end - 2], self.stereo[end - 1]];
        }
        frames
    }

    /// Silence for `gap` packets (at most `MAX_FILL_PACKETS`), faded from the
    /// last frame. Returns samples the ring could not take.
    fn fill_gap<P: Producer<Item = f32>>(&mut self, gap: usize, ring: &mut P) -> usize {
        self.fade_in = true;
        if gap > MAX_FILL_PACKETS {
            self.last = [0.0; STEREO];
            return 0;
        }
        let mut dropped = 0;
        for packet in 0..gap {
            for frame in 0..PACKET_FRAMES {
                let index = packet * PACKET_FRAMES + frame;
                let gain = if index < FADE_FRAMES {
                    1.0 - (index + 1) as f32 / FADE_FRAMES as f32
                } else {
                    0.0
                };
                self.stereo[frame * STEREO] = self.last[0] * gain;
                self.stereo[frame * STEREO + 1] = self.last[1] * gain;
            }
            dropped += write_all(ring, &self.stereo);
        }
        self.last = [0.0; STEREO];
        dropped
    }
}

/// Push whole stereo frames; returns the samples that did not fit.
fn write_all<P: Producer<Item = f32>>(ring: &mut P, samples: &[f32]) -> usize {
    let room = ring.vacant_len() - ring.vacant_len() % STEREO;
    let take = samples.len().min(room);
    let pushed = ring.push_slice(&samples[..take]);
    samples.len() - pushed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netaudio::protocol::pcm16_encode;
    use ringbuf::traits::{Consumer, Observer, Split};
    use ringbuf::HeapRb;

    fn packet(value: f32, channels: usize) -> Vec<u8> {
        let samples = vec![value; PACKET_FRAMES * channels];
        let mut bytes = vec![0u8; samples.len() * 2];
        pcm16_encode(&samples, &mut bytes);
        bytes
    }

    fn ring() -> (ringbuf::HeapProd<f32>, ringbuf::HeapCons<f32>) {
        HeapRb::<f32>::new(48_000 * 2).split()
    }

    fn drain(cons: &mut ringbuf::HeapCons<f32>) -> Vec<f32> {
        let mut out = vec![0.0; cons.occupied_len()];
        cons.pop_slice(&mut out);
        out
    }

    #[test]
    fn mono_fills_both_sides_in_order() {
        let (mut prod, mut cons) = ring();
        let mut stream = PeerStream::new(48_000);
        let report = stream.push(10, 1, &packet(0.5, 1), 0.0, &mut prod);
        assert_eq!(report, PushReport::default());
        let out = drain(&mut cons);
        assert_eq!(out.len(), PACKET_FRAMES * 2);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-4));
    }

    #[test]
    fn stereo_keeps_left_and_right_apart() {
        let (mut prod, mut cons) = ring();
        let mut stream = PeerStream::new(48_000);
        let mut samples = vec![0.0f32; PACKET_FRAMES * 2];
        for frame in samples.chunks_exact_mut(2) {
            frame[0] = 0.25;
            frame[1] = -0.25;
        }
        let mut bytes = vec![0u8; samples.len() * 2];
        pcm16_encode(&samples, &mut bytes);
        stream.push(0, 2, &bytes, 0.0, &mut prod);
        let out = drain(&mut cons);
        assert!((out[0] - 0.25).abs() < 1e-4 && (out[1] + 0.25).abs() < 1e-4);
    }

    #[test]
    fn a_lost_packet_becomes_faded_silence_of_the_same_length() {
        let (mut prod, mut cons) = ring();
        let mut stream = PeerStream::new(48_000);
        stream.push(1, 1, &packet(0.5, 1), 0.0, &mut prod);
        let report = stream.push(3, 1, &packet(0.5, 1), 0.0, &mut prod);
        assert_eq!(report.lost_packets, 1);
        let out = drain(&mut cons);
        assert_eq!(out.len(), 3 * PACKET_FRAMES * 2);
        let gap = &out[PACKET_FRAMES * 2..2 * PACKET_FRAMES * 2];
        // Fades down from the last sample, then silence.
        assert!(gap[0] < 0.5 && gap[0] > 0.4);
        assert!(gap[FADE_FRAMES * 2..].iter().all(|&s| s == 0.0));
        // The packet after the gap fades in.
        let after = &out[2 * PACKET_FRAMES * 2..];
        assert_eq!(after[0], 0.0);
        assert!((after[(FADE_FRAMES + 1) * 2] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn long_gaps_are_not_filled() {
        let (mut prod, mut cons) = ring();
        let mut stream = PeerStream::new(48_000);
        stream.push(1, 1, &packet(0.5, 1), 0.0, &mut prod);
        let report = stream.push(1 + MAX_FILL_PACKETS as u32 + 2, 1, &packet(0.5, 1), 0.0, &mut prod);
        assert_eq!(report.lost_packets, MAX_FILL_PACKETS as u32 + 1);
        assert_eq!(drain(&mut cons).len(), 2 * PACKET_FRAMES * 2);
    }

    #[test]
    fn late_and_duplicate_packets_are_dropped() {
        let (mut prod, mut cons) = ring();
        let mut stream = PeerStream::new(48_000);
        stream.push(5, 1, &packet(0.5, 1), 0.0, &mut prod);
        assert!(stream.push(5, 1, &packet(0.5, 1), 0.0, &mut prod).late);
        assert!(stream.push(4, 1, &packet(0.5, 1), 0.0, &mut prod).late);
        assert_eq!(drain(&mut cons).len(), PACKET_FRAMES * 2);
    }

    #[test]
    fn sequence_wraps_without_a_gap() {
        let (mut prod, _cons) = ring();
        let mut stream = PeerStream::new(48_000);
        stream.push(u32::MAX, 1, &packet(0.1, 1), 0.0, &mut prod);
        let report = stream.push(0, 1, &packet(0.1, 1), 0.0, &mut prod);
        assert_eq!(report, PushReport::default());
    }

    #[test]
    fn a_full_ring_counts_what_it_drops() {
        let (mut prod, _cons) = HeapRb::<f32>::new(PACKET_FRAMES * 2 + 3).split();
        let mut stream = PeerStream::new(48_000);
        stream.push(0, 1, &packet(0.1, 1), 0.0, &mut prod);
        let report = stream.push(1, 1, &packet(0.1, 1), 0.0, &mut prod);
        // Three samples free: one whole stereo frame fits, the rest drops.
        assert_eq!(report.dropped_samples, PACKET_FRAMES * 2 - 2);
        assert_eq!(prod.vacant_len(), 1);
    }

    #[test]
    fn steady_arrival_has_no_jitter_and_wobble_shows_up() {
        let mut steady = JitterMeter::default();
        for n in 0..100 {
            steady.update(n as f64 * 5_000.0 + 7_000.0, n as f64 * 5_000.0);
        }
        assert!(steady.jitter_us() < 1e-6);
        let mut wobbly = JitterMeter::default();
        for n in 0..400 {
            let wobble = if n % 2 == 0 { 0.0 } else { 2_000.0 };
            wobbly.update(n as f64 * 5_000.0 + wobble, n as f64 * 5_000.0);
        }
        assert!((wobbly.jitter_us() - 2_000.0).abs() < 50.0, "{}", wobbly.jitter_us());
    }
}
