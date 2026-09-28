//! NDI® audio output (design decisions 1–4 in
//! `docs/specs/2026-09-28-integrations-design.md`).
//!
//! - `ffi`: the hand-declared SDK structs, constants and signatures.
//! - `runtime`: loads the installed NDI Runtime at run time.
//! - `audio_frame`: interleaved stereo → 480-sample planar float frames.
//! - `sender`: one sender thread per enabled bus, fed by a lock-free ring.
//! - `persist`: the settings file (`app_data_dir()/ndi.json`).
//!
//! This file holds the operator's settings (`NdiOutputs`), the source names
//! and `start_senders`, which the engine calls when it starts. Settings are
//! applied at the next engine start.
pub mod audio_frame;
pub mod ffi;
pub mod persist;
pub mod runtime;
#[cfg(test)]
mod runtime_tests;
pub mod sender;

use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_STREAM};
use ringbuf::traits::Consumer;
use sender::NdiBusSender;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub const DEFAULT_BASE_NAME: &str = "TIWATON Studio";
pub const MAX_BASE_NAME_CHARS: usize = 60;

/// Characters removed from the base name: path/shell-special characters and
/// parentheses (NDI shows sources as `MACHINE (name)` and we append the bus
/// in parentheses ourselves).
const FORBIDDEN_CHARS: [char; 11] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|', '(', ')'];

/// Which buses are published as NDI sources, and their base name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NdiOutputs {
    pub stream: bool,
    pub main: bool,
    pub monitor: bool,
    pub base_name: String,
}

impl Default for NdiOutputs {
    fn default() -> Self {
        NdiOutputs {
            stream: false,
            main: false,
            monitor: false,
            base_name: DEFAULT_BASE_NAME.to_string(),
        }
    }
}

impl NdiOutputs {
    pub fn any_enabled(&self) -> bool {
        self.stream || self.main || self.monitor
    }

    /// A copy with the base name sanitized.
    pub fn sanitized(&self) -> Self {
        NdiOutputs {
            base_name: sanitize_base_name(&self.base_name),
            ..self.clone()
        }
    }

    /// `(bus, source name)` for every enabled output, Stream first.
    pub fn plan(&self) -> Vec<(usize, String)> {
        let base = sanitize_base_name(&self.base_name);
        [
            (self.stream, BUS_STREAM, "Stream"),
            (self.main, BUS_MAIN, "Main"),
            (self.monitor, BUS_MONITOR, "Monitor"),
        ]
        .into_iter()
        .filter(|(enabled, _, _)| *enabled)
        .map(|(_, bus, label)| (bus, source_name(&base, label)))
        .collect()
    }
}

/// "TIWATON Studio" + "Stream" → "TIWATON Studio (Stream)".
pub fn source_name(base: &str, bus_label: &str) -> String {
    format!("{base} ({bus_label})")
}

/// Trim, collapse whitespace, drop control and forbidden characters and cap
/// at `MAX_BASE_NAME_CHARS` characters. Empty → `DEFAULT_BASE_NAME`.
/// Mirrored by `sanitizeNdiBaseName` in `src/lib/ndiOutput.js`.
pub fn sanitize_base_name(raw: &str) -> String {
    let mut cleaned = String::with_capacity(raw.len());
    let mut last_was_space = true; // drops leading spaces
    for ch in raw.chars() {
        let ch = if ch.is_whitespace() { ' ' } else { ch };
        if ch.is_control() || FORBIDDEN_CHARS.contains(&ch) {
            continue;
        }
        if ch == ' ' {
            if last_was_space {
                continue;
            }
            last_was_space = true;
        } else {
            last_was_space = false;
        }
        cleaned.push(ch);
    }
    let capped: String = cleaned.trim_end().chars().take(MAX_BASE_NAME_CHARS).collect();
    let capped = capped.trim_end();
    if capped.is_empty() {
        DEFAULT_BASE_NAME.to_string()
    } else {
        capped.to_string()
    }
}

