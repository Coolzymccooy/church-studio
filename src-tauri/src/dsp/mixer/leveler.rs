//! Slow loudness leveler: steers a stereo bus toward a LUFS target so the
//! stream reaches the platform at the operator's loudness.
//!
//! Measurement is BS.1770 K-weighted, both channels summed, in 100 ms
//! blocks. The level that steers the gain is the mean of the last 3 s of
//! *programme* blocks: a block only counts when the 400 ms ending with it
//! and the 400 ms starting with it are both within `GATE_BELOW_TARGET_LU` of
//! the target. Short pauses inside speech still count (as a loudness meter
//! counts them), but silence and a quiet room never enter the measurement,
//! not even the blocks at the edge of a phrase, so they do not move the gain
//! (it freezes). Looking 300 ms ahead only delays the measurement, never the
//! audio.
//!
//! The leveler measures its input (open loop, so it cannot oscillate) and
//! asks for `target − measured`, bounded to ±`LEVELER_RANGE_DB`. The applied
//! gain slews toward that at most `LEVELER_RATE_DB_PER_S`, per sample, so
//! it never clicks. It adds no latency and sits between the bus fader and
//! the bus limiter, which still holds the ceiling.
//!
//! Real-time rules: every buffer is a fixed array (no allocation), and all
//! state advances per sample or per fixed 100 ms block counted in samples,
//! so the output does not depend on the callback's block size.
use crate::dsp::biquad::Biquad;

/// Most the leveler boosts or cuts, in dB.
pub const LEVELER_RANGE_DB: f32 = 6.0;
/// Fastest the leveler's gain moves, in dB per second.
pub const LEVELER_RATE_DB_PER_S: f32 = 3.0;
/// Target range and default (LUFS).
pub const LEVELER_TARGET_MIN: f32 = -30.0;
pub const LEVELER_TARGET_MAX: f32 = -10.0;
pub const LEVELER_TARGET_DEFAULT: f32 = -14.0;

/// Momentary loudness this far under the target is not programme.
const GATE_BELOW_TARGET_LU: f32 = 20.0;
const BLOCK_MS: f64 = 100.0;
/// Momentary window, in blocks (400 ms).
const MOMENTARY_BLOCKS: usize = 4;
/// Blocks kept to judge one block by the 400 ms before and after it.
const HISTORY_BLOCKS: usize = 2 * MOMENTARY_BLOCKS - 1;
/// Measurement window, in programme blocks (3 s).
const WINDOW_BLOCKS: usize = 30;
/// Programme blocks needed before the gain moves at all.
const MIN_WINDOW_BLOCKS: usize = 4;

/// Fixed-size ring of block mean squares.
#[derive(Clone, Copy)]
struct Ring<const N: usize> {
    values: [f64; N],
    pos: usize,
    filled: usize,
}

impl<const N: usize> Ring<N> {
    const fn new() -> Self {
        Ring { values: [0.0; N], pos: 0, filled: 0 }
    }

    fn push(&mut self, v: f64) {
        self.values[self.pos] = v;
        self.pos = (self.pos + 1) % N;
        self.filled = (self.filled + 1).min(N);
    }

    fn mean(&self) -> f64 {
        if self.filled == 0 {
            return 0.0;
        }
        // The filled entries are the whole array once full, else 0..filled.
        self.values[..self.filled].iter().sum::<f64>() / self.filled as f64
    }

    fn clear(&mut self) {
        *self = Self::new();
    }

    /// Entry pushed `age` pushes ago (0 = newest). `age < filled`.
    fn at(&self, age: usize) -> f64 {
        self.values[(self.pos + N - 1 - age) % N]
    }

    /// Mean of the entries aged `from..=to` that exist (0 if none).
    fn mean_ages(&self, from: usize, to: usize) -> f64 {
        let to = to.min(self.filled.saturating_sub(1));
        if self.filled == 0 || from > to {
            return 0.0;
        }
        (from..=to).map(|a| self.at(a)).sum::<f64>() / (to - from + 1) as f64
    }
}

/// K-weighting for one channel (BS.1770 stage 1 shelf + stage 2 RLB HPF).
struct KWeight {
    shelf: Biquad,
    rlb: Biquad,
}

