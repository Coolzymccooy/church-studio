//! Automixer (Dugan-style gain sharing) and auto-ducking.
//! Design: `docs/specs/2026-11-19-automix-ducking-design.md`.
//!
//! Strips with `automix` on form one group that shares a constant total
//! power gain: `g_i = sqrt(w_i / Σ w_j)`, where `w_i` is the strip's smoothed
//! power times its speech factor (RNNoise VAD side-chain at 48 kHz, analysis
//! only). While no group strip is 15 dB over its own noise floor the shares
//! are frozen: last-mic hold. Strips with `duck` on dip by
//! `duck_depth_db` while any group strip is speaking.
//!
//! Real time: everything is sized in `new`; `process` neither allocates nor
//! locks, with one exception made harmless: nnnoiseless runs its FFTs through
//! easyfft, which keeps the FFT planner and scratch per THREAD and sets them
//! up on that thread's first frame. `process` therefore runs one silent VAD
//! frame in the engine's first block (`warm_up`), so that one-time set-up
//! happens at engine start, never when the first talker starts mid-service. All state advances per sample (VAD on fixed 480-sample frames,
//! noise floor on fixed 10 ms frames), so the block size never changes the
//! result.
use super::params::{clamp_or, StripParams, DUCK_DEPTH_DEFAULT_DB, DUCK_DEPTH_MAX_DB, MIN_DB};
use nnnoiseless::DenoiseState;
use std::sync::atomic::Ordering::Relaxed;

/// RNNoise frame (10 ms at 48 kHz).
const VAD_FRAME: usize = 480;
const VAD_SAMPLE_RATE: f64 = 48_000.0;
const I16_SCALE: f32 = 32_768.0;
/// VAD probability above which a frame counts as speech.
const VAD_SPEECH: f32 = 0.5;
/// Speech stays "on" this long after the last speech frame.
const SPEECH_HOLD_S: f64 = 0.3;
/// Weight multiplier while the VAD hears no speech (≈ 7 dB handicap).
const NO_SPEECH_FACTOR: f32 = 0.2;

/// Level envelope (power) time constants.
const ENV_ATTACK_S: f64 = 0.005;
const ENV_RELEASE_S: f64 = 0.080;
/// Automix gain smoothing.
const GAIN_UP_S: f64 = 0.005;
const GAIN_DOWN_S: f64 = 0.150;
/// Deepest automix attenuation (−40 dB).
const MIN_AUTOMIX_GAIN: f32 = 0.01;

/// Noise floor: rolling minimum of 10 ms frame powers over 8 × 250 ms.
const FLOOR_FRAME_S: f64 = 0.010;
const FLOOR_FRAMES_PER_SUB: usize = 25;
const FLOOR_SUBS: usize = 8;
/// Floor never goes below −100 dBFS (power).
const FLOOR_MIN: f32 = 1e-10;
/// "Active" (counts for speaking and ducking) = 10 dB over the floor and
/// above −80 dBFS.
const ACTIVE_OVER_FLOOR: f32 = 10.0;
/// The shares follow the mics only while some member is 15 dB over its
/// floor; below that (the fading tail of the last word, room noise) they
/// stay frozen, so the hold keeps the talker's balance, not the tail's.
const SHARE_OVER_FLOOR: f32 = 31.6;
const ACTIVE_MIN: f32 = 1e-8;
/// Added to the group sum so silence never divides by zero.
const SUM_EPS: f32 = 1e-12;

/// Ducking: amount envelope 0 → 1 (fully dipped).
const DUCK_ATTACK_S: f64 = 0.025;
const DUCK_HOLD_S: f64 = 0.5;
const DUCK_RELEASE_S: f64 = 0.4;

const DB_PER_NEPER: f32 = 8.685_889_6; // 20 / ln(10)

fn one_pole(seconds: f64, sr: f64) -> f32 {
    let n = seconds * sr;
    if n.is_finite() && n > 0.0 {
        (1.0 - (-1.0 / n).exp()) as f32
    } else {
        1.0
    }
}

fn samples(seconds: f64, sr: f64) -> u32 {
    let n = (seconds * sr).round();
    if n.is_finite() && n >= 1.0 {
        n as u32
    } else {
        1
    }
}

fn lin_to_db(gain: f32) -> f32 {
    if gain > 1e-6 {
        20.0 * gain.log10()
    } else {
        -120.0
    }
}

