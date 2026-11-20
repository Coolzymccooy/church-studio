//! Tauri commands for the computer-audio input. The logic lives in
//! `computer_audio`.
//!
//! The setting takes effect at the next engine start (the tap, ring and
//! strips are built with the engine); `computer_audio_set` says when a
//! restart is needed. It is saved to `app_data_dir()/computer_audio.json`
//! and loaded at startup (`load_saved`).
use crate::audio::EngineState;
use crate::computer_audio::settings::{self, SETTINGS_FILE};
use crate::computer_audio::{self, ComputerAudio, ComputerAudioSettings};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the app data directory: {e}"))?;
    Ok(base.join(SETTINGS_FILE))
}

/// The saved setting, or off when the file is missing or corrupt. Called
/// once at app setup.
pub fn load_saved(app: &AppHandle) -> ComputerAudio {
    match settings_path(app) {
        Ok(path) => settings::load(&path),
        Err(err) => {
            log::warn!("computer audio setting not loaded: {err}; off");
            ComputerAudio::default()
        }
    }
}

/// Whether a running engine was started with a different setting.
fn restart_required(engine: &EngineState, next: ComputerAudio) -> (bool, bool) {
    match engine.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(running) => (true, running.computer_audio_applied != next.enabled),
            None => (false, false),
        },
        Err(_) => (false, false),
    }
}

/// `{ enabled, permission, running, restart_required }`.
#[tauri::command]
pub fn computer_audio_get(
    settings: State<'_, ComputerAudioSettings>,
    engine: State<'_, EngineState>,
) -> serde_json::Value {
    let current = settings.snapshot();
    let (running, restart_required) = restart_required(engine.inner(), current);
    serde_json::json!({
        "enabled": current.enabled,
        "permission": computer_audio::permission(),
        "running": running,
        "restart_required": restart_required,
    })
}

/// Store and save the setting. A failed save keeps it for this session and
/// is reported in `save_error`.
#[tauri::command]
pub fn computer_audio_set(
    app: AppHandle,
    settings: State<'_, ComputerAudioSettings>,
    engine: State<'_, EngineState>,
    enabled: bool,
) -> serde_json::Value {
    let next = ComputerAudio { enabled };
    settings.replace(next);
    let save_error = settings_path(&app)
        .and_then(|path| settings::save(&path, &next))
        .err();
    if let Some(err) = save_error.as_ref() {
        log::warn!("computer audio setting not saved: {err}");
    }
    let (running, restart_required) = restart_required(engine.inner(), next);
    let message = restart_required.then_some("Restart the engine to apply computer audio.");
    serde_json::json!({
        "enabled": next.enabled,
        "permission": computer_audio::permission(),
        "running": running,
        "restart_required": restart_required,
        "message": message,
        "save_error": save_error,
    })
}