impl KWeight {
    fn new(sr: f64) -> Self {
        KWeight {
            shelf: Biquad::high_shelf(1681.97, 4.0, sr),
            rlb: Biquad::hpf(38.14, sr),
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        self.rlb.tick(self.shelf.tick(x))
    }
}

pub struct Leveler {
    sr: f64,
    k_left: KWeight,
    k_right: KWeight,
    block_len: usize,
    block_count: usize,
    block_sum: f64,
    history: Ring<HISTORY_BLOCKS>,
    window: Ring<WINDOW_BLOCKS>,
    /// True while the measurement runs (switched off at unity it idles).
    measuring: bool,
    /// Gain the leveler asks for, dB.
    wanted_db: f64,
    /// Gain applied now, dB (f64 so 100 000s of steps stay exact), and its
    /// linear value.
    gain_db: f64,
    gain_lin: f32,
    step_db: f64,
}

impl Leveler {
    pub fn new(sr: f64) -> Self {
        let block_len = ((sr * BLOCK_MS / 1000.0).round() as usize).max(1);
        Leveler {
            sr,
            k_left: KWeight::new(sr),
            k_right: KWeight::new(sr),
            block_len,
            block_count: 0,
            block_sum: 0.0,
            history: Ring::new(),
            window: Ring::new(),
            measuring: false,
            wanted_db: 0.0,
            gain_db: 0.0,
            gain_lin: 1.0,
            step_db: LEVELER_RATE_DB_PER_S as f64 / sr,
        }
    }

    /// Gain applied at the end of the last block, dB (0 when idle).
    pub fn gain_db(&self) -> f32 {
        self.gain_db as f32
    }

    /// Level `left`/`right` in place. `on` and `target_lufs` are read once
    /// per block by the caller; the target is clamped to its range.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], on: bool, target_lufs: f32) {
        let n = left.len().min(right.len());
        if !on && self.gain_db == 0.0 {
            // Off at unity: bit-exact passthrough, and the measurement
            // restarts fresh when switched back on.
            self.idle();
            return;
        }
        if !self.measuring {
            self.measuring = true;
        }
        let target = clamp_target(target_lufs);
        if !on {
            self.wanted_db = 0.0;
        }
        for i in 0..n {
            let kl = self.k_left.tick(left[i]) as f64;
            let kr = self.k_right.tick(right[i]) as f64;
            self.block_sum += kl * kl + kr * kr;
            self.block_count += 1;
            if self.block_count == self.block_len {
                self.end_block(on, target);
            }
            self.slew();
            left[i] *= self.gain_lin;
            right[i] *= self.gain_lin;
        }
    }

    /// Close one 100 ms block: gate it and update the wanted gain.
    fn end_block(&mut self, on: bool, target: f32) {
        let ms = self.block_sum / self.block_len as f64;
        self.block_sum = 0.0;
        self.block_count = 0;
        if !ms.is_finite() {
            // A NaN/inf sample poisoned the filters: start the measurement
            // again and keep the gain where it is.
            self.reset_measurement();
            return;
        }
        self.history.push(ms);
        self.admit_programme_block(target);
        if !on {
            self.wanted_db = 0.0;
        } else if self.window.filled >= MIN_WINDOW_BLOCKS {
            let measured = ms_to_lufs(self.window.mean());
            let wanted = (target - measured).clamp(-LEVELER_RANGE_DB, LEVELER_RANGE_DB);
            self.wanted_db = wanted as f64;
        }
    }

    /// Judge the block `MOMENTARY_BLOCKS - 1` blocks back: it is programme
    /// if the 400 ms ending with it and the 400 ms starting with it both
    /// clear the gate.
    fn admit_programme_block(&mut self, target: f32) {
        let age = MOMENTARY_BLOCKS - 1;
        if self.history.filled <= age {
            return;
        }
        let gate = target - GATE_BELOW_TARGET_LU;
        let after = ms_to_lufs(self.history.mean_ages(0, age));
        let before = ms_to_lufs(self.history.mean_ages(age, 2 * age));
        if after >= gate && before >= gate {
            self.window.push(self.history.at(age));
        }
    }

    #[inline]
    fn slew(&mut self) {
        if self.gain_db == self.wanted_db {
            return;
        }
        self.gain_db = if self.gain_db < self.wanted_db {
            (self.gain_db + self.step_db).min(self.wanted_db)
        } else {
            (self.gain_db - self.step_db).max(self.wanted_db)
        };
        self.gain_lin = 10f64.powf(self.gain_db / 20.0) as f32;
    }

    fn idle(&mut self) {
        if self.measuring {
            self.measuring = false;
            self.reset_measurement();
            self.wanted_db = 0.0;
        }
    }

    fn reset_measurement(&mut self) {
        self.k_left = KWeight::new(self.sr);
        self.k_right = KWeight::new(self.sr);
        self.block_sum = 0.0;
        self.block_count = 0;
        self.history.clear();
        self.window.clear();
    }
}

fn clamp_target(lufs: f32) -> f32 {
    if lufs.is_nan() {
        LEVELER_TARGET_DEFAULT
    } else {
        lufs.clamp(LEVELER_TARGET_MIN, LEVELER_TARGET_MAX)
    }
}

/// Mean square (channels summed) → LUFS; silence is −70.
fn ms_to_lufs(ms: f64) -> f32 {
    if ms <= 1e-10 {
        return -70.0;
    }
    (10.0 * ms.log10() - 0.691) as f32
}
