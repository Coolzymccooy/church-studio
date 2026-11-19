//! Tauri commands for multitrack recording (design decision 9). The logic
//! lives in `recorder`; this file adds Tauri state, the clock, the
//! `<Documents>` folder, settings persistence and the `recorder-status`
//! event (about once a second while recording, and once when it ends).
use crate::audio::EngineState;
use crate::mixer_control::MixerControl;
use crate::recorder::config::{self, RecorderConfig, DEFAULT_FOLDER_NAME};
use crate::recorder::names::unique_name;
use crate::recorder::plan::{build_tracks, clamp_offset, stamp_for};
use crate::recorder::session::Marker;
use crate::recorder::status::{RecorderStatus, RecordingSummary};
use crate::recorder::writer::{self, StatusFn, WriterEvent, WriterPlan};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

pub const STATUS_EVENT: &str = "recorder-status";
const PATCH_INTERVAL: Duration = Duration::from_secs(5);
const STATUS_INTERVAL: Duration = Duration::from_secs(1);

/// Tauri-managed recorder settings and the last recording's summary.
#[derive(Default)]
pub struct RecorderControl {
    config: Mutex<RecorderConfig>,
    last: Arc<Mutex<Option<RecordingSummary>>>,
}

impl RecorderControl {
    pub fn config(&self) -> RecorderConfig {
        lock(&self.config).clone()
    }

    pub fn replace_config(&self, next: RecorderConfig) {
        *lock(&self.config) = next;
    }

    fn last_summary(&self) -> Option<RecordingSummary> {
        lock(&self.last).clone()
    }
}

/// A poisoned lock still holds valid data here (plain values).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderConfigPayload {
    pub folder: Option<String>,
    /// Where recordings go when `folder` is null.
    pub default_folder: Option<String>,
    pub include_main: bool,
    pub armed_strips: Option<Vec<u32>>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the app data directory: {e}"))?;
    Ok(base.join(config::SETTINGS_FILE))
}

fn default_root(app: &AppHandle) -> Result<PathBuf, String> {
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| format!("cannot resolve the Documents folder: {e}"))?;
    Ok(documents.join(DEFAULT_FOLDER_NAME))
}

/// Saved settings, or the defaults when missing or corrupt. Called once at
/// app setup.
pub fn load_saved_config(app: &AppHandle) -> RecorderConfig {
    match settings_path(app) {
        Ok(path) => config::load_from_path(&path),
        Err(err) => {
            log::warn!("recorder settings not loaded: {err}; using defaults");
            RecorderConfig::default()
        }
    }
}

fn config_payload(app: &AppHandle, config: &RecorderConfig) -> RecorderConfigPayload {
    RecorderConfigPayload {
        folder: config.folder.clone(),
        default_folder: default_root(app).ok().map(|p| p.display().to_string()),
        include_main: config.include_main,
        armed_strips: config.armed_strips.clone(),
    }
}

fn current_status(engine: &EngineState, control: &RecorderControl) -> RecorderStatus {
    let mut guard = lock(engine);
    let Some(running) = guard.as_mut() else {
        return RecorderStatus::idle(false, control.last_summary());
    };
    let recorder = &mut running.recorder;
    if !recorder.is_recording() {
        return RecorderStatus::idle(true, control.last_summary());
    }
    let (folder, tracks) = recorder
        .info()
        .map(|info| (info.folder.display().to_string(), info.track_names.clone()))
        .unwrap_or_default();
    RecorderStatus::recording(
        folder,
        tracks,
        recorder.elapsed_seconds(),
        recorder.dropped_frames(),
        recorder.marker_count(),
    )
}

#[tauri::command]
pub fn recorder_status(
    engine: State<'_, EngineState>,
    control: State<'_, RecorderControl>,
) -> RecorderStatus {
    current_status(engine.inner(), control.inner())
}

#[tauri::command]
pub fn recorder_get_config(app: AppHandle, control: State<'_, RecorderControl>) -> RecorderConfigPayload {
    config_payload(&app, &control.config())
}