/// Per-strip level, noise floor and speech detector.
struct Detector {
    env: f32,
    env_attack: f32,
    env_release: f32,

    frame_len: u32,
    frame_pos: u32,
    frame_acc: f32,
    sub_min: f32,
    sub_frames: usize,
    subs: [f32; FLOOR_SUBS],
    sub_idx: usize,
    floor: f32,

    vad: Option<Box<DenoiseState<'static>>>,
    vad_in: Vec<f32>,
    vad_out: Vec<f32>,
    vad_pos: usize,
    /// Test hook: a fixed VAD probability instead of the network.
    vad_override: Option<f32>,
    speech_hold: u32,
    speech_hold_len: u32,
}

impl Detector {
    fn new(sr: f64) -> Self {
        let vad = if (sr - VAD_SAMPLE_RATE).abs() < 0.5 {
            Some(DenoiseState::new())
        } else {
            None
        };
        Detector {
            env: 0.0,
            env_attack: one_pole(ENV_ATTACK_S, sr),
            env_release: one_pole(ENV_RELEASE_S, sr),
            frame_len: samples(FLOOR_FRAME_S, sr),
            frame_pos: 0,
            frame_acc: 0.0,
            sub_min: f32::MAX,
            sub_frames: 0,
            subs: [f32::MAX; FLOOR_SUBS],
            sub_idx: 0,
            floor: FLOOR_MIN,
            vad,
            vad_in: vec![0.0; VAD_FRAME],
            vad_out: vec![0.0; VAD_FRAME],
            vad_pos: 0,
            vad_override: None,
            speech_hold: 0,
            speech_hold_len: samples(SPEECH_HOLD_S, sr),
        }
    }

    /// True when a speech probability is available (48 kHz or test hook).
    fn has_vad(&self) -> bool {
        self.vad.is_some() || self.vad_override.is_some()
    }

    /// Advance one sample. The VAD runs only when `run_vad` (group member).
    #[inline]
    fn tick(&mut self, x: f32, run_vad: bool) {
        let x = if x.is_finite() { x } else { 0.0 };
        let p = x * x;
        let coef = if p > self.env { self.env_attack } else { self.env_release };
        self.env += (p - self.env) * coef;

        self.frame_acc += p;
        self.frame_pos += 1;
        if self.frame_pos >= self.frame_len {
            self.end_floor_frame();
        }

        if self.speech_hold > 0 {
            self.speech_hold -= 1;
        }
        if run_vad && self.has_vad() {
            self.vad_in[self.vad_pos] = x * I16_SCALE;
            self.vad_pos += 1;
            if self.vad_pos == VAD_FRAME {
                self.vad_pos = 0;
                self.end_vad_frame();
            }
        }
    }

    fn end_floor_frame(&mut self) {
        let mean = self.frame_acc / self.frame_len as f32;
        self.frame_acc = 0.0;
        self.frame_pos = 0;
        self.sub_min = self.sub_min.min(mean);
        self.sub_frames += 1;
        if self.sub_frames >= FLOOR_FRAMES_PER_SUB {
            self.subs[self.sub_idx] = self.sub_min;
            self.sub_idx = (self.sub_idx + 1) % FLOOR_SUBS;
            self.sub_min = f32::MAX;
            self.sub_frames = 0;
        }
        let min = self.subs.iter().fold(self.sub_min, |m, &s| m.min(s));
        self.floor = if min.is_finite() { min.max(FLOOR_MIN) } else { FLOOR_MIN };
    }

    fn end_vad_frame(&mut self) {
        let prob = match (self.vad_override, self.vad.as_mut()) {
            (Some(p), _) => p,
            (None, Some(state)) => state.process_frame(&mut self.vad_out[..], &self.vad_in[..]),
            (None, None) => 0.0,
        };
        if prob.is_finite() && prob > VAD_SPEECH {
            self.speech_hold = self.speech_hold_len;
        }
    }

    fn active(&self) -> bool {
        self.env > ACTIVE_MIN && self.env > self.floor * ACTIVE_OVER_FLOOR
    }

    /// Loud enough over the floor to steer the shares.
    fn steering(&self) -> bool {
        self.env > ACTIVE_MIN && self.env > self.floor * SHARE_OVER_FLOOR
    }

    fn speech(&self) -> bool {
        self.speech_hold > 0
    }

