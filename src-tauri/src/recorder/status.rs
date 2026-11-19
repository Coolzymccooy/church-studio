//! The `recorder_status` / `recorder-status` payload (design decision 9) and
//! the summary of the last finished recording. camelCase, like the other
//! Tauri payloads.
use super::writer::WriterOutcome;
use serde::Serialize;

pub const STATE_UNAVAILABLE: &str = "unavailable";
pub const STATE_IDLE: &str = "idle";
pub const STATE_RECORDING: &str = "recording";

/// What the last recording left behind.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSummary {
    pub folder: String,
    pub duration_seconds: f64,
    pub dropped_frames: u64,
    pub markers: usize,
    /// Every file written, in track order.
    pub files: Vec<String>,
    pub error: Option<String>,
}

impl RecordingSummary {
    pub fn from_outcome(outcome: &WriterOutcome) -> Self {
        RecordingSummary {
            folder: outcome.folder.display().to_string(),
            duration_seconds: outcome.duration_seconds,
            dropped_frames: outcome.dropped_frames,
            markers: outcome.markers,
            files: outcome
                .tracks
                .iter()
                .flat_map(|t| t.files.iter().cloned())
                .collect(),
            error: outcome.error.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderStatus {
    /// "unavailable" (engine stopped), "idle" or "recording".
    pub state: String,
    pub engine_running: bool,
    pub elapsed_seconds: f64,
    /// The folder being recorded into (while recording).
    pub folder: Option<String>,
    /// Track names being recorded (while recording).
    pub tracks: Vec<String>,
    pub dropped_frames: u64,
    pub markers: usize,
    pub last_error: Option<String>,
    pub last_summary: Option<RecordingSummary>,
}

impl RecorderStatus {
    /// Not recording: the engine state plus whatever the last recording
    /// left.
    pub fn idle(engine_running: bool, last: Option<RecordingSummary>) -> Self {
        let state = if engine_running { STATE_IDLE } else { STATE_UNAVAILABLE };
        RecorderStatus {
            state: state.to_string(),
            engine_running,
            elapsed_seconds: 0.0,
            folder: None,
            tracks: Vec::new(),
            dropped_frames: 0,
            markers: 0,
            last_error: last.as_ref().and_then(|s| s.error.clone()),
            last_summary: last,
        }
    }

    pub fn recording(
        folder: String,
        tracks: Vec<String>,
        elapsed_seconds: f64,
        dropped_frames: u64,
        markers: usize,
    ) -> Self {
        RecorderStatus {
            state: STATE_RECORDING.to_string(),
            engine_running: true,
            elapsed_seconds,
            folder: Some(folder),
            tracks,
            dropped_frames,
            markers,
            last_error: None,
            last_summary: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::session::{SessionTrack, TrackKind};
    use std::path::PathBuf;

    #[test]
    fn status_json_is_camel_case() {
        let status = RecorderStatus::recording("D:/Rec/x".to_string(), vec!["Pastor".to_string()], 2.5, 1, 3);
        let value = serde_json::to_value(&status).unwrap();
        for key in [
            "state", "engineRunning", "elapsedSeconds", "folder", "tracks", "droppedFrames",
            "markers", "lastError", "lastSummary",
        ] {
            assert!(value.get(key).is_some(), "missing {key}");
        }
        assert_eq!(value["state"], "recording");
    }

    #[test]
    fn idle_status_carries_the_last_summary_and_error() {
        let outcome = WriterOutcome {
            folder: PathBuf::from("rec"),
            tracks: vec![SessionTrack {
                file: "a.wav".to_string(),
                files: vec!["a.wav".to_string(), "a part 2.wav".to_string()],
                name: "A".to_string(),
                kind: TrackKind::Strip,
                channels: 1,
                source: "hardware".to_string(),
                strip: Some(0),
            }],
            duration_seconds: 4.0,
            dropped_frames: 0,
            markers: 2,
            error: Some("disk full".to_string()),
        };
        let summary = RecordingSummary::from_outcome(&outcome);
        assert_eq!(summary.files, vec!["a.wav".to_string(), "a part 2.wav".to_string()]);
        let status = RecorderStatus::idle(true, Some(summary));
        assert_eq!(status.state, STATE_IDLE);
        assert_eq!(status.last_error.as_deref(), Some("disk full"));
        assert_eq!(RecorderStatus::idle(false, None).state, STATE_UNAVAILABLE);
    }
}
