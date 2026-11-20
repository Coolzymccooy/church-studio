//! Whether the computer-audio input is on, and its file
//! (`app_data_dir()/computer_audio.json`, next to `ndi_inputs.json`).
//!
//! Off by default. Like the NDI inputs, a change applies at the next engine
//! start. A missing or corrupt file means "off" (with a warning for a
//! corrupt one); saving is atomic (temp file, then rename).
use crate::ndi::persist;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub const SETTINGS_FILE: &str = "computer_audio.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ComputerAudio {
    pub enabled: bool,
}

pub fn from_json(text: &str) -> Result<ComputerAudio, String> {
    serde_json::from_str::<ComputerAudio>(text)
        .map_err(|e| format!("computer audio settings file is not valid: {e}"))
}

pub fn to_json(settings: &ComputerAudio) -> Result<String, String> {
    serde_json::to_string_pretty(settings)
        .map_err(|e| format!("cannot encode the computer audio settings: {e}"))
}

/// The file's contents (`None` = could not be read) → settings, plus a
/// warning when "off" was used because the file is corrupt.
pub fn or_default(text: Option<&str>) -> (ComputerAudio, Option<String>) {
    match text {
        None => (ComputerAudio::default(), None),
        Some(text) => match from_json(text) {
            Ok(settings) => (settings, None),
            Err(err) => (ComputerAudio::default(), Some(format!("{err}; computer audio is off"))),
        },
    }
}

pub fn load(path: &Path) -> ComputerAudio {
    let text = persist::read_optional(path);
    let (settings, warning) = or_default(text.as_deref());
    if let Some(warning) = warning {
        log::warn!("{warning} ({})", path.display());
    }
    settings
}

pub fn save(path: &Path, settings: &ComputerAudio) -> Result<(), String> {
    persist::write_atomic(path, &to_json(settings)?)
}

/// Managed Tauri state: the setting used at the next engine start.
pub struct ComputerAudioSettings(Mutex<ComputerAudio>);

impl ComputerAudioSettings {
    pub fn new() -> Self {
        ComputerAudioSettings(Mutex::new(ComputerAudio::default()))
    }

    pub fn snapshot(&self) -> ComputerAudio {
        match self.0.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    pub fn replace(&self, next: ComputerAudio) {
        match self.0.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_and_when_the_file_is_missing_or_corrupt() {
        assert!(!ComputerAudio::default().enabled);
        assert!(!ComputerAudioSettings::new().snapshot().enabled);
        let (missing, warning) = or_default(None);
        assert!(!missing.enabled && warning.is_none());
        for text in ["", "{", "true", r#"{"enabled":"yes"}"#] {
            let (settings, warning) = or_default(Some(text));
            assert!(!settings.enabled, "{text:?}");
            assert!(warning.is_some(), "{text:?}");
        }
        assert_eq!(from_json("{}").unwrap(), ComputerAudio::default());
    }

    #[test]
    fn json_round_trips() {
        let on = ComputerAudio { enabled: true };
        assert_eq!(from_json(&to_json(&on).unwrap()).unwrap(), on);
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("tiwaton-computer-audio-{}", std::process::id()));
        let path = dir.join(SETTINGS_FILE);
        save(&path, &ComputerAudio { enabled: true }).unwrap();
        assert!(load(&path).enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