    /// Active and (no VAD, or the VAD hears speech).
    fn speaking(&self) -> bool {
        self.active() && (!self.has_vad() || self.speech())
    }

    fn weight(&self) -> f32 {
        let factor = if !self.has_vad() || self.speech() { 1.0 } else { NO_SPEECH_FACTOR };
        self.env * factor
    }
}

/// Per-strip automix/duck state and block readouts.
struct Slot {
    det: Detector,
    /// Smoothed automix gain (linear).
    gain: f32,
    /// Power share captured while the group was active (last-mic hold).
    held_share: f32,
    member: bool,
    /// `automix` on but muted or faded out: out of the group, with its gain
    /// and held share frozen so muting and unmuting never swell its bleed.
    paused: bool,
    duck: bool,
    /// Duck depth in nepers-per-dB form: gain = exp(-depth_np × amount).
    duck_depth_np: f32,
    /// Combined automix × duck gain for each sample of the block.
    out: Vec<f32>,
    min_automix: f32,
    min_duck: f32,
    speaking: bool,
}

/// Readouts for one strip after a block (dB ≤ 0).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutoReadout {
    pub automix_gr_db: f32,
    pub duck_gr_db: f32,
    pub speech: bool,
}

pub struct AutoMix {
    slots: Vec<Slot>,
    gain_up: f32,
    gain_down: f32,
    duck_amount: f32,
    duck_attack: f32,
    duck_release: f32,
    duck_hold: u32,
    duck_hold_len: u32,
    any_duck: bool,
    any_member: bool,
    warmed: bool,
}

impl AutoMix {
    pub fn new(sr: f64, max_block: usize, num_strips: usize) -> Self {
        let max_block = max_block.max(1);
        AutoMix {
            slots: (0..num_strips)
                .map(|_| Slot {
                    det: Detector::new(sr),
                    gain: 1.0,
                    held_share: 0.0,
                    member: false,
                    paused: false,
                    duck: false,
                    duck_depth_np: 0.0,
                    out: vec![1.0; max_block],
                    min_automix: 1.0,
                    min_duck: 1.0,
                    speaking: false,
                })
                .collect(),
            gain_up: one_pole(GAIN_UP_S, sr),
            gain_down: one_pole(GAIN_DOWN_S, sr),
            duck_amount: 0.0,
            duck_attack: one_pole(DUCK_ATTACK_S, sr),
            duck_release: one_pole(DUCK_RELEASE_S, sr),
            duck_hold: 0,
            duck_hold_len: samples(DUCK_HOLD_S, sr),
            any_duck: false,
            any_member: false,
            warmed: false,
        }
    }

    /// Run one silent VAD frame so the per-thread FFT set-up inside
    /// nnnoiseless/easyfft happens now (see the module docs).
    fn warm_up(&mut self) {
        self.warmed = true;
        if let Some(det) = self.slots.iter_mut().find(|s| s.det.vad.is_some()).map(|s| &mut s.det) {
            if let Some(state) = det.vad.as_mut() {
                let silence = [0.0f32; VAD_FRAME];
                state.process_frame(&mut det.vad_out[..], &silence);
            }
        }
    }

    /// True once the engine's first block has run the VAD warm-up.
    pub fn warmed(&self) -> bool {
        self.warmed
    }

    /// Test hook: force strip `index`'s VAD probability (`None` = network).
    pub fn set_vad_override(&mut self, index: usize, prob: Option<f32>) {
        if let Some(slot) = self.slots.get_mut(index) {
            slot.det.vad_override = prob;
        }
    }

    /// Read the strip flags for this block. Muted strips and strips with the
    /// fader off leave the group; `duck` wins over `automix`.
    fn sync(&mut self, params: &[StripParams]) {
        self.any_duck = false;
        self.any_member = false;
        for (slot, p) in self.slots.iter_mut().zip(params.iter()) {
            let duck = p.duck.load(Relaxed);
            let audible = !p.mute.load(Relaxed) && p.fader_db.load(Relaxed) > MIN_DB;
            let automix = p.automix.load(Relaxed) && !duck;
            slot.duck = duck;
            slot.member = automix && audible;
            slot.paused = automix && !audible;
            if !automix {
                slot.held_share = 0.0;
            }
            let depth = clamp_or(
                p.duck_depth_db.load(Relaxed),
                0.0,
                DUCK_DEPTH_MAX_DB,
                DUCK_DEPTH_DEFAULT_DB,
            );
            slot.duck_depth_np = depth / DB_PER_NEPER;
            slot.min_automix = 1.0;
            slot.min_duck = 1.0;
            self.any_duck |= duck;
            self.any_member |= slot.member;
        }
    }

