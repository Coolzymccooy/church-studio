//! Control side of the live mixer (`docs/specs/2026-09-27-mixer-live-contract.md`).
//!
//! `MixerControl` is Tauri-managed state that exists whether or not the audio
//! engine runs, so the UI can prepare a mix while stopped. It owns:
//! - the shared `Arc<MixerParams>` (atomics only; the audio thread reads it),
//! - the index of the strip carrying the voice chain (`AtomicUsize`, read by
//!   the audio thread),
//! - strip names (control side only; never touched by the audio thread).
//!
//! Everything here is pure (no Tauri, no files) so it can be unit-tested.
use crate::dsp::mixer::params::{
    clamp_or, BusParams, StripParams, BUS_MAIN, BUS_MONITOR, BUS_STREAM, NUM_BUSES,
};
use crate::dsp::mixer::{MixerParams, MixerScene};
pub use crate::mixer_model::{BusState, MixerState, StoredScene, StripState};
use atomic_float::AtomicF32;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;

/// Strips the mixer is sized for (inputs beyond this are ignored).
pub const MAX_STRIPS: usize = 32;
/// Bus ids in `BUS_*` index order.
pub const BUS_IDS: [&str; NUM_BUSES] = ["main", "stream", "monitor"];
pub const MAX_STRIP_NAME_CHARS: usize = 24;
pub const MAX_SCENE_NAME_CHARS: usize = 40;
/// `voice_strip` value meaning "no strip runs the voice chain".
pub const NO_VOICE_STRIP: usize = usize::MAX;
/// Version tag written into scene files.
pub const SCENE_FILE_VERSION: u32 = 1;

/// One f32 key: (name, min, max, default), as listed in the contract.
type F32Key = (&'static str, f32, f32, f32);

const STRIP_F32_KEYS: &[F32Key] = &[
    ("trim_db", -20.0, 40.0, 0.0),
    ("hpf_hz", 20.0, 400.0, 80.0),
    ("gate_threshold_db", -80.0, 0.0, -45.0),
    ("eq_low_db", -15.0, 15.0, 0.0),
    ("eq_mid_db", -15.0, 15.0, 0.0),
    ("eq_high_db", -15.0, 15.0, 0.0),
    ("comp_threshold_db", -40.0, 0.0, -18.0),
    ("comp_ratio", 1.0, 20.0, 3.0),
    ("pan", -1.0, 1.0, 0.0),
    ("fader_db", -90.0, 10.0, 0.0),
    ("send_main_db", -90.0, 10.0, 0.0),
    ("send_stream_db", -90.0, 10.0, 0.0),
    ("send_monitor_db", -90.0, 10.0, 0.0),
];

/// Bool strip keys stored in `StripParams` with their defaults
/// (`voice_chain` is handled separately).
const STRIP_BOOL_KEYS: [(&str, bool); 7] = [
    ("polarity", false),
    ("hpf_enabled", true),
    ("gate_enabled", false),
    ("comp_enabled", false),
    ("mute", false),
    ("solo", false),
    ("monitor_post_fader", false),
];

const BUS_F32_KEYS: &[F32Key] = &[
    ("fader_db", -90.0, 10.0, 0.0),
    ("limiter_ceiling_db", -12.0, 0.0, -1.0),
];

fn f32_spec(specs: &[F32Key], key: &str) -> Option<F32Key> {
    specs.iter().find(|s| s.0 == key).copied()
}

/// Clamp `value` to a key's range (NaN becomes the default).
fn clamp_key(spec: F32Key, value: f32) -> f32 {
    clamp_or(value, spec.1, spec.2, spec.3)
}

fn strip_f32<'a>(p: &'a StripParams, key: &str) -> Option<&'a AtomicF32> {
    let field = match key {
        "trim_db" => &p.trim_db,
        "hpf_hz" => &p.hpf_freq_hz,
        "gate_threshold_db" => &p.gate_threshold_db,
        "eq_low_db" => &p.eq_low_db,
        "eq_mid_db" => &p.eq_mid_db,
        "eq_high_db" => &p.eq_high_db,
        "comp_threshold_db" => &p.comp_threshold_db,
        "comp_ratio" => &p.comp_ratio,
        "pan" => &p.pan,
        "fader_db" => &p.fader_db,
        "send_main_db" => &p.send_db[BUS_MAIN],
        "send_stream_db" => &p.send_db[BUS_STREAM],
        "send_monitor_db" => &p.send_db[BUS_MONITOR],
        _ => return None,
    };
    Some(field)
}

