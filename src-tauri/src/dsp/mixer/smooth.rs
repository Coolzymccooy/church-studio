//! Per-sample linear parameter ramps (de-zippering).

/// Default ramp length for gains, pans and mutes.
pub const RAMP_MS: f64 = 10.0;

/// Number of samples a ramp of `RAMP_MS` takes at `sample_rate`.
pub fn ramp_samples(sample_rate: f64) -> u32 {
    let n = (sample_rate * RAMP_MS / 1000.0).round();
    if n.is_finite() && n >= 1.0 {
        n as u32
    } else {
        1
    }
}

/// Moves linearly from the current value to a new target over a fixed number
/// of samples. Retargeting mid-ramp starts a new ramp from where it is, so
/// the output never jumps.
#[derive(Clone, Copy)]
pub struct LinearSmoother {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    ramp: u32,
}

impl LinearSmoother {
    pub fn new(value: f32, ramp: u32) -> Self {
        LinearSmoother {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp: ramp.max(1),
        }
    }

    pub fn set_target(&mut self, target: f32) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.remaining = self.ramp;
        self.step = (target - self.current) / self.ramp as f32;
    }

    /// Jump straight to `value` (used before the first block).
    pub fn snap(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        self.step = 0.0;
        self.remaining = 0;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            } else {
                self.current += self.step;
            }
        }
        self.current
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn target(&self) -> f32 {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_linearly_to_target() {
        let mut s = LinearSmoother::new(0.0, 4);
        s.set_target(1.0);
        let v: Vec<f32> = (0..6).map(|_| s.next()).collect();
        assert_eq!(v, vec![0.25, 0.5, 0.75, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn retarget_mid_ramp_is_continuous() {
        let mut s = LinearSmoother::new(0.0, 4);
        s.set_target(1.0);
        s.next();
        s.next(); // 0.5
        s.set_target(0.0);
        let a = s.next();
        assert!((a - 0.375).abs() < 1e-6);
    }
}
