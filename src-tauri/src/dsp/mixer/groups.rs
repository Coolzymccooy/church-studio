use super::params::{clamp_or, FADER_MAX_DB, MIN_DB};
use atomic_float::AtomicF32;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

pub const GROUP_COUNT: usize = 8;

#[derive(Default)]
pub struct StripGroups {
    pub dca: AtomicU8,
    pub mute: AtomicU8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Membership {
    pub dca: u8,
    pub mute: u8,
}

impl StripGroups {
    pub fn capture(&self) -> Membership {
        Membership {
            dca: self.dca.load(Relaxed),
            mute: self.mute.load(Relaxed),
        }
    }

    pub fn store(&self, membership: Membership) {
        self.dca.store(membership.dca, Relaxed);
        self.mute.store(membership.mute, Relaxed);
    }
}

pub struct GroupParams {
    pub faders: [AtomicF32; GROUP_COUNT],
    pub dca_muted: AtomicU8,
    pub mute_active: AtomicU8,
}

impl GroupParams {
    pub fn new() -> Self {
        Self {
            faders: std::array::from_fn(|_| AtomicF32::new(0.0)),
            dca_muted: AtomicU8::new(0),
            mute_active: AtomicU8::new(0),
        }
    }

    pub fn capture(&self) -> GroupState {
        GroupState {
            faders: std::array::from_fn(|i| self.faders[i].load(Relaxed)),
            dca_muted: self.dca_muted.load(Relaxed),
            mute_active: self.mute_active.load(Relaxed),
        }
    }

    pub fn store(&self, state: &GroupState) {
        for (field, db) in self.faders.iter().zip(state.faders) {
            field.store(clamp_or(db, MIN_DB, FADER_MAX_DB, 0.0), Relaxed);
        }
        self.dca_muted.store(state.dca_muted, Relaxed);
        self.mute_active.store(state.mute_active, Relaxed);
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GroupState {
    pub faders: [f32; GROUP_COUNT],
    pub dca_muted: u8,
    pub mute_active: u8,
}

impl GroupState {
    pub fn level(&self, groups: u8) -> f32 {
        if groups == 0 {
            return 1.0;
        }
        let mut total = 0.0;
        for (_, db) in self
            .faders
            .into_iter()
            .enumerate()
            .filter(|(i, _)| groups & (1 << i) != 0)
        {
            if db <= MIN_DB {
                return 0.0;
            }
            total += db;
        }
        // Multiple attenuators can take the sum below the fader's travel.
        10f32.powf(total.min(FADER_MAX_DB) / 20.0)
    }

    pub fn muted(&self, membership: Membership) -> bool {
        self.dca_muted & membership.dca != 0 || self.mute_active & membership.mute != 0
    }

    pub fn strip_gain(&self, membership: Membership) -> StripGroupGain {
        StripGroupGain {
            level: self.level(membership.dca),
            muted: self.muted(membership),
        }
    }
}

pub struct StripGroupGain {
    pub level: f32,
    pub muted: bool,
}

impl Default for StripGroupGain {
    fn default() -> Self {
        Self {
            level: 1.0,
            muted: false,
        }
    }
}
