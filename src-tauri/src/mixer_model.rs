//! JSON models of the live mixer: the `mixer_state` result and the on-disk
//! scene file. Field names follow `docs/specs/2026-09-27-mixer-live-contract.md`.
use crate::dsp::mixer::MixerScene;
use serde::{Deserialize, Serialize};

/// One strip in `MixerState` (snake_case keys, identical to the set keys).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StripState {
    pub index: u32,
    pub name: String,
    pub trim_db: f32,
    pub polarity: bool,
    pub hpf_enabled: bool,
    pub hpf_hz: f32,
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
    pub send_main_db: f32,
    pub send_stream_db: f32,
    pub send_monitor_db: f32,
    pub monitor_post_fader: bool,
    pub voice_chain: bool,
}

/// One bus in `MixerState`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BusState {
    pub id: String,
    pub fader_db: f32,
    pub mute: bool,
    pub limiter_ceiling_db: f32,
}

/// `mixer_state` / `mixer_load_scene` result. Top level is camelCase.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerState {
    pub input_channels: u32,
    pub running: bool,
    pub strips: Vec<StripState>,
    pub buses: Vec<BusState>,
    pub scenes: Vec<String>,
}

/// On-disk scene file: the DSP snapshot plus the control-side extras.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredScene {
    #[serde(default)]
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub strip_names: Vec<String>,
    #[serde(default)]
    pub voice_strip: usize,
    pub mixer: MixerScene,
}