/// Store and save the settings. They apply to the next recording. A chosen
/// folder must be an absolute path; null or blank means the default.
#[tauri::command]
pub fn recorder_set_config(
    app: AppHandle,
    control: State<'_, RecorderControl>,
    folder: Option<String>,
    include_main: bool,
    armed_strips: Option<Vec<u32>>,
) -> Result<RecorderConfigPayload, String> {
    if folder.as_deref().is_some_and(config::is_network_path) {
        return Err("Choose a folder on this computer; network folders are not supported".to_string());
    }
    let next = RecorderConfig {
        folder,
        include_main,
        armed_strips,
    }
    .sanitized();
    if let Some(folder) = next.folder.as_ref() {
        if !Path::new(folder).is_absolute() {
            return Err("Choose a full folder path for recordings".to_string());
        }
    }
    // Saved first: a failed save leaves the in-memory settings unchanged,
    // so what the UI shows is what the next app start will load.
    settings_path(&app).and_then(|path| config::save_to_path(&path, &next))?;
    control.replace_config(next.clone());
    Ok(config_payload(&app, &next))
}

/// Emits progress, and on finish stores the summary and emits it.
fn status_emitter(
    app: AppHandle,
    last: Arc<Mutex<Option<RecordingSummary>>>,
    folder: String,
    tracks: Vec<String>,
) -> StatusFn {
    Box::new(move |event: WriterEvent| {
        let status = match event {
            WriterEvent::Progress {
                elapsed_seconds,
                dropped_frames,
                markers,
            } => RecorderStatus::recording(folder.clone(), tracks.clone(), elapsed_seconds, dropped_frames, markers),
            WriterEvent::Finished(outcome) => {
                let summary = RecordingSummary::from_outcome(&outcome);
                if let Some(err) = summary.error.as_ref() {
                    log::error!("recorder: recording stopped: {err}");
                }
                *lock(&last) = Some(summary.clone());
                RecorderStatus::idle(true, Some(summary))
            }
        };
        let _ = app.emit(STATUS_EVENT, status);
    })
}

/// Strip display names, as the mixer shows them.
fn strip_names(mixer: &MixerControl, input_channels: u32, ndi_inputs: u32) -> Vec<String> {
    mixer
        .snapshot_with_ndi(input_channels, ndi_inputs, true, Vec::new())
        .strips
        .into_iter()
        .map(|strip| strip.name)
        .collect()
}

