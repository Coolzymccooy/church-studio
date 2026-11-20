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
    /// Mon 2 / mon 3 sends (always pre-fader; -90 = off).
    pub send_monitor2_db: f32,
    pub send_monitor3_db: f32,
    pub monitor_post_fader: bool,
    pub voice_chain: bool,
    pub automix: bool,
    pub duck: bool,
    pub duck_depth_db: f32,
    /// Live automix attenuation, positive dB (0 while stopped).
    pub automix_gr_db: f32,
    /// Live auto-duck attenuation, positive dB (0 while stopped).
    pub duck_gr_db: f32,
}

/// One bus in `MixerState`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BusState {
    pub id: String,
    /// Name of the output device this bus plays on; null while the engine
    /// is stopped or no device is set for it.
    pub output: Option<String>,
    pub fader_db: f32,
    pub mute: bool,
    pub limiter_ceiling_db: f32,
    /// Loudness leveler on this bus (a delivery setting; not in scenes).
    pub leveler_enabled: bool,
    pub leveler_target_lufs: f32,
}

/// `mixer_state` / `mixer_load_scene` result. Top level is camelCase.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerState {
    pub input_channels: u32,
    /// Strips right after the hardware ones that carry computer audio
    /// (serialized `computerStrips`): 0, or 2 (left, then right).
    pub computer_strips: u32,
    /// Strips after the hardware and computer ones that carry NDI inputs
    /// (serialized `ndiStrips`); they are the last `ndi_strips` entries of
    /// `strips`.
    pub ndi_strips: u32,
    /// Remote strips of network audio (serialized `netStrips`), right after
    /// the hardware ones.
    pub net_strips: u32,
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
    /// Strip running the voice chain; any value ≥ 32 means none
    /// (`NO_VOICE_STRIP`). Missing in a file = strip 0 (the default).
    #[serde(default)]
    pub voice_strip: usize,
    pub mixer: MixerScene,
}
