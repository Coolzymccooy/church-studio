/// Second-order IIR biquad filter — Direct Form II Transposed.
/// Coefficients computed in f64 for precision; samples in f32.
/// Reference: Audio EQ Cookbook, Robert Bristow-Johnson.
#[derive(Clone)]
pub struct Biquad {
    b0: f64, b1: f64, b2: f64,
    a1: f64, a2: f64,
    s1: f64, s2: f64,
}

impl Biquad {
    fn from_coeffs(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Biquad { b0: b0/a0, b1: b1/a0, b2: b2/a0, a1: a1/a0, a2: a2/a0, s1: 0.0, s2: 0.0 }
    }

    pub fn identity() -> Self {
        Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, s1: 0.0, s2: 0.0 }
    }

    /// High-pass filter (Butterworth, Q=0.707)
    pub fn hpf(freq: f64, sr: f64) -> Self {
        let q = std::f64::consts::FRAC_1_SQRT_2; // 0.7071 = Butterworth
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let alpha = w0.sin() / (2.0 * q);
        let cw = w0.cos();
        Self::from_coeffs(
            (1.0 + cw) / 2.0, -(1.0 + cw), (1.0 + cw) / 2.0,
            1.0 + alpha, -2.0 * cw, 1.0 - alpha,
        )
    }

    /// Low-pass filter (Butterworth)
    pub fn lpf(freq: f64, sr: f64) -> Self {
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let alpha = w0.sin() / (2.0 * q);
        let cw = w0.cos();
        Self::from_coeffs(
            (1.0 - cw) / 2.0, 1.0 - cw, (1.0 - cw) / 2.0,
            1.0 + alpha, -2.0 * cw, 1.0 - alpha,
        )
    }

    /// Bandpass filter (constant 0 dB peak gain)
    pub fn bpf(freq: f64, bw_hz: f64, sr: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let q = freq / bw_hz.max(1.0);
        let alpha = w0.sin() / (2.0 * q);
        Self::from_coeffs(
            alpha, 0.0, -alpha,
            1.0 + alpha, -2.0 * w0.cos(), 1.0 - alpha,
        )
    }

    /// Peaking EQ filter
    pub fn peaking(freq: f64, q: f64, gain_db: f64, sr: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let a = 10f64.powf(gain_db / 40.0); // sqrt(10^(dBgain/20))
        let alpha = w0.sin() / (2.0 * q);
        Self::from_coeffs(
            1.0 + alpha * a, -2.0 * w0.cos(), 1.0 - alpha * a,
            1.0 + alpha / a, -2.0 * w0.cos(), 1.0 - alpha / a,
        )
    }

    /// Low shelf
    pub fn low_shelf(freq: f64, gain_db: f64, sr: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let a = 10f64.powf(gain_db / 40.0);
        let alpha = w0.sin() / 2.0 * (2.0f64).sqrt(); // S=1
        let cw = w0.cos();
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_coeffs(
            a * ((a+1.0) - (a-1.0)*cw + sq),
            2.0*a * ((a-1.0) - (a+1.0)*cw),
            a * ((a+1.0) - (a-1.0)*cw - sq),
            (a+1.0) + (a-1.0)*cw + sq,
            -2.0 * ((a-1.0) + (a+1.0)*cw),
            (a+1.0) + (a-1.0)*cw - sq,
        )
    }

    /// High shelf
    pub fn high_shelf(freq: f64, gain_db: f64, sr: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let a = 10f64.powf(gain_db / 40.0);
        let alpha = w0.sin() / 2.0 * (2.0f64).sqrt();
        let cw = w0.cos();
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_coeffs(
            a * ((a+1.0) + (a-1.0)*cw + sq),
            -2.0*a * ((a-1.0) + (a+1.0)*cw),
            a * ((a+1.0) + (a-1.0)*cw - sq),
            (a+1.0) - (a-1.0)*cw + sq,
            2.0 * ((a-1.0) - (a+1.0)*cw),
            (a+1.0) - (a-1.0)*cw - sq,
        )
    }

    /// Process a single sample (Direct Form II Transposed)
    #[inline(always)]
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y as f32
    }

    pub fn reset(&mut self) { self.s1 = 0.0; self.s2 = 0.0; }
}
