//! Tauri commands for NDI® audio input. The logic lives in `ndi::receive`.
//!
//! The selection takes effect at the next engine start (receivers, rings
//! and strips are built with the engine); `ndi_set_inputs` says when a
//! restart is needed. It is saved to `app_data_dir()/ndi_inputs.json` and
//! loaded at startup (`load_saved_inputs`).
use crate::audio::{EngineState, RunningEngine};
use crate::ndi::receive::discover::{self, NdiSourceInfo};
use crate::ndi::receive::settings::{self as input_settings, INPUTS_FILE};
use crate::ndi::receive::{NdiInputSettings, NdiInputs};
use crate::ndi::runtime;
use crate::ndi::NdiSettings;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

fn inputs_path(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the app data directory: {e}"))?;
    Ok(base.join(INPUTS_FILE))
}

/// Saved inputs, or none when the file is missing or corrupt. Called once
/// at app setup.
pub fn load_saved_inputs(app: &AppHandle) -> NdiInputs {
    match inputs_path(app) {
        Ok(path) => input_settings::load_inputs(&path),
        Err(err) => {
            log::warn!("NDI inputs not loaded: {err}; using none");
            NdiInputs::default()
        }
    }
}

/// The NDI sources on the network, sorted, with this app's own outputs
/// flagged. Listening takes a second or two, so it runs on the blocking
/// pool, never on the UI thread.
#[tauri::command]
pub async fn ndi_list_sources(
    outputs: State<'_, NdiSettings>,
) -> Result<Vec<NdiSourceInfo>, String> {
    let own: Vec<String> = outputs
        .snapshot()
        .plan()
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = runtime::runtime()?;
        discover::list_sources(runtime, &own)
    })
    .await
    .map_err(|e| format!("NDI source search failed: {e}"))?
}

#[tauri::command]
pub fn ndi_get_inputs(settings: State<'_, NdiInputSettings>) -> NdiInputs {
    settings.snapshot()
}

/// Store and save the selected sources. Selecting any without a runtime
/// that can receive is an error with install guidance (the selection is
/// left unchanged). A failed save keeps the selection for this session and
/// is reported in `save_error`. Async so a runtime load still in progress
/// blocks a worker thread, not the UI thread.
#[tauri::command]
pub async fn ndi_set_inputs(
    app: AppHandle,
    settings: State<'_, NdiInputSettings>,
    engine: State<'_, EngineState>,
    inputs: NdiInputs,
) -> Result<serde_json::Value, String> {
    let next = inputs.sanitized();
    if next.any_enabled() {
        let runtime = runtime::runtime()?;
        if !runtime.supports_receive() {
            return Err(
                "This NDI Runtime cannot receive audio. Update NDI Tools from https://ndi.video/tools."
                    .to_string(),
            );
        }
    }
    settings.replace(next.clone());
    let save_error = inputs_path(&app)
        .and_then(|path| input_settings::save_inputs(&path, &next))
        .err();
    if let Some(err) = save_error.as_ref() {
        log::warn!("NDI inputs not saved: {err}");
    }

    let (running, restart_required) = match engine.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(running) => (true, running.ndi_inputs_applied != next),
            None => (false, false),
        },
        Err(_) => (false, false),
    };
    let message = if restart_required {
        Some("Restart the engine to apply the NDI inputs.")
    } else {
        None
    };
    Ok(serde_json::json!({
        "inputs": next,
        "running": running,
        "restart_required": restart_required,
        "message": message,
        "save_error": save_error,
    }))
}

/// The `inputs` list of `engine_status.ndi`.
pub fn engine_inputs_status(engine: Option<&RunningEngine>) -> serde_json::Value {
    let Some(engine) = engine else {
        return serde_json::json!([]);
    };
    let rate = engine.sample_rate.max(1) as f64;
    let inputs: Vec<serde_json::Value> = engine
        .ndi_inputs
        .iter()
        .map(|input| {
            let stats = &input.stats;
            let fill = stats.fill_frames.load(Ordering::Relaxed) as f64;
            serde_json::json!({
                "source": input.source,
                "strip": input.strip,
                "connected": stats.connected.load(Ordering::Relaxed),
                "fill_ms": (fill * 1000.0 / rate).round(),
                "dropped_samples": stats.dropped_samples.load(Ordering::Relaxed),
                "underruns": stats.underruns.load(Ordering::Relaxed),
                "skipped_frames": stats.skipped_frames.load(Ordering::Relaxed),
            })
        })
        .collect();
    serde_json::Value::Array(inputs)
}
