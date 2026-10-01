//! Tauri commands for NDI® audio output. The logic lives in `ndi`.
//!
//! Settings take effect at the next engine start: the senders and their
//! rings are built with the engine. `ndi_set_outputs` says when a restart is
//! needed. Settings are saved to `app_data_dir()/ndi.json` and loaded at
//! startup (`load_saved_outputs`).
use crate::audio::{EngineState, RunningEngine};
use crate::ndi::persist;
use crate::ndi::runtime::{self, NdiStatus};
use crate::ndi::{NdiOutputs, NdiSettings};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the app data directory: {e}"))?;
    Ok(base.join(persist::SETTINGS_FILE))
}

/// Saved settings, or the defaults (with a logged warning) when the file is
/// missing or corrupt. Called once at app setup.
pub fn load_saved_outputs(app: &AppHandle) -> NdiOutputs {
    match settings_path(app) {
        Ok(path) => persist::load_from_path(&path),
        Err(err) => {
            log::warn!("NDI settings not loaded: {err}; using defaults");
            NdiOutputs::default()
        }
    }
}

#[tauri::command]
pub fn ndi_status() -> NdiStatus {
    runtime::ndi_status()
}

#[tauri::command]
pub fn ndi_get_outputs(settings: State<'_, NdiSettings>) -> NdiOutputs {
    settings.snapshot()
}

/// Store and save the outputs. Enabling any output without the runtime
/// installed is an error with install guidance (the settings are left
/// unchanged). A failed save keeps the new settings for this session and is
/// reported in `save_error`. Async so a runtime load still in progress
/// blocks a worker thread, not the UI thread.
#[tauri::command]
pub async fn ndi_set_outputs(
    app: AppHandle,
    settings: State<'_, NdiSettings>,
    engine: State<'_, EngineState>,
    outputs: NdiOutputs,
) -> Result<serde_json::Value, String> {
    let next = outputs.sanitized();
    if next.any_enabled() {
        runtime::runtime()?;
    }
    settings.replace(next.clone());
    let save_error = settings_path(&app)
        .and_then(|path| persist::save_to_path(&path, &next))
        .err();
    if let Some(err) = save_error.as_ref() {
        log::warn!("NDI settings not saved: {err}");
    }

    let (running, restart_required) = match engine.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(running) => (true, running.ndi_applied != next),
            None => (false, false),
        },
        Err(_) => (false, false),
    };
    let message = if restart_required {
        Some("Restart the engine to apply the NDI outputs.")
    } else {
        None
    };
    Ok(serde_json::json!({
        "outputs": next,
        "running": running,
        "restart_required": restart_required,
        "message": message,
        "save_error": save_error,
    }))
}

/// The `ndi` block of `engine_status`.
pub fn engine_ndi_status(engine: Option<&RunningEngine>) -> serde_json::Value {
    let available = runtime::ndi_status().available;
    match engine {
        Some(engine) => serde_json::json!({
            "available": available,
            "sending": engine.ndi_sending,
            "dropped_samples": engine.ndi_dropped_samples(),
            "inputs": crate::ndi_input_commands::engine_inputs_status(Some(engine)),
        }),
        None => serde_json::json!({
            "available": available,
            "sending": [],
            "dropped_samples": 0,
            "inputs": crate::ndi_input_commands::engine_inputs_status(None),
        }),
    }
}
