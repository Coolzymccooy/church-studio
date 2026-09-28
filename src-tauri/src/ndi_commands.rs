//! Tauri commands for NDI® audio output. The logic lives in `ndi`.
//!
//! Settings take effect at the next engine start: the senders and their
//! rings are built with the engine. `ndi_set_outputs` says when a restart is
//! needed.
use crate::audio::{EngineState, RunningEngine};
use crate::ndi::runtime::{self, NdiStatus};
use crate::ndi::{NdiOutputs, NdiSettings};
use tauri::State;

#[tauri::command]
pub fn ndi_status() -> NdiStatus {
    runtime::ndi_status()
}

#[tauri::command]
pub fn ndi_get_outputs(settings: State<'_, NdiSettings>) -> NdiOutputs {
    settings.snapshot()
}

/// Store the outputs. Enabling any output without the runtime installed is
/// an error with install guidance (the settings are left unchanged).
#[tauri::command]
pub fn ndi_set_outputs(
    settings: State<'_, NdiSettings>,
    engine: State<'_, EngineState>,
    outputs: NdiOutputs,
) -> Result<serde_json::Value, String> {
    let next = outputs.sanitized();
    if next.any_enabled() {
        runtime::runtime()?;
    }
    settings.replace(next.clone());

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
        }),
        None => serde_json::json!({
            "available": available,
            "sending": [],
            "dropped_samples": 0,
        }),
    }
}