fn strip_bool<'a>(p: &'a StripParams, key: &str) -> Option<&'a AtomicBool> {
    let field = match key {
        "polarity" => &p.polarity_invert,
        "hpf_enabled" => &p.hpf_enabled,
        "gate_enabled" => &p.gate_enabled,
        "comp_enabled" => &p.comp_enabled,
        "mute" => &p.mute,
        "solo" => &p.solo,
        "monitor_post_fader" => &p.monitor_post_fader,
        _ => return None,
    };
    Some(field)
}

fn bus_f32<'a>(p: &'a BusParams, key: &str) -> Option<&'a AtomicF32> {
    match key {
        "fader_db" => Some(&p.fader_db),
        "limiter_ceiling_db" => Some(&p.limiter_ceiling_db),
        _ => None,
    }
}

/// Bus id ("main" | "stream" | "monitor") → `BUS_*` index.
pub fn bus_index(id: &str) -> Result<usize, String> {
    BUS_IDS
        .iter()
        .position(|b| *b == id)
        .ok_or_else(|| format!("unknown bus '{id}' (expected main, stream or monitor)"))
}

pub fn default_strip_name(index: usize) -> String {
    format!("Ch {}", index + 1)
}

/// Fader of strips 2 and up at start: off (−inf), so a 2-channel mic keeps
/// today's sound (only channel 1 is heard) until the operator raises it.
pub const OTHER_STRIPS_FADER_DB: f32 = -90.0;

/// Default fader for strip `index`: 0 dB on strip 0, off on the others.
pub fn default_fader_db(index: usize) -> f32 {
    if index == 0 {
        0.0
    } else {
        OTHER_STRIPS_FADER_DB
    }
}

/// Contract defaults for strip `index` (differs from `StripParams::new()`:
/// HPF on, gate threshold −45 dB, Monitor send 0 dB pre-fader, and every
/// strip but the first with its fader off).
fn reset_strip(index: usize, p: &StripParams) {
    for spec in STRIP_F32_KEYS.iter() {
        if let Some(field) = strip_f32(p, spec.0) {
            field.store(spec.3, Relaxed);
        }
    }
    p.fader_db.store(default_fader_db(index), Relaxed);
    for (key, default) in STRIP_BOOL_KEYS.iter() {
        if let Some(field) = strip_bool(p, key) {
            field.store(*default, Relaxed);
        }
    }
}

fn reset_bus(p: &BusParams) {
    p.fader_db.store(0.0, Relaxed);
    p.mute.store(false, Relaxed);
    p.limiter_enabled.store(true, Relaxed);
    p.limiter_ceiling_db.store(-1.0, Relaxed);
}

/// Re-clamp every value (used after loading a scene file, which may carry
/// out-of-range or NaN values).
fn sanitize_strip(index: usize, p: &StripParams) {
    // A NaN fader falls back to this strip's own default (off above strip 0).
    let fader_nan = p.fader_db.load(Relaxed).is_nan();
    for spec in STRIP_F32_KEYS.iter() {
        if let Some(field) = strip_f32(p, spec.0) {
            field.store(clamp_key(*spec, field.load(Relaxed)), Relaxed);
        }
    }
    if fader_nan {
        p.fader_db.store(default_fader_db(index), Relaxed);
    }
}

fn sanitize_bus(p: &BusParams) {
    for spec in BUS_F32_KEYS.iter() {
        if let Some(field) = bus_f32(p, spec.0) {
            field.store(clamp_key(*spec, field.load(Relaxed)), Relaxed);
        }
    }
}

