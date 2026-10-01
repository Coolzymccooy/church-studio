//! The OBS status snapshot sent to the UI, plus pure helpers: friendly error
//! text, timecode formatting, scene ordering and reconnect backoff.
use obws::error::Error as ObsError;
use serde::Serialize;
use std::time::Duration;

pub const MSG_NOT_RUNNING: &str =
    "OBS isn't running or WebSocket server is off (Tools → WebSocket Server Settings)";
pub const MSG_WRONG_PASSWORD: &str =
    "Wrong OBS password (check Tools → WebSocket Server Settings → Show Connect Info)";
pub const MSG_NOT_CONNECTED: &str = "Not connected to OBS";
pub const MSG_LOST: &str = "Lost the connection to OBS. Retrying…";
pub const MSG_CONFIRM: &str = "Please confirm first: this changes what goes out live";

pub const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
pub const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObsStatus {
    pub enabled: bool,
    pub connecting: bool,
    pub connected: bool,
    pub obs_version: Option<String>,
    pub current_scene: Option<String>,
    pub scenes: Vec<String>,
    pub streaming: bool,
    pub recording: bool,
    pub stream_timecode: Option<String>,
    pub record_timecode: Option<String>,
    pub last_error: Option<String>,
}

impl ObsStatus {
    /// Clear everything learned from a connection, keeping `enabled` and the
    /// last error.
    pub fn reset_connection(&mut self) {
        self.connected = false;
        self.connecting = self.enabled;
        self.obs_version = None;
        self.current_scene = None;
        self.scenes.clear();
        self.streaming = false;
        self.recording = false;
        self.stream_timecode = None;
        self.record_timecode = None;
    }
}

/// Doubling backoff, 2 s → 30 s.
pub fn next_backoff(current: Duration) -> Duration {
    let doubled = current.saturating_mul(2);
    if doubled > MAX_BACKOFF {
        MAX_BACKOFF
    } else if doubled < INITIAL_BACKOFF {
        INITIAL_BACKOFF
    } else {
        doubled
    }
}

/// `HH:MM:SS` from whole seconds; negatives clamp to zero.
pub fn format_timecode(total_secs: i64) -> String {
    let secs = total_secs.max(0);
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

/// OBS reports scenes with index 0 at the bottom of its list; show them in
/// the order the operator sees in OBS (highest index first).
pub fn order_scenes(mut scenes: Vec<(String, usize)>) -> Vec<String> {
    scenes.sort_by(|a, b| b.1.cmp(&a.1));
    scenes.into_iter().map(|(name, _)| name).collect()
}

/// obs-websocket closes with code 4009 when authentication fails (wrong or
/// missing password).
pub fn looks_like_auth_failure(details: &str) -> bool {
    let lower = details.to_lowercase();
    lower.contains("4009") || lower.contains("authentication")
}

/// Friendly, operator-facing text for an obws error. Never includes the
/// password (obws errors don't carry it).
pub fn friendly_error(err: &ObsError) -> String {
    match err {
        ObsError::Connect(_) | ObsError::Timeout => MSG_NOT_RUNNING.to_string(),
        ObsError::Handshake(inner) => {
            if looks_like_auth_failure(&format!("{inner:?}")) {
                MSG_WRONG_PASSWORD.to_string()
            } else {
                "OBS refused the connection. Check the address and port.".to_string()
            }
        }
        ObsError::ObsStudioVersion(version, _) => {
            format!("OBS {version} is too old. Update to OBS Studio 30.2 or newer.")
        }
        ObsError::ObsWebsocketVersion(version, _) => {
            format!("OBS WebSocket {version} is too old. Update OBS Studio to 30.2 or newer.")
        }
        ObsError::Disconnected | ObsError::Send(_) | ObsError::ReceiveMessage(_) => {
            MSG_NOT_CONNECTED.to_string()
        }
        ObsError::Api { code, message } => match message {
            Some(text) if !text.trim().is_empty() => format!("OBS said: {text}"),
            _ => format!("OBS rejected the request ({code:?})"),
        },
        other => format!("OBS error: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps() {
        let mut b = INITIAL_BACKOFF;
        let mut seen = vec![b.as_secs()];
        for _ in 0..6 {
            b = next_backoff(b);
            seen.push(b.as_secs());
        }
        assert_eq!(seen, vec![2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(next_backoff(Duration::from_secs(0)), INITIAL_BACKOFF);
    }

    #[test]
    fn timecode_formats() {
        assert_eq!(format_timecode(0), "00:00:00");
        assert_eq!(format_timecode(59), "00:00:59");
        assert_eq!(format_timecode(3661), "01:01:01");
        assert_eq!(format_timecode(-5), "00:00:00");
    }

    #[test]
    fn scenes_follow_obs_display_order() {
        let ordered = order_scenes(vec![
            ("Bottom".to_string(), 0),
            ("Top".to_string(), 2),
            ("Middle".to_string(), 1),
        ]);
        assert_eq!(ordered, vec!["Top", "Middle", "Bottom"]);
    }

    #[test]
    fn auth_failure_detection() {
        assert!(looks_like_auth_failure(
            "ConnectionClosed(Some(CloseDetails { code: Library(4009), reason: \"\" }))"
        ));
        assert!(looks_like_auth_failure("Authentication failed."));
        assert!(!looks_like_auth_failure("NoHello"));
    }

    #[test]
    fn reset_connection_keeps_enabled_and_error() {
        let mut s = ObsStatus {
            enabled: true,
            connected: true,
            streaming: true,
            scenes: vec!["A".to_string()],
            last_error: Some("x".to_string()),
            ..ObsStatus::default()
        };
        s.reset_connection();
        assert!(s.enabled && s.connecting && !s.connected && !s.streaming);
        assert!(s.scenes.is_empty());
        assert_eq!(s.last_error.as_deref(), Some("x"));
    }

    #[test]
    fn status_serializes_camel_case() {
        let json = serde_json::to_value(ObsStatus::default()).unwrap();
        for key in [
            "connected",
            "obsVersion",
            "currentScene",
            "scenes",
            "streaming",
            "recording",
            "streamTimecode",
            "lastError",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
    }
}