    /// Compute the per-sample gains of this block. `detect(i)` is strip `i`'s
    /// detection signal (after trim and HPF), at least `n` samples long.
    pub fn process<'a>(
        &mut self,
        params: &[StripParams],
        n: usize,
        detect: impl Fn(usize) -> &'a [f32],
    ) {
        if !self.warmed {
            self.warm_up();
        }
        self.sync(params);
        let n = n.min(self.slots.first().map(|s| s.out.len()).unwrap_or(0));
        for t in 0..n {
            self.tick(t, &detect);
        }
        for slot in self.slots.iter_mut() {
            slot.speaking = slot.member && slot.det.speaking();
        }
    }

    #[inline]
    fn tick<'a>(&mut self, t: usize, detect: &impl Fn(usize) -> &'a [f32]) {
        let mut sum = 0.0f32;
        let mut any_steering = false;
        let mut any_speaking = false;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let x = detect(i).get(t).copied().unwrap_or(0.0);
            slot.det.tick(x, slot.member);
            if slot.member {
                sum += slot.det.weight();
                any_steering |= slot.det.steering();
                any_speaking |= slot.det.speaking();
            }
        }
        self.update_shares(sum, any_steering);
        self.update_duck(any_speaking);

        let duck_amount = self.duck_amount;
        for slot in self.slots.iter_mut() {
            let duck = if slot.duck && duck_amount > 0.0 {
                (-slot.duck_depth_np * duck_amount).exp()
            } else {
                1.0
            };
            slot.min_automix = slot.min_automix.min(slot.gain);
            slot.min_duck = slot.min_duck.min(duck);
            slot.out[t] = slot.gain * duck;
        }
    }

    /// Move each automix gain toward its share (live while a member steers,
    /// else held: last-mic hold). Non-members glide back to unity; paused
    /// members keep their gain.
    fn update_shares(&mut self, sum: f32, live: bool) {
        if !self.any_member {
            for slot in self.slots.iter_mut().filter(|s| !s.paused) {
                slot.gain += (1.0 - slot.gain) * self.gain_up;
            }
            return;
        }
        let total = sum + SUM_EPS;
        let mut held_sum = 0.0f32;
        let mut members = 0usize;
        for slot in self.slots.iter_mut().filter(|s| s.member) {
            if live {
                slot.held_share = slot.det.weight() / total;
            }
            held_sum += slot.held_share;
            members += 1;
        }
        let equal = 1.0 / members.max(1) as f32;
        for slot in self.slots.iter_mut().filter(|s| !s.paused) {
            let target = if !slot.member {
                1.0
            } else if held_sum > SUM_EPS {
                (slot.held_share / held_sum).sqrt().max(MIN_AUTOMIX_GAIN)
            } else {
                equal.sqrt()
            };
            let coef = if target > slot.gain { self.gain_up } else { self.gain_down };
            slot.gain += (target - slot.gain) * coef;
        }
    }

    fn update_duck(&mut self, any_speaking: bool) {
        if any_speaking {
            self.duck_hold = self.duck_hold_len;
        } else if self.duck_hold > 0 {
            self.duck_hold -= 1;
        }
        let (target, coef) = if self.duck_hold > 0 {
            (1.0, self.duck_attack)
        } else {
            (0.0, self.duck_release)
        };
        self.duck_amount += (target - self.duck_amount) * coef;
        if self.duck_amount < 1e-6 {
            self.duck_amount = 0.0;
        }
    }

    /// Strip `i`'s combined automix × duck gain for the last block.
    pub fn gains(&self, i: usize, n: usize) -> &[f32] {
        match self.slots.get(i) {
            Some(slot) => &slot.out[..n.min(slot.out.len())],
            None => &[],
        }
    }

    pub fn readout(&self, i: usize) -> AutoReadout {
        match self.slots.get(i) {
            Some(slot) => AutoReadout {
                automix_gr_db: lin_to_db(slot.min_automix).min(0.0),
                duck_gr_db: lin_to_db(slot.min_duck).min(0.0),
                speech: slot.speaking,
            },
            None => AutoReadout::default(),
        }
    }
}
