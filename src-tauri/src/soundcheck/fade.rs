use crate::mixer_control::MAX_STRIPS;

pub(super) type Frame = [f32; MAX_STRIPS];
const SILENCE: Frame = [0.0; MAX_STRIPS];

pub(super) struct PlaybackFade {
    frames: u32,
    start: u32,
    remaining: u32,
    last: Frame,
    anchor: Frame,
}

impl PlaybackFade {
    pub(super) fn new(rate: u32) -> Self {
        Self {
            frames: (rate / 200).max(1),
            start: 0,
            remaining: 0,
            last: SILENCE,
            anchor: SILENCE,
        }
    }

    pub(super) fn restart(&mut self) {
        self.start = 0;
    }

    pub(super) fn release(&mut self) {
        if self.remaining == 0 && self.last.iter().any(|x| *x != 0.0) {
            self.remaining = self.frames;
            self.anchor = self.last;
        }
        self.restart();
    }

    pub(super) fn is_releasing(&self) -> bool {
        self.remaining != 0
    }

    pub(super) fn render(&mut self, frame: Option<Frame>, position: u64, length: u64) -> Frame {
        if let Some(frame) = frame {
            let edge = u64::from(self.frames).min((length / 2).max(1)) as u32;
            self.start = self.start.saturating_add(1).min(edge);
            let left = length.saturating_sub(position + 1).min(u64::from(edge)) as u32;
            let gain = self.start.min(left) as f32 / edge as f32;
            self.last = frame.map(|sample| sample * gain);
        } else {
            self.release();
            if self.remaining > 0 {
                self.remaining -= 1;
                let gain = self.remaining as f32 / self.frames as f32;
                self.last = self.anchor.map(|sample| sample * gain);
            } else {
                self.last = SILENCE;
            }
        }
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_fades_out_and_recovery_fades_in() {
        let mut fade = PlaybackFade::new(48_000);
        for i in 0..512 {
            fade.render(Some([0.5; MAX_STRIPS]), i, 10_000);
        }
        let mut prior = 0.5;
        for _ in 0..240 {
            let next = fade.render(None, 512, 10_000)[0];
            assert!(next < prior && prior - next < 0.003);
            prior = next;
        }
        assert_eq!(prior, 0.0);
        assert_eq!(fade.render(None, 512, 10_000)[0], 0.0);
        let resumed = fade.render(Some([-0.5; MAX_STRIPS]), 512, 10_000)[0];
        assert!(resumed < 0.0 && resumed.abs() < 0.003);
    }
}
