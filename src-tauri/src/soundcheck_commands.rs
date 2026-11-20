use crate::audio::{EngineState, RunningEngine};
use crate::soundcheck::{open_session, PlaybackStatus};
use std::{path::PathBuf, sync::Arc};
use tauri::State;

fn with_engine<T>(
    engine: &EngineState,
    action: impl FnOnce(&mut RunningEngine) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = engine.lock().map_err(|_| "Engine unavailable")?;
    action(guard.as_mut().ok_or("Start the engine for soundcheck")?)
}

#[tauri::command]
pub async fn soundcheck_load(
    engine: State<'_, EngineState>,
    path: String,
) -> Result<PlaybackStatus, String> {
    let (identity, inputs, rate) = with_engine(engine.inner(), |running| {
        Ok((
            running.playback.identity.clone(),
            running.strip_layout().total(),
            running.sample_rate,
        ))
    })?;
    let session = tauri::async_runtime::spawn_blocking(move || {
        open_session(&PathBuf::from(path), inputs, rate)
    })
    .await
    .map_err(|_| "Cannot open soundcheck session")??;
    with_engine(engine.inner(), |running| {
        if !Arc::ptr_eq(&identity, &running.playback.identity) {
            return Err("Engine restarted. Reopen session".into());
        }
        running.playback.load(session)?;
        Ok(running.playback.status())
    })
}

#[tauri::command]
pub fn soundcheck_unload(engine: State<'_, EngineState>) -> Result<(), String> {
    with_engine(engine.inner(), |running| running.playback.unload())
}

#[tauri::command]
pub fn soundcheck_set_source(
    engine: State<'_, EngineState>,
    index: usize,
    playback: bool,
) -> Result<(), String> {
    with_engine(engine.inner(), |running| {
        if playback && running.recorder.is_recording() {
            return Err("Stop recording for soundcheck".into());
        }
        running.playback.set_source(index, playback)
    })
}

#[tauri::command]
pub fn soundcheck_play(engine: State<'_, EngineState>) -> Result<(), String> {
    with_engine(engine.inner(), |running| {
        if running.recorder.is_recording() {
            return Err("Stop recording for soundcheck".into());
        }
        running.playback.play()
    })
}

#[tauri::command]
pub fn soundcheck_stop(engine: State<'_, EngineState>) -> Result<(), String> {
    with_engine(engine.inner(), |running| running.playback.stop())
}

#[tauri::command]
pub fn soundcheck_set_loop(engine: State<'_, EngineState>, enabled: bool) -> Result<(), String> {
    with_engine(engine.inner(), |running| running.playback.set_loop(enabled))
}

#[tauri::command]
pub fn soundcheck_locate(engine: State<'_, EngineState>, seconds: f64) -> Result<(), String> {
    with_engine(engine.inner(), |running| running.playback.locate(seconds))
}

#[tauri::command]
pub fn soundcheck_status(engine: State<'_, EngineState>) -> Result<PlaybackStatus, String> {
    let guard = engine.lock().map_err(|_| "Engine unavailable")?;
    Ok(guard
        .as_ref()
        .map_or_else(PlaybackStatus::stopped, |running| running.playback.status()))
}
