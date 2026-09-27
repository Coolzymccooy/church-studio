/// Fixed-capacity circular history of recent mono input samples.
///
/// The audio callback writes every block with `push` (no allocation, no
/// memmove — at most two `copy_from_slice` calls). The control thread takes a
/// `snapshot` (allocates, oldest → newest) when it needs the recent audio, for
/// example to compute a noise profile.
pub struct HistoryRing {
    buf: Vec<f32>,
    /// Next write position.
    write: usize,
    /// Number of valid samples (≤ capacity).
    len: usize,
}

impl HistoryRing {
    pub fn new(capacity: usize) -> Self {
        HistoryRing {
            buf: vec![0.0; capacity.max(1)],
            write: 0,
            len: 0,
        }
    }

    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Append samples, overwriting the oldest ones when full.
    pub fn push(&mut self, samples: &[f32]) {
        let cap = self.buf.len();
        let src = if samples.len() > cap {
            &samples[samples.len() - cap..]
        } else {
            samples
        };
        if src.is_empty() {
            return;
        }

        let first = (cap - self.write).min(src.len());
        self.buf[self.write..self.write + first].copy_from_slice(&src[..first]);
        let rest = src.len() - first;
        if rest > 0 {
            self.buf[..rest].copy_from_slice(&src[first..]);
        }

        self.write = (self.write + src.len()) % cap;
        self.len = (self.len + src.len()).min(cap);
    }

    /// Copy of the stored samples, oldest first.
    pub fn snapshot(&self) -> Vec<f32> {
        let cap = self.buf.len();
        let mut out = Vec::with_capacity(self.len);
        let start = (self.write + cap - self.len) % cap;
        let first = (cap - start).min(self.len);
        out.extend_from_slice(&self.buf[start..start + first]);
        out.extend_from_slice(&self.buf[..self.len - first]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::HistoryRing;

    fn ramp(from: usize, to: usize) -> Vec<f32> {
        (from..to).map(|v| v as f32).collect()
    }

    #[test]
    fn empty_snapshot() {
        let ring = HistoryRing::new(8);
        assert_eq!(ring.len(), 0);
        assert!(ring.snapshot().is_empty());
    }

    #[test]
    fn partial_fill_keeps_order() {
        let mut ring = HistoryRing::new(8);
        ring.push(&ramp(0, 3));
        ring.push(&ramp(3, 5));
        assert_eq!(ring.len(), 5);
        assert_eq!(ring.snapshot(), ramp(0, 5));
    }

    #[test]
    fn wraparound_returns_oldest_to_newest() {
        let mut ring = HistoryRing::new(8);
        ring.push(&ramp(0, 5));
        ring.push(&ramp(5, 11)); // wraps: keeps 3..11
        assert_eq!(ring.len(), 8);
        assert_eq!(ring.snapshot(), ramp(3, 11));

        for i in 11..40 {
            ring.push(&[i as f32]);
        }
        assert_eq!(ring.snapshot(), ramp(32, 40));
    }

    #[test]
    fn oversized_push_keeps_newest() {
        let mut ring = HistoryRing::new(4);
        ring.push(&ramp(0, 2));
        ring.push(&ramp(2, 12));
        assert_eq!(ring.capacity(), 4);
        assert_eq!(ring.snapshot(), ramp(8, 12));
    }

    #[test]
    fn exact_capacity_push_wraps_cleanly() {
        let mut ring = HistoryRing::new(4);
        ring.push(&ramp(0, 4));
        assert_eq!(ring.snapshot(), ramp(0, 4));
        ring.push(&ramp(4, 8));
        assert_eq!(ring.snapshot(), ramp(4, 8));
        ring.push(&ramp(8, 9));
        assert_eq!(ring.snapshot(), ramp(5, 9));
    }
}
