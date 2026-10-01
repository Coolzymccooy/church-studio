//! Which NDI® sources are received, and their file
//! (`app_data_dir()/ndi_inputs.json`, next to the outputs' `ndi.json`).
//!
//! Like the outputs, the selection is applied at the next engine start. A
//! missing or corrupt file falls back to "no inputs" with a warning; saving
//! is atomic (temp file, then rename).
use crate::ndi::persist;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub const INPUTS_FILE: &str = "ndi_inputs.json";
/// At most this many NDI inputs (design decision 5).
pub const MAX_NDI_INPUTS: usize = 4;
/// Longer names are dropped rather than cut (a cut name would not match).
pub const MAX_SOURCE_NAME_CHARS: usize = 256;

/// The NDI sources to receive, by full name ("MACHINE (Source)").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NdiInputs {
    pub sources: Vec<String>,
}

impl NdiInputs {
    pub fn any_enabled(&self) -> bool {
        !self.sources.is_empty()
    }

    /// A copy with the source list sanitized (`sanitize_sources`).
    pub fn sanitized(&self) -> Self {
        NdiInputs {
            sources: sanitize_sources(&self.sources),
        }
    }
}

/// Trim each name and drop empty ones, names with control characters and
/// names over `MAX_SOURCE_NAME_CHARS`; drop duplicates (first wins) and keep
/// at most `MAX_NDI_INPUTS`. Mirrored by `sanitizeNdiSourceList` in
/// `src/lib/ndiInput.js`.
pub fn sanitize_sources(raw: &[String]) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for name in raw {
        let name = name.trim();
        let valid = !name.is_empty()
            && name.chars().count() <= MAX_SOURCE_NAME_CHARS
            && !name.chars().any(char::is_control);
        if valid && !clean.iter().any(|kept| kept == name) {
            clean.push(name.to_string());
        }
        if clean.len() == MAX_NDI_INPUTS {
            break;
        }
    }
    clean
}

/// Parse the file; the list is sanitized.
pub fn inputs_from_json(text: &str) -> Result<NdiInputs, String> {
    serde_json::from_str::<NdiInputs>(text)
        .map(|inputs| inputs.sanitized())
        .map_err(|e| format!("NDI inputs file is not valid: {e}"))
}

pub fn inputs_to_json(inputs: &NdiInputs) -> Result<String, String> {
    serde_json::to_string_pretty(inputs).map_err(|e| format!("cannot encode NDI inputs: {e}"))
}

/// The file's contents (`None` = could not be read) → inputs, plus a
/// warning when the defaults were used because the file is corrupt.
pub fn inputs_or_default(text: Option<&str>) -> (NdiInputs, Option<String>) {
    match text {
        None => (NdiInputs::default(), None),
        Some(text) => match inputs_from_json(text) {
            Ok(inputs) => (inputs, None),
            Err(err) => (NdiInputs::default(), Some(format!("{err}; using no inputs"))),
        },
    }
}

/// Load from `path`; a missing file is simply "no inputs".
pub fn load_inputs(path: &Path) -> NdiInputs {
    let text = persist::read_optional(path);
    let (inputs, warning) = inputs_or_default(text.as_deref());
    if let Some(warning) = warning {
        log::warn!("{warning} ({})", path.display());
    }
    inputs
}

pub fn save_inputs(path: &Path, inputs: &NdiInputs) -> Result<(), String> {
    persist::write_atomic(path, &inputs_to_json(inputs)?)
}

/// Managed Tauri state: the inputs used at the next engine start.
pub struct NdiInputSettings(pub Mutex<NdiInputs>);

impl NdiInputSettings {
    pub fn new() -> Self {
        NdiInputSettings(Mutex::new(NdiInputs::default()))
    }

    pub fn snapshot(&self) -> NdiInputs {
        match self.0.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn replace(&self, next: NdiInputs) {
        match self.0.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sanitize_trims_dedupes_and_caps_at_four() {
        let raw = names(&[" A (One) ", "", "A (One)", "B\u{7} (x)", "C", "D", "E", "F"]);
        assert_eq!(sanitize_sources(&raw), names(&["A (One)", "C", "D", "E"]));
    }

    #[test]
    fn sanitize_drops_overlong_names() {
        let long = "x".repeat(MAX_SOURCE_NAME_CHARS + 1);
        let ok = "y".repeat(MAX_SOURCE_NAME_CHARS);
        assert_eq!(sanitize_sources(&[long, ok.clone()]), vec![ok]);
    }

    #[test]
    fn file_parse_falls_back_on_corruption() {
        let (inputs, warning) = inputs_or_default(Some(r#"{"sources":["PC (Mic)","PC (Mic)"]}"#));
        assert_eq!(inputs.sources, names(&["PC (Mic)"]));
        assert!(warning.is_none());
        for text in ["", "{", "[1]", r#"{"sources":"PC"}"#] {
            let (inputs, warning) = inputs_or_default(Some(text));
            assert_eq!(inputs, NdiInputs::default(), "{text:?}");
            assert!(warning.is_some(), "{text:?}");
        }
        let (missing, warning) = inputs_or_default(None);
        assert!(!missing.any_enabled());
        assert!(warning.is_none());
    }

    #[test]
    fn json_round_trips_and_missing_fields_default() {
        let inputs = NdiInputs { sources: names(&["PC (Keys)", "LAPTOP (Video)"]) };
        let text = inputs_to_json(&inputs).unwrap();
        assert_eq!(inputs_from_json(&text).unwrap(), inputs);
        assert_eq!(inputs_from_json("{}").unwrap(), NdiInputs::default());
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("tiwaton-ndi-in-test-{}", std::process::id()));
        let path = dir.join(INPUTS_FILE);
        let inputs = NdiInputs { sources: names(&["PC (Keys)"]) };
        save_inputs(&path, &inputs).unwrap();
        assert_eq!(load_inputs(&path), inputs);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
