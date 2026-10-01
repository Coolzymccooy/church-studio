//! `session.json` (design decision 7) and the marker and track records it
//! holds. Field names are camelCase, like the Tauri payloads, and the file
//! carries a format tag and version so a later "import session" can read it.
use serde::{Deserialize, Serialize};

pub const SESSION_FILE: &str = "session.json";
pub const SESSION_FORMAT: &str = "tiwaton-multitrack";
pub const SESSION_FORMAT_VERSION: u32 = 1;
/// Longest marker label, in characters.
pub const MAX_MARKER_LABEL_CHARS: usize = 80;

/// What a track holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Strip,
    Bus,
}

/// One recorded track. `file` is the first part; `files` lists every part
/// (more than one after a rollover).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTrack {
    pub file: String,
    pub files: Vec<String>,
    pub name: String,
    pub kind: TrackKind,
    pub channels: u16,
    /// "hardware" or "ndi" for a strip; "stream" or "main" for a bus.
    pub source: String,
    /// Mixer strip index (strips only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub strip: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub time_seconds: f64,
    pub label: String,
    /// "operator" (Add marker) or "scene" (a mixer scene load).
    pub source: String,
}

impl Marker {
    pub fn new(time_seconds: f64, label: &str, source: &str) -> Self {
        Marker {
            time_seconds,
            label: clean_marker_label(label),
            source: source.to_string(),
        }
    }
}

/// Trimmed, control characters removed, at most 80 characters; empty →
/// "Marker".
pub fn clean_marker_label(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_MARKER_LABEL_CHARS)
        .collect();
    let cleaned = cleaned.trim_end().to_string();
    if cleaned.is_empty() {
        "Marker".to_string()
    } else {
        cleaned
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFile {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub title: Option<String>,
    pub sample_rate: u32,
    pub started_local: String,
    pub started_utc: String,
    pub duration_seconds: f64,
    pub dropped_frames: u64,
    pub tracks: Vec<SessionTrack>,
    pub markers: Vec<Marker>,
    /// Why the recording stopped early (a disk error), if it did.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
}

pub fn session_to_json(session: &SessionFile) -> Result<String, String> {
    serde_json::to_string_pretty(session).map_err(|e| format!("cannot encode session.json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SessionFile {
        SessionFile {
            format: SESSION_FORMAT.to_string(),
            format_version: SESSION_FORMAT_VERSION,
            app_version: "1.5.0".to_string(),
            title: Some("Sunday".to_string()),
            sample_rate: 48_000,
            started_local: "2026-10-01T10:30:05+01:00".to_string(),
            started_utc: "2026-10-01T09:30:05Z".to_string(),
            duration_seconds: 12.5,
            dropped_frames: 3,
            tracks: vec![
                SessionTrack {
                    file: "01 Pastor.wav".to_string(),
                    files: vec!["01 Pastor.wav".to_string()],
                    name: "Pastor".to_string(),
                    kind: TrackKind::Strip,
                    channels: 1,
                    source: "hardware".to_string(),
                    strip: Some(0),
                },
                SessionTrack {
                    file: "Stream Mix.wav".to_string(),
                    files: vec!["Stream Mix.wav".to_string()],
                    name: "Stream Mix".to_string(),
                    kind: TrackKind::Bus,
                    channels: 2,
                    source: "stream".to_string(),
                    strip: None,
                },
            ],
            markers: vec![Marker::new(61.25, "Sermon", "scene")],
            error: None,
        }
    }

    #[test]
    fn session_json_has_the_documented_shape() {
        let value: serde_json::Value =
            serde_json::from_str(&session_to_json(&sample()).unwrap()).unwrap();
        for key in [
            "format", "formatVersion", "appVersion", "title", "sampleRate", "startedLocal",
            "startedUtc", "durationSeconds", "droppedFrames", "tracks", "markers",
        ] {
            assert!(value.get(key).is_some(), "missing {key}");
        }
        assert!(value.get("error").is_none());
        assert_eq!(value["format"], "tiwaton-multitrack");
        let strip = &value["tracks"][0];
        assert_eq!(strip["kind"], "strip");
        assert_eq!(strip["channels"], 1);
        assert_eq!(strip["source"], "hardware");
        assert_eq!(strip["strip"], 0);
        let bus = &value["tracks"][1];
        assert_eq!(bus["kind"], "bus");
        assert!(bus.get("strip").is_none());
        let marker = &value["markers"][0];
        assert_eq!(marker["timeSeconds"], 61.25);
        assert_eq!(marker["label"], "Sermon");
        assert_eq!(marker["source"], "scene");
    }

    #[test]
    fn session_json_round_trips() {
        let text = session_to_json(&sample()).unwrap();
        let back: SessionFile = serde_json::from_str(&text).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn marker_labels_are_cleaned() {
        assert_eq!(clean_marker_label("  Sermon \n"), "Sermon");
        assert_eq!(clean_marker_label(""), "Marker");
        assert_eq!(clean_marker_label("\u{7}"), "Marker");
        assert_eq!(clean_marker_label(&"x".repeat(200)).chars().count(), MAX_MARKER_LABEL_CHARS);
    }
}
