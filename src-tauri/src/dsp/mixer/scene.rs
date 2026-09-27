//! Mixer scenes: a by-value snapshot of every strip and bus control that can
//! be saved as JSON and recalled onto the live `MixerParams`.
use super::params::{BusParams, MixerParams, StripParams, NUM_BUSES};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering::Relaxed;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StripScene {
    pub trim_db: f32,
    pub polarity_invert: bool,
    pub hpf_enabled: bool,
    pub hpf_freq_hz: f32,
    pub gate_enabled: bool,
    pub gate_threshold_db: f32,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub comp_enabled: bool,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub fader_db: f32,
    pub send_db: Vec<f32>,
    pub monitor_post_fader: bool,
}

impl StripScene {
    fn capture(p: &StripParams) -> Self {
        StripScene {
            trim_db: p.trim_db.load(Relaxed),
            polarity_invert: p.polarity_invert.load(Relaxed),
            hpf_enabled: p.hpf_enabled.load(Relaxed),
            hpf_freq_hz: p.hpf_freq_hz.load(Relaxed),
            gate_enabled: p.gate_enabled.load(Relaxed),
            gate_threshold_db: p.gate_threshold_db.load(Relaxed),
            eq_low_db: p.eq_low_db.load(Relaxed),
            eq_mid_db: p.eq_mid_db.load(Relaxed),
            eq_high_db: p.eq_high_db.load(Relaxed),
            comp_enabled: p.comp_enabled.load(Relaxed),
            comp_threshold_db: p.comp_threshold_db.load(Relaxed),
            comp_ratio: p.comp_ratio.load(Relaxed),
            pan: p.pan.load(Relaxed),
            mute: p.mute.load(Relaxed),
            solo: p.solo.load(Relaxed),
            fader_db: p.fader_db.load(Relaxed),
            send_db: p.send_db.iter().map(|s| s.load(Relaxed)).collect(),
            monitor_post_fader: p.monitor_post_fader.load(Relaxed),
        }
    }

    fn apply(&self, p: &StripParams) {
        p.trim_db.store(self.trim_db, Relaxed);
        p.polarity_invert.store(self.polarity_invert, Relaxed);
        p.hpf_enabled.store(self.hpf_enabled, Relaxed);
        p.hpf_freq_hz.store(self.hpf_freq_hz, Relaxed);
        p.gate_enabled.store(self.gate_enabled, Relaxed);
        p.gate_threshold_db.store(self.gate_threshold_db, Relaxed);
        p.eq_low_db.store(self.eq_low_db, Relaxed);
        p.eq_mid_db.store(self.eq_mid_db, Relaxed);
        p.eq_high_db.store(self.eq_high_db, Relaxed);
        p.comp_enabled.store(self.comp_enabled, Relaxed);
        p.comp_threshold_db.store(self.comp_threshold_db, Relaxed);
        p.comp_ratio.store(self.comp_ratio, Relaxed);
        p.pan.store(self.pan, Relaxed);
        p.mute.store(self.mute, Relaxed);
        p.solo.store(self.solo, Relaxed);
        p.fader_db.store(self.fader_db, Relaxed);
        for (dst, &src) in p.send_db.iter().zip(self.send_db.iter()) {
            dst.store(src, Relaxed);
        }
        p.monitor_post_fader.store(self.monitor_post_fader, Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusScene {
    pub fader_db: f32,
    pub mute: bool,
    pub limiter_enabled: bool,
    pub limiter_ceiling_db: f32,
}

impl BusScene {
    fn capture(p: &BusParams) -> Self {
        BusScene {
            fader_db: p.fader_db.load(Relaxed),
            mute: p.mute.load(Relaxed),
            limiter_enabled: p.limiter_enabled.load(Relaxed),
            limiter_ceiling_db: p.limiter_ceiling_db.load(Relaxed),
        }
    }

    fn apply(&self, p: &BusParams) {
        p.fader_db.store(self.fader_db, Relaxed);
        p.mute.store(self.mute, Relaxed);
        p.limiter_enabled.store(self.limiter_enabled, Relaxed);
        p.limiter_ceiling_db.store(self.limiter_ceiling_db, Relaxed);
    }
}

/// Whole-console snapshot. Recalling it onto live params is click-free
/// because the mixer ramps every gain toward its new target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MixerScene {
    pub name: String,
    pub strips: Vec<StripScene>,
    pub buses: Vec<BusScene>,
}

impl MixerScene {
    pub fn capture(params: &MixerParams) -> Self {
        Self::capture_named(params, "")
    }

    pub fn capture_named(params: &MixerParams, name: &str) -> Self {
        MixerScene {
            name: name.to_string(),
            strips: params.strips.iter().map(StripScene::capture).collect(),
            buses: params.buses.iter().map(BusScene::capture).collect(),
        }
    }

    /// Store the scene into `params`. Extra strips/buses in the scene are
    /// ignored; strips missing from the scene are left unchanged.
    pub fn apply(&self, params: &MixerParams) {
        for (scene, p) in self.strips.iter().zip(params.strips.iter()) {
            scene.apply(p);
        }
        for (scene, p) in self.buses.iter().zip(params.buses.iter()).take(NUM_BUSES) {
            scene.apply(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::mixer::params::{BUS_MONITOR, BUS_STREAM};

    #[test]
    fn serde_round_trip_and_apply() {
        let params = MixerParams::new(4);
        params.strips[1].fader_db.store(-6.5, Relaxed);
        params.strips[1].solo.store(true, Relaxed);
        params.strips[2].eq_mid_db.store(4.0, Relaxed);
        params.strips[3].send_db[BUS_MONITOR].store(-3.0, Relaxed);
        params.buses[BUS_STREAM].limiter_ceiling_db.store(-2.0, Relaxed);

        let scene = MixerScene::capture_named(&params, "Sermon");
        let json = serde_json::to_string(&scene).expect("serialize");
        let back: MixerScene = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(scene, back);

        let fresh = MixerParams::new(4);
        back.apply(&fresh);
        assert_eq!(MixerScene::capture_named(&fresh, "Sermon"), scene);
        assert_eq!(fresh.strips[1].fader_db.load(Relaxed), -6.5);
        assert!(fresh.strips[1].solo.load(Relaxed));
        assert_eq!(fresh.buses[BUS_STREAM].limiter_ceiling_db.load(Relaxed), -2.0);
    }
}
