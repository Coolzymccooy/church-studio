use super::params::{clamp_or, FADER_MAX_DB, MIN_DB, NUM_BUSES};
use atomic_float::AtomicF32;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

pub const FX_COUNT: usize = 2;
pub const FX_IDS: [&str; FX_COUNT] = ["fx_a", "fx_b"];
pub const FX_SEND_KEYS: [&str; FX_COUNT] = ["send_fx_a_db", "send_fx_b_db"];
pub const OUTPUT_SEND_KEYS: [&str; NUM_BUSES] = [
    "send_main_db",
    "send_stream_db",
    "send_monitor_db",
    "send_monitor2_db",
    "send_monitor3_db",
];
pub const RETURN_SEND_DEFAULTS: [f32; NUM_BUSES] = [0.0, 0.0, MIN_DB, MIN_DB, MIN_DB];
pub const DECAY: (f32, f32, f32) = (0.3, 5.0, 1.8);
pub const TEMPO: (f32, f32, f32) = (40.0, 240.0, 120.0);
pub const FEEDBACK: (f32, f32, f32) = (0.0, 80.0, 25.0);

pub fn bound(value: f32, (min, max, default): (f32, f32, f32)) -> f32 {
    clamp_or(value, min, max, default)
}

pub struct FxParams {
    pub returns: [ReturnParams; FX_COUNT],
    pub decay: AtomicF32,
    pub tempo: AtomicF32,
    pub feedback: AtomicF32,
}

impl FxParams {
    pub fn new() -> Self {
        Self {
            returns: std::array::from_fn(|_| ReturnParams::new()),
            decay: AtomicF32::new(DECAY.2),
            tempo: AtomicF32::new(TEMPO.2),
            feedback: AtomicF32::new(FEEDBACK.2),
        }
    }
}

pub struct ReturnParams {
    pub fader: AtomicF32,
    pub sends: [AtomicF32; NUM_BUSES],
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub monitor_post: AtomicBool,
}

impl ReturnParams {
    fn new() -> Self {
        Self {
            fader: AtomicF32::new(0.0),
            sends: RETURN_SEND_DEFAULTS.map(AtomicF32::new),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            monitor_post: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReturnScene {
    pub fader: f32,
    pub sends: [f32; NUM_BUSES],
    pub mute: bool,
    pub solo: bool,
    pub monitor_post: bool,
}

impl ReturnScene {
    fn capture(params: &ReturnParams) -> Self {
        Self {
            fader: params.fader.load(Relaxed),
            sends: std::array::from_fn(|i| params.sends[i].load(Relaxed)),
            mute: params.mute.load(Relaxed),
            solo: params.solo.load(Relaxed),
            monitor_post: params.monitor_post.load(Relaxed),
        }
    }

    fn apply(&self, params: &ReturnParams) {
        params
            .fader
            .store(clamp_or(self.fader, MIN_DB, FADER_MAX_DB, 0.0), Relaxed);
        for ((param, value), default) in params
            .sends
            .iter()
            .zip(self.sends)
            .zip(RETURN_SEND_DEFAULTS)
        {
            param.store(clamp_or(value, MIN_DB, FADER_MAX_DB, default), Relaxed);
        }
        params.mute.store(self.mute, Relaxed);
        params.solo.store(self.solo, Relaxed);
        params.monitor_post.store(self.monitor_post, Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FxScene {
    pub returns: [ReturnScene; FX_COUNT],
    pub decay: f32,
    pub tempo: f32,
    pub feedback: f32,
}

impl Default for FxScene {
    fn default() -> Self {
        Self::capture(&FxParams::new())
    }
}

impl FxScene {
    pub fn capture(params: &FxParams) -> Self {
        Self {
            returns: std::array::from_fn(|i| ReturnScene::capture(&params.returns[i])),
            decay: params.decay.load(Relaxed),
            tempo: params.tempo.load(Relaxed),
            feedback: params.feedback.load(Relaxed),
        }
    }

    pub fn apply(&self, params: &FxParams) {
        for (scene, params) in self.returns.iter().zip(&params.returns) {
            scene.apply(params);
        }
        params.decay.store(bound(self.decay, DECAY), Relaxed);
        params.tempo.store(bound(self.tempo, TEMPO), Relaxed);
        params
            .feedback
            .store(bound(self.feedback, FEEDBACK), Relaxed);
    }
}
