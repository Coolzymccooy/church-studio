//! `NdiOutputs` on disk (`app_data_dir()/ndi.json`).
//!
//! A missing or unreadable/corrupt file falls back to the defaults with a
//! warning, so a bad file can never stop the app. Saving writes a temp file
//! next to the target and renames it over the target, so a crash mid-write
//! leaves either the old file or the new one, never half of one.
use super::NdiOutputs;
use std::fs;
use std::path::{Path, PathBuf};

pub const SETTINGS_FILE: &str = "ndi.json";

/// Parse the settings file. Missing fields take their defaults and the base
/// name is sanitized.
pub fn outputs_from_json(text: &str) -> Result<NdiOutputs, String> {
    serde_json::from_str::<NdiOutputs>(text)
        .map(|outputs| outputs.sanitized())
        .map_err(|e| format!("NDI settings file is not valid: {e}"))
}

pub fn outputs_to_json(outputs: &NdiOutputs) -> Result<String, String> {
    serde_json::to_string_pretty(outputs).map_err(|e| format!("cannot encode NDI settings: {e}"))
}

/// The file's contents (`None` = could not be read) → settings, plus a
/// warning when the defaults were used.
pub fn outputs_or_default(text: Option<&str>) -> (NdiOutputs, Option<String>) {
    match text {
        None => (
            NdiOutputs::default(),
            Some("NDI settings file not found; using defaults".to_string()),
        ),
        Some(text) => match outputs_from_json(text) {
            Ok(outputs) => (outputs, None),
            Err(err) => (NdiOutputs::default(), Some(format!("{err}; using defaults"))),
        },
    }
}

/// Load from `path`, logging a warning and returning the defaults when the
/// file is missing, unreadable or corrupt.
pub fn load_from_path(path: &Path) -> NdiOutputs {
    let text = read_optional(path);
    let (outputs, warning) = outputs_or_default(text.as_deref());
    if let Some(warning) = warning {
        log::warn!("{warning} ({})", path.display());
    }
    outputs
}

/// Write `outputs` to `path` atomically: temp file, then rename (which
/// replaces an existing file on every platform std supports).
pub fn save_to_path(path: &Path, outputs: &NdiOutputs) -> Result<(), String> {
    write_atomic(path, &outputs_to_json(outputs)?)
}

/// The file's text, or `None` when it is missing (silently) or unreadable
/// (with a logged warning).
pub fn read_optional(path: &Path) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("cannot read {}: {e}", path.display());
            }
            None
        }
    }
}

/// Write `text` to `path` atomically: temp file, then rename (which
/// replaces an existing file on every platform std supports).
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let tmp = temp_path(path);
    fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot replace {}: {e}", path.display())
    })
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| SETTINGS_FILE.into());
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ndi::DEFAULT_BASE_NAME;

    #[test]
    fn missing_file_gives_defaults_with_warning() {
        let (outputs, warning) = outputs_or_default(None);
        assert_eq!(outputs, NdiOutputs::default());
        assert!(warning.is_some());
    }

    #[test]
    fn corrupt_file_gives_defaults_with_warning() {
        for text in ["", "{", "not json", r#"{"stream":"yes"}"#, "42"] {
            let (outputs, warning) = outputs_or_default(Some(text));
            assert_eq!(outputs, NdiOutputs::default(), "input {text:?}");
            assert!(warning.is_some(), "input {text:?}");
        }
    }

    #[test]
    fn valid_file_is_parsed_and_sanitized() {
        let (outputs, warning) =
            outputs_or_default(Some(r#"{"stream":true,"monitor":true,"base_name":"  A/(B) "}"#));
        assert!(warning.is_none());
        assert!(outputs.stream && outputs.monitor && !outputs.main);
        assert_eq!(outputs.base_name, "AB");
    }

    #[test]
    fn partial_file_takes_defaults_for_missing_fields() {
        let outputs = outputs_from_json(r#"{"main":true}"#).unwrap();
        assert!(outputs.main);
        assert_eq!(outputs.base_name, DEFAULT_BASE_NAME);
    }

    #[test]
    fn json_round_trips() {
        let outputs = NdiOutputs {
            stream: true,
            main: false,
            monitor: true,
            base_name: "Grace Church".to_string(),
        };
        let text = outputs_to_json(&outputs).unwrap();
        assert_eq!(outputs_from_json(&text).unwrap(), outputs);
    }

    #[test]
    fn temp_path_sits_next_to_the_target() {
        let tmp = temp_path(Path::new("/data/app/ndi.json"));
        assert_eq!(tmp, Path::new("/data/app/ndi.json.tmp"));
    }

    #[test]
    fn save_then_load_replaces_the_file() {
        let dir = std::env::temp_dir().join(format!("tiwaton-ndi-test-{}", std::process::id()));
        let path = dir.join(SETTINGS_FILE);
        let first = NdiOutputs {
            stream: true,
            ..NdiOutputs::default()
        };
        let second = NdiOutputs {
            main: true,
            base_name: "Second".to_string(),
            ..NdiOutputs::default()
        };
        save_to_path(&path, &first).unwrap();
        save_to_path(&path, &second).unwrap();
        assert_eq!(load_from_path(&path), second);
        assert!(!temp_path(&path).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
