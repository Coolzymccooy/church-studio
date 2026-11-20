//! Tauri commands for "play" on the last recording (`crate::preview`):
//!
//! | command | args | returns |
//! |---|---|---|
//! | `recorder_play` | `track?` (a file of the last recording; default its Stream mix) | `PreviewStatus` |
//! | `recorder_play_stop` | none | `PreviewStatus` |
//! | `recorder_play_status` | none | `PreviewStatus` (idle while the engine is off) |
use crate::audio::EngineState;
use crate::export::open_parts;
use crate::export::request::{find_track, read_session};
use crate::preview::PreviewStatus;
use crate::recorder::session::SessionFile;
use crate::recorder_commands::RecorderControl;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::State;

const STREAM_MIX: &str = "Stream Mix";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

const IDLE: PreviewStatus = PreviewStatus { playing: false, position_seconds: 0.0, duration_seconds: 0.0 };

/// The track to play when none is named: the Stream mix, else the first.
pub fn default_track(session: &SessionFile) -> Option<String> {
    pick_default(session.tracks.iter().map(|t| (t.name.as_str(), t.file.as_str())))
}

/// `(name, file)` pairs → the Stream mix's file, else the first file.
fn pick_default<'a>(mut tracks: impl Iterator<Item = (&'a str, &'a str)> + Clone) -> Option<String> {
    let first = tracks.clone().next().map(|(_, file)| file.to_string());
    tracks.find(|(name, _)| *name == STREAM_MIX).map(|(_, file)| file.to_string()).or(first)
}

#[tauri::command]
pub fn recorder_play(
    engine: State<'_, EngineState>,
    control: State<'_, RecorderControl>,
    track: Option<String>,
) -> Result<PreviewStatus, String> {
    let summary = control.last_summary().ok_or("Record something first")?;
    let folder = PathBuf::from(summary.folder);
    let session = read_session(&folder)?;
    let file = match track {
        Some(file) => file,
        None => default_track(&session).ok_or("The recording has no tracks")?,
    };
    let recorded = find_track(&folder, &session, &file)?;
    let readers = open_parts(&recorded.parts)?;
    let guard = lock(engine.inner());
    let running = guard.as_ref().ok_or("Start the engine to listen")?;
    running.preview.play(readers)?;
    Ok(running.preview.status())
}

#[tauri::command]
pub fn recorder_play_stop(engine: State<'_, EngineState>) -> PreviewStatus {
    let guard = lock(engine.inner());
    match guard.as_ref() {
        Some(running) => {
            running.preview.stop();
            running.preview.status()
        }
        None => IDLE,
    }
}

#[tauri::command]
pub fn recorder_play_status(engine: State<'_, EngineState>) -> PreviewStatus {
    lock(engine.inner()).as_ref().map_or(IDLE, |running| running.preview.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stream_mix_plays_by_default_else_the_first_track() {
        let none: [(&str, &str); 0] = [];
        assert_eq!(pick_default(none.into_iter()), None);
        let both = [("Pastor", "01 Pastor.wav"), ("Stream Mix", "Stream Mix.wav")];
        assert_eq!(pick_default(both.into_iter()).as_deref(), Some("Stream Mix.wav"));
        let strips = [("Pastor", "01 Pastor.wav"), ("Keys", "02 Keys.wav")];
        assert_eq!(pick_default(strips.into_iter()).as_deref(), Some("01 Pastor.wav"));
    }
}