/// Trim, bound and check a strip name. Empty means "use the default name".
pub fn clean_strip_name(index: usize, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(default_strip_name(index));
    }
    if name.chars().count() > MAX_STRIP_NAME_CHARS {
        return Err(format!(
            "strip name is longer than {MAX_STRIP_NAME_CHARS} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("strip name contains control characters".to_string());
    }
    Ok(name.to_string())
}

/// Validate a scene name and turn it into a file-name slug.
///
/// Names are 1–40 characters (after trimming) of ASCII letters, digits,
/// space, `-` and `_`. The slug is lowercase with spaces replaced by `-`.
/// Anything else (dots, slashes, unicode) is rejected, so a slug can never
/// escape the scenes directory. Windows device names are rejected too.
pub fn scene_slug(name: &str) -> Result<String, String> {
    let name = name.trim();
    let len = name.chars().count();
    if len == 0 {
        return Err("scene name is empty".to_string());
    }
    if len > MAX_SCENE_NAME_CHARS {
        return Err(format!(
            "scene name is longer than {MAX_SCENE_NAME_CHARS} characters"
        ));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_');
    if !valid {
        return Err(
            "scene name may only contain letters, digits, spaces, '-' and '_'".to_string(),
        );
    }
    let slug: String = name
        .chars()
        .map(|c| if c == ' ' { '-' } else { c.to_ascii_lowercase() })
        .collect();
    if is_reserved_file_name(&slug) {
        return Err(format!("'{name}' is a reserved name"));
    }
    Ok(slug)
}

fn is_reserved_file_name(slug: &str) -> bool {
    const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];
    if RESERVED.contains(&slug) {
        return true;
    }
    let bytes = slug.as_bytes();
    bytes.len() == 4
        && (slug.starts_with("com") || slug.starts_with("lpt"))
        && (b'1'..=b'9').contains(&bytes[3])
}

/// What the audio engine needs from the mixer control state.
#[derive(Clone)]
pub struct MixerLink {
    pub params: Arc<MixerParams>,
    pub voice_strip: Arc<AtomicUsize>,
}

pub struct MixerControl {
    pub params: Arc<MixerParams>,
    pub voice_strip: Arc<AtomicUsize>,
    names: Mutex<Vec<String>>,
}

impl Default for MixerControl {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerControl {
    /// 32 strips at the contract defaults; the voice chain on strip 0;
    /// only strip 0's fader is up.
    pub fn new() -> Self {
        let params = MixerParams::new(MAX_STRIPS);
        for (index, strip) in params.strips.iter().enumerate() {
            reset_strip(index, strip);
        }
        for bus in params.buses.iter() {
            reset_bus(bus);
        }
        MixerControl {
            params: Arc::new(params),
            voice_strip: Arc::new(AtomicUsize::new(0)),
            names: Mutex::new((0..MAX_STRIPS).map(default_strip_name).collect()),
        }
    }

    pub fn link(&self) -> MixerLink {
        MixerLink {
            params: self.params.clone(),
            voice_strip: self.voice_strip.clone(),
        }
    }

    fn strip(&self, index: u32) -> Result<&StripParams, String> {
        self.params
            .strips
            .get(index as usize)
            .ok_or_else(|| format!("strip index {index} out of range (0..{MAX_STRIPS})"))
    }

    pub fn set_strip_param(&self, index: u32, key: &str, value: f32) -> Result<(), String> {
        let strip = self.strip(index)?;
        match (f32_spec(STRIP_F32_KEYS, key), strip_f32(strip, key)) {
            (Some(spec), Some(field)) => {
                field.store(clamp_key(spec, value), Relaxed);
                Ok(())
            }
            _ => Err(format!("unknown strip key '{key}'")),
        }
    }

    pub fn set_strip_bool(&self, index: u32, key: &str, value: bool) -> Result<(), String> {
        let strip = self.strip(index)?;
        if key == "voice_chain" {
            return self.set_voice_chain(index as usize, value);
        }
        let field = strip_bool(strip, key).ok_or_else(|| format!("unknown strip key '{key}'"))?;
        field.store(value, Relaxed);
        Ok(())
    }

    /// At most one strip carries the voice chain: turning it on for a strip
    /// moves it there (clearing the others). Turning it off on the strip that
    /// holds it leaves no strip with the voice chain (`NO_VOICE_STRIP`);
    /// turning it off on any other strip is a no-op.
    fn set_voice_chain(&self, index: usize, on: bool) -> Result<(), String> {
        if on {
            self.voice_strip.store(index, Relaxed);
        } else if self.voice_strip.load(Relaxed) == index {
            self.voice_strip.store(NO_VOICE_STRIP, Relaxed);
        }
        Ok(())
    }

    pub fn rename_strip(&self, index: u32, name: &str) -> Result<(), String> {
        self.strip(index)?;
        let clean = clean_strip_name(index as usize, name)?;
        let mut names = self.names.lock();
        if let Some(slot) = names.get_mut(index as usize) {
            *slot = clean;
        }
        Ok(())
    }

    pub fn set_bus_param(&self, bus: &str, key: &str, value: f32) -> Result<(), String> {
        let p = &self.params.buses[bus_index(bus)?];
        match (f32_spec(BUS_F32_KEYS, key), bus_f32(p, key)) {
            (Some(spec), Some(field)) => {
                field.store(clamp_key(spec, value), Relaxed);
                Ok(())
            }
            _ => Err(format!("unknown bus key '{key}'")),
        }
    }

    pub fn set_bus_bool(&self, bus: &str, key: &str, value: bool) -> Result<(), String> {
        let p = &self.params.buses[bus_index(bus)?];
        match key {
            "mute" => {
                p.mute.store(value, Relaxed);
                Ok(())
            }
            _ => Err(format!("unknown bus key '{key}'")),
        }
    }

    fn strip_state(&self, index: usize, name: String) -> StripState {
        let p = &self.params.strips[index];
        let f = |key: &str| strip_f32(p, key).map(|a| a.load(Relaxed)).unwrap_or(0.0);
        let b = |key: &str| strip_bool(p, key).map(|a| a.load(Relaxed)).unwrap_or(false);
        StripState {
            index: index as u32,
            name,
            trim_db: f("trim_db"),
            polarity: b("polarity"),
            hpf_enabled: b("hpf_enabled"),
            hpf_hz: f("hpf_hz"),
            gate_enabled: b("gate_enabled"),
            gate_threshold_db: f("gate_threshold_db"),
            eq_low_db: f("eq_low_db"),
            eq_mid_db: f("eq_mid_db"),
            eq_high_db: f("eq_high_db"),
            comp_enabled: b("comp_enabled"),
            comp_threshold_db: f("comp_threshold_db"),
            comp_ratio: f("comp_ratio"),
            pan: f("pan"),
            mute: b("mute"),
            solo: b("solo"),
            fader_db: f("fader_db"),
            send_main_db: f("send_main_db"),
            send_stream_db: f("send_stream_db"),
            send_monitor_db: f("send_monitor_db"),
            monitor_post_fader: b("monitor_post_fader"),
            voice_chain: self.voice_strip.load(Relaxed) == index,
        }
    }

    /// Build the `MixerState` JSON model. `input_channels` is the running
    /// engine's channel count (1 when stopped); one strip is listed per
    /// input channel, capped at `MAX_STRIPS`.
    pub fn snapshot(&self, input_channels: u32, running: bool, scenes: Vec<String>) -> MixerState {
        let count = (input_channels as usize).clamp(1, MAX_STRIPS);
        let names: Vec<String> = self.names.lock().clone();
        let strips = (0..count)
            .map(|i| {
                let name = names.get(i).cloned().unwrap_or_else(|| default_strip_name(i));
                self.strip_state(i, name)
            })
            .collect();
        let buses = BUS_IDS
            .iter()
            .zip(self.params.buses.iter())
            .map(|(id, p)| BusState {
                id: (*id).to_string(),
                fader_db: p.fader_db.load(Relaxed),
                mute: p.mute.load(Relaxed),
                limiter_ceiling_db: p.limiter_ceiling_db.load(Relaxed),
            })
            .collect();
        MixerState {
            input_channels: input_channels.max(1),
            running,
            strips,
            buses,
            scenes,
        }
    }

    pub fn capture_scene(&self, name: &str) -> StoredScene {
        StoredScene {
            version: SCENE_FILE_VERSION,
            name: name.trim().to_string(),
            strip_names: self.names.lock().clone(),
            voice_strip: self.voice_strip.load(Relaxed),
            mixer: MixerScene::capture_named(&self.params, name.trim()),
        }
    }

    /// Recall a scene onto the live params (click-free: the mixer ramps).
    /// Values are re-clamped; bad names fall back to the defaults.
    pub fn apply_scene(&self, scene: &StoredScene) {
        // Strips the scene does not cover (a scene saved by an older build
        // with fewer strips) go back to their defaults.
        for (index, strip) in self.params.strips.iter().enumerate() {
            if index >= scene.mixer.strips.len() {
                reset_strip(index, strip);
            }
        }
        scene.mixer.apply(&self.params);
        for (index, strip) in self.params.strips.iter().enumerate() {
            sanitize_strip(index, strip);
        }
        for bus in self.params.buses.iter() {
            sanitize_bus(bus);
        }
        {
            let mut names = self.names.lock();
            for (i, slot) in names.iter_mut().enumerate() {
                if let Some(stored) = scene.strip_names.get(i) {
                    *slot = clean_strip_name(i, stored).unwrap_or_else(|_| default_strip_name(i));
                }
            }
        }
        let voice = if scene.voice_strip < MAX_STRIPS {
            scene.voice_strip
        } else {
            NO_VOICE_STRIP
        };
        self.voice_strip.store(voice, Relaxed);
    }
}