/// Start recording. `utcOffsetMinutes` is the UI's local offset, for the
/// folder name and `startedLocal` (UTC when absent).
///
/// Three phases so no disk I/O happens under the engine lock: plan (lock),
/// create the folder and files (`spawn_blocking`, no lock), then arm the
/// tap (lock), checking that the same engine is still running.
#[tauri::command]
pub async fn recorder_start(
    app: AppHandle,
    engine: State<'_, EngineState>,
    mixer: State<'_, MixerControl>,
    control: State<'_, RecorderControl>,
    title: Option<String>,
    utc_offset_minutes: Option<i32>,
) -> Result<RecorderStatus, String> {
    let config = control.config();
    let root = match config.folder.as_ref() {
        Some(folder) => PathBuf::from(folder),
        None => default_root(&app)?,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (folder, stamp) = stamp_for(
        now,
        clamp_offset(utc_offset_minutes),
        title.as_deref(),
        env!("CARGO_PKG_VERSION"),
    );

    // 1. Plan under the lock (no I/O).
    let (tracks, layout, sample_rate, shared) = {
        let mut guard = lock(engine.inner());
        let running = guard
            .as_mut()
            .ok_or_else(|| "Start the engine before recording".to_string())?;
        if running.recorder.is_recording() {
            return Err("A recording is already running".to_string());
        }
        let ndi_strips: Vec<usize> = running.ndi_inputs.iter().map(|input| input.strip).collect();
        let names = strip_names(mixer.inner(), running.input_channels, ndi_strips.len() as u32);
        let layout = running.recorder.layout();
        let tracks = build_tracks(layout, &names, &ndi_strips, &config);
        (tracks, layout, running.recorder.sample_rate(), running.recorder.shared())
    };

    // 2. Claim the folder and create every file without the lock.
    let (plan, prepared) = tauri::async_runtime::spawn_blocking(move || {
        let folder = unique_name(&folder, |name| root.join(name).exists());
        let plan = WriterPlan {
            dir: root.join(folder),
            sample_rate,
            layout,
            tracks,
            stamp,
            patch_interval: PATCH_INTERVAL,
            status_interval: STATUS_INTERVAL,
        };
        writer::prepare(&plan).map(|prepared| (plan, prepared))
    })
    .await
    .map_err(|e| format!("the recorder could not create its files: {e}"))??;

    // 3. Arm the tap, unless the engine stopped or restarted meanwhile.
    let on_status = status_emitter(
        app.clone(),
        control.last.clone(),
        plan.dir.display().to_string(),
        plan.tracks.iter().map(|t| t.name.clone()).collect(),
    );
    let leftover = {
        let mut guard = lock(engine.inner());
        match guard.as_mut().filter(|running| running.recorder.owns(&shared)) {
            Some(running) => {
                running.recorder.start_prepared(plan, prepared, on_status)?;
                None
            }
            None => Some((plan, prepared)),
        }
    };
    if let Some((plan, prepared)) = leftover {
        writer::abandon(&plan, prepared);
        return Err("The engine stopped while the recording was starting".to_string());
    }
    Ok(current_status(engine.inner(), control.inner()))
}

/// Stop, finalize and return the idle status with the summary. Only the
/// flag flip happens under the engine lock; the grace period, drain,
/// finalize and join run on a blocking thread with the lock released, so
/// meters, scene loads and other commands are never stalled by a stop.
#[tauri::command]
pub async fn recorder_stop(
    engine: State<'_, EngineState>,
    control: State<'_, RecorderControl>,
) -> Result<RecorderStatus, String> {
    let pending = {
        let mut guard = lock(engine.inner());
        let pending = guard.as_mut().and_then(|running| running.recorder.begin_stop());
        drop(guard);
        pending
    };
    if let Some(pending) = pending {
        // The outcome reaches the UI through the writer's `Finished` event
        // and `control.last`; here we only wait for the files to be final.
        let _outcome = tauri::async_runtime::spawn_blocking(move || pending.finish())
            .await
            .map_err(|e| format!("the recorder stop did not complete: {e}"))?;
    }
    Ok(current_status(engine.inner(), control.inner()))
}

#[tauri::command]
pub fn recorder_add_marker(engine: State<'_, EngineState>, label: String) -> Result<Marker, String> {
    let mut guard = lock(engine.inner());
    let running = guard.as_mut().ok_or_else(|| "Not recording".to_string())?;
    running.recorder.add_marker(&label, "operator")
}

/// Add a "scene" marker when a mixer scene loads (any route: operator,
/// Tiwaton Link or OBS). Does nothing unless recording.
pub fn mark_scene_loaded(engine: &EngineState, scene: &str) {
    let mut guard = lock(engine);
    if let Some(running) = guard.as_mut() {
        if running.recorder.is_recording() {
            let _ = running.recorder.add_marker(scene, "scene");
        }
    }
}

/// Open the last recording's folder (or the recordings folder) in the
/// system file manager. Only folders the recorder itself chose are opened.
#[tauri::command]
pub fn recorder_open_folder(app: AppHandle, control: State<'_, RecorderControl>) -> Result<(), String> {
    let last = control
        .last_summary()
        .map(|summary| PathBuf::from(summary.folder))
        .filter(|path| path.is_dir());
    let target = match last {
        Some(path) => path,
        None => control.config().folder.map(PathBuf::from).map_or_else(|| default_root(&app), Ok)?,
    };
    std::fs::create_dir_all(&target)
        .map_err(|e| format!("cannot open {}: {e}", target.display()))?;
    open_in_file_manager(&target)
}

fn open_in_file_manager(path: &Path) -> Result<(), String> {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot open {}: {e}", path.display()))
}