/// Managed Tauri state: the settings used at the next engine start.
pub struct NdiSettings(pub Mutex<NdiOutputs>);

impl NdiSettings {
    pub fn new() -> Self {
        NdiSettings(Mutex::new(NdiOutputs::default()))
    }

    pub fn snapshot(&self) -> NdiOutputs {
        match self.0.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn replace(&self, next: NdiOutputs) {
        match self.0.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }
}

/// Start one sender per enabled output. `make_ring(capacity)` builds each
/// SPSC ring; the producers (with their bus) go to the audio callback. An
/// output that cannot start is logged and skipped, so the engine always
/// starts.
pub fn start_senders<P, C, F>(
    outputs: &NdiOutputs,
    sample_rate: u32,
    dropped: &Arc<AtomicU64>,
    mut make_ring: F,
) -> (Vec<(usize, P)>, Vec<NdiBusSender>)
where
    F: FnMut(usize) -> (P, C),
    C: Consumer<Item = f32> + Send + 'static,
{
    let mut producers = Vec::new();
    let mut senders = Vec::new();
    let plan = outputs.plan();
    if plan.is_empty() {
        return (producers, senders);
    }
    let runtime = match runtime::runtime() {
        Ok(runtime) => runtime,
        Err(err) => {
            log::warn!("NDI outputs not started: {err}");
            return (producers, senders);
        }
    };
    for (bus, name) in plan {
        let (producer, consumer) = make_ring(sender::ring_capacity(sample_rate));
        match NdiBusSender::start(runtime, name.clone(), sample_rate, consumer, dropped.clone()) {
            Ok(sender) => {
                log::info!("NDI source '{name}' sending");
                producers.push((bus, producer));
                senders.push(sender);
            }
            Err(err) => log::warn!("NDI source '{name}' not started: {err}"),
        }
    }
    (producers, senders)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_follow_the_design() {
        let outputs = NdiOutputs {
            stream: true,
            main: true,
            monitor: true,
            ..NdiOutputs::default()
        };
        let names: Vec<String> = outputs.plan().into_iter().map(|(_, n)| n).collect();
        assert_eq!(
            names,
            vec![
                "TIWATON Studio (Stream)".to_string(),
                "TIWATON Studio (Main)".to_string(),
                "TIWATON Studio (Monitor)".to_string(),
            ]
        );
    }

    #[test]
    fn plan_maps_buses_and_skips_disabled() {
        let outputs = NdiOutputs {
            monitor: true,
            ..NdiOutputs::default()
        };
        assert_eq!(
            outputs.plan(),
            vec![(BUS_MONITOR, "TIWATON Studio (Monitor)".to_string())]
        );
        assert!(NdiOutputs::default().plan().is_empty());
        assert!(!NdiOutputs::default().any_enabled());
    }

    #[test]
    fn sanitize_trims_collapses_and_strips() {
        assert_eq!(sanitize_base_name("  Grace   Church \n Live "), "Grace Church Live");
        assert_eq!(sanitize_base_name("A/B\\C:(D)*?\"<>|\u{7}E"), "ABCDE");
        assert_eq!(sanitize_base_name("   "), DEFAULT_BASE_NAME);
        assert_eq!(sanitize_base_name("()"), DEFAULT_BASE_NAME);
    }

    #[test]
    fn sanitize_caps_at_60_characters() {
        let long = "é".repeat(80);
        assert_eq!(sanitize_base_name(&long).chars().count(), MAX_BASE_NAME_CHARS);
        let spaced = format!("{} tail", "x".repeat(59));
        assert_eq!(sanitize_base_name(&spaced), "x".repeat(59));
    }

    #[test]
    fn outputs_deserialize_with_defaults() {
        let parsed: NdiOutputs = serde_json::from_str(r#"{"stream":true}"#).unwrap();
        assert!(parsed.stream);
        assert!(!parsed.main);
        assert_eq!(parsed.base_name, DEFAULT_BASE_NAME);
    }
}
