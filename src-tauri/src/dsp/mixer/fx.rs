//! Wet-only stereo effects. Delay lines are allocated at engine construction
//! and remain on the callback thread. Every stored sample is finite or zero.
use super::smooth::{ramp_samples, LinearSmoother};

#[inline]
fn clean(sample: f32) -> f32 {
    if sample.is_finite() && sample.abs() >= 1e-20 {
        sample
    } else {
        0.0
    }
}

struct Line<const CHANNELS: usize> {
    samples: Vec<[f32; CHANNELS]>,
    write: usize,
}

impl<const CHANNELS: usize> Line<CHANNELS> {
    fn new(size: usize) -> Self {
        Self {
            samples: vec![[0.0; CHANNELS]; size.max(1)],
            write: 0,
        }
    }

    fn at(&self, distance: usize) -> [f32; CHANNELS] {
        let index = if self.write >= distance {
            self.write - distance
        } else {
            self.samples.len() - (distance - self.write)
        };
        self.samples[index]
    }

    fn push(&mut self, sample: [f32; CHANNELS]) {
        self.samples[self.write] = sample.map(clean);
        self.write += 1;
        if self.write == self.samples.len() {
            self.write = 0;
        }
    }

    fn oldest(&self) -> f32 {
        self.samples[self.write][0]
    }
}

fn sample_count(sr: f32, seconds: f32) -> usize {
    ((sr * seconds).round() as usize).max(1)
}

const REVERB_INPUT_GAIN: f32 = 0.06;
const DAMPING_HZ: f32 = 3_000.0;

struct Comb {
    line: Line<1>,
    delay_seconds: f32,
    gain: LinearSmoother,
    damping: f32,
    filtered: f32,
}

impl Comb {
    fn new(sr: f32, seconds: f32) -> Self {
        let length = sample_count(sr, seconds);
        Self {
            line: Line::new(length),
            delay_seconds: length as f32 / sr,
            gain: LinearSmoother::new(0.0, ramp_samples(sr as f64)),
            damping: (-std::f32::consts::TAU * DAMPING_HZ / sr).exp(),
            filtered: 0.0,
        }
    }

    fn set_decay(&mut self, rt60: f32, snap: bool) {
        // −60 dB at DC after rt60 seconds of circulation. The damping
        // pole has unity DC gain and gives higher bands a shorter tail.
        let gain = 10f32.powf(-3.0 * self.delay_seconds / rt60);
        if snap {
            self.gain.snap(gain);
        } else {
            self.gain.set_target(gain);
        }
    }

    fn tick(&mut self, sample: f32) -> f32 {
        let echo = self.line.oldest();
        self.filtered = clean(echo * (1.0 - self.damping) + self.filtered * self.damping);
        self.line.push([sample + self.gain.next() * self.filtered]);
        echo
    }
}

struct Diffuser(Line<1>);

impl Diffuser {
    fn tick(&mut self, sample: f32) -> f32 {
        let delayed = self.0.oldest();
        let mixed = sample + 0.5 * delayed;
        self.0.push([mixed]);
        clean(delayed - 0.5 * mixed)
    }
}

struct RoomSide {
    combs: [Comb; 4],
    diffusers: [Diffuser; 2],
}

impl RoomSide {
    fn new(sr: f32, offset: f32) -> Self {
        Self {
            combs: [0.0297, 0.0371, 0.0411, 0.0437].map(|delay| Comb::new(sr, delay + offset)),
            diffusers: [0.0050, 0.0017]
                .map(|delay| Diffuser(Line::new(sample_count(sr, delay + offset)))),
        }
    }

    fn tick(&mut self, sample: f32) -> f32 {
        let mut wet = self
            .combs
            .iter_mut()
            .map(|comb| comb.tick(sample))
            .sum::<f32>()
            * 0.25;
        for diffuser in &mut self.diffusers {
            wet = diffuser.tick(wet);
        }
        wet
    }
}

pub(super) struct Reverb {
    sides: [RoomSide; 2],
    decay: f32,
}

impl Reverb {
    pub(super) fn new(sr: f32) -> Self {
        Self {
            sides: [RoomSide::new(sr, 0.0), RoomSide::new(sr, 0.0013)],
            decay: 0.0,
        }
    }

    pub(super) fn set_decay(&mut self, decay: f32, snap: bool) {
        if snap || self.decay != decay {
            self.decay = decay;
            for side in &mut self.sides {
                for comb in &mut side.combs {
                    comb.set_decay(decay, snap);
                }
            }
        }
    }

    #[inline]
    pub(super) fn tick(&mut self, [l, r]: [f32; 2]) -> [f32; 2] {
        let (l, r) = (clean(l) * REVERB_INPUT_GAIN, clean(r) * REVERB_INPUT_GAIN);
        [
            self.sides[0].tick(0.8 * l + 0.2 * r),
            self.sides[1].tick(0.8 * r + 0.2 * l),
        ]
    }
}

pub(super) struct Delay {
    sr: f32,
    line: Line<2>,
    previous: usize,
    current: usize,
    requested: usize,
    left: usize,
    fade_samples: usize,
    feedback: LinearSmoother,
}

impl Delay {
    pub(super) fn new(sr: f32) -> Self {
        let size = sample_count(sr, 1.5);
        let initial = sample_count(sr, 0.5);
        Self {
            sr,
            line: Line::new(size),
            previous: initial,
            current: initial,
            requested: initial,
            left: 0,
            fade_samples: sample_count(sr, 0.03),
            feedback: LinearSmoother::new(0.25, ramp_samples(sr as f64)),
        }
    }

    pub(super) fn configure(&mut self, bpm: f32, feedback_pct: f32, snap: bool) {
        self.requested = sample_count(self.sr, 60.0 / bpm).min(self.line.samples.len());
        if snap {
            self.current = self.requested;
            self.left = 0;
            self.feedback.snap(feedback_pct / 100.0);
        } else {
            self.feedback.set_target(feedback_pct / 100.0);
        }
    }

    #[inline]
    pub(super) fn tick(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.left == 0 && self.current != self.requested {
            self.previous = self.current;
            self.current = self.requested;
            self.left = self.fade_samples;
        }
        let next = self.line.at(self.current);
        let wet = if self.left > 0 {
            let old = self.line.at(self.previous);
            let mix = 1.0 - self.left as f32 / self.fade_samples as f32;
            self.left -= 1;
            std::array::from_fn(|i| clean(old[i] + (next[i] - old[i]) * mix))
        } else {
            next
        };

        let feedback = self.feedback.next();
        self.line
            .push(std::array::from_fn(|i| clean(input[i]) + wet[i] * feedback));
        wet
    }
}
