//! Recorder settings, stored in `app_data_dir()/recorder.json` (design
//! decision 6) with the same atomic write and corrupt-file fallback as the
//! NDI settings (`crate::ndi::persist`).
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SETTINGS_FILE: &str = "recorder.json";
/// Folder under `<Documents>` used when no folder is chosen.
pub const DEFAULT_FOLDER_NAME: &str = "TIWATON Recordings";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecorderConfig {
    /// Where recordings go; `None` = `<Documents>/TIWATON Recordings`.
    pub folder: Option<String>,
    /// Also record the Main mix (off by default).
    pub include_main: bool,
    /// Strips to record; `None` = every active strip.
    pub armed_strips: Option<Vec<u32>>,
}

impl RecorderConfig {
    /// A blank folder means the default; armed strips are sorted and
    /// de-duplicated.
    pub fn sanitized(mut self) -> Self {
        self.folder = self
            .folder
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty());
        if let Some(strips) = self.armed_strips.as_mut() {
            strips.sort_unstable();
            strips.dedup();
        }
        self
    }

    /// Is strip `index` armed?
    pub fn is_armed(&self, index: usize) -> bool {
        match self.armed_strips.as_ref() {
            None => true,
            Some(strips) => strips.iter().any(|&s| s as usize == index),
        }
    }
}

/// Parse the settings file; a corrupt file is an error.
pub fn config_from_json(text: &str) -> Result<RecorderConfig, String> {
    serde_json::from_str::<RecorderConfig>(text)
        .map(RecorderConfig::sanitized)
        .map_err(|e| format!("recorder settings file is not valid: {e}"))
}

pub fn config_to_json(config: &RecorderConfig) -> Result<String, String> {
    serde_json::to_string_pretty(config).map_err(|e| format!("cannot encode recorder settings: {e}"))
}

/// Load from `path`; a missing or corrupt file gives the defaults (with a
/// logged warning when corrupt or unreadable).
pub fn load_from_path(path: &Path) -> RecorderConfig {
    let Some(text) = crate::ndi::persist::read_optional(path) else {
        return RecorderConfig::default();
    };
    config_from_json(&text).unwrap_or_else(|err| {
        log::warn!("{err}; using defaults ({})", path.display());
        RecorderConfig::default()
    })
}

pub fn save_to_path(path: &Path, config: &RecorderConfig) -> Result<(), String> {
    crate::ndi::persist::write_atomic(path, &config_to_json(config)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_record_every_strip_without_main() {
        let config = RecorderConfig::default();
        assert!(config.is_armed(0) && config.is_armed(31));
        assert!(!config.include_main);
        assert_eq!(config.folder, None);
    }

    #[test]
    fn json_is_camel_case_and_round_trips() {
        let config = RecorderConfig {
            folder: Some("D:/Services".to_string()),
            include_main: true,
            armed_strips: Some(vec![0, 2]),
        };
        let text = config_to_json(&config).unwrap();
        assert!(text.contains("includeMain") && text.contains("armedStrips"));
        assert_eq!(config_from_json(&text).unwrap(), config);
    }

    #[test]
    fn sanitizing_blanks_and_duplicates() {
        let config = config_from_json(r#"{"folder":"  ","armedStrips":[3,1,3]}"#).unwrap();
        assert_eq!(config.folder, None);
        assert_eq!(config.armed_strips, Some(vec![1, 3]));
        assert!(config.is_armed(1) && !config.is_armed(2));
    }

    #[test]
    fn corrupt_or_missing_files_give_defaults() {
        assert!(config_from_json("{").is_err());
        let path = std::env::temp_dir().join(format!("tiwaton-recorder-missing-{}.json", std::process::id()));
        assert_eq!(load_from_path(&path), RecorderConfig::default());
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("tiwaton-recorder-cfg-{}", std::process::id()));
        let path = dir.join(SETTINGS_FILE);
        let config = RecorderConfig {
            include_main: true,
            ..RecorderConfig::default()
        };
        save_to_path(&path, &config).unwrap();
        assert_eq!(load_from_path(&path), config);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
