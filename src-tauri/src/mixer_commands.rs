//! Tauri commands for the live mixer (see
//! `docs/specs/2026-09-27-mixer-live-contract.md`). The logic lives in
//! `mixer_control`; this file adds Tauri state access and scene files.
//!
//! Scenes are stored as JSON in `app_data_dir()/scenes/<slug>.json`. The
//! slug comes from `scene_slug`, which only allows `[a-z0-9_-]`, so a name
//! can never address a path outside that directory.
use crate::audio::EngineState;
use crate::mixer_control::{scene_slug, MixerControl, MixerState, StoredScene};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

const SCENES_DIR: &str = "scenes";

/// Optional listener told when the operator loads a scene through
/// `mixer_load_scene` (the OBS scene link registers one). Loads made by the
/// backend itself via `load_scene_state` don't call it, so a linked change
/// can't bounce back.
pub struct SceneLoadedHook(Box<dyn Fn(&str) + Send + Sync>);

impl SceneLoadedHook {
    pub fn new<F: Fn(&str) + Send + Sync + 'static>(f: F) -> Self {
        Self(Box::new(f))
    }
}

/// (input channel count, NDI input count, running). 1 channel and no NDI
/// inputs when stopped.
fn engine_info(engine: &EngineState) -> (u32, u32, bool) {
    match engine.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(running) => (
                running.input_channels.max(1),
                running.strip_layout().ndi as u32,
                true,
            ),
            None => (1, 0, false),
        },
        Err(_) => (1, 0, false),
    }
}

fn scenes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the app data directory: {e}"))?;
    Ok(base.join(SCENES_DIR))
}

fn scene_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let slug = scene_slug(name)?;
    Ok(scenes_dir(app)?.join(format!("{slug}.json")))
}

fn read_scene_file(path: &Path) -> Result<StoredScene, String> {
    let text = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "scene not found".to_string()
        } else {
            format!("cannot read scene: {e}")
        }
    })?;
    serde_json::from_str(&text).map_err(|e| format!("scene file is not valid: {e}"))
}

/// Display names of the saved scenes, sorted case-insensitively. A file that
/// cannot be parsed is listed by its slug so it can still be deleted.
pub(crate) fn list_scene_names(app: &AppHandle) -> Result<Vec<String>, String> {
    let dir = scenes_dir(app)?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot list scenes: {e}")),
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) => stem.to_string(),
            None => continue,
        };
        let name = match read_scene_file(&path) {
            Ok(scene) if scene_slug(&scene.name).ok().as_deref() == Some(stem.as_str()) => {
                scene.name
            }
            _ => stem,
        };
        names.push(name);
    }
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names)
}

fn scenes_or_empty(app: &AppHandle) -> Vec<String> {
    list_scene_names(app).unwrap_or_else(|e| {
        log::warn!("mixer: {e}");
        Vec::new()
    })
}

fn build_state(app: &AppHandle, mixer: &MixerControl, engine: &EngineState) -> MixerState {
    let (channels, ndi_inputs, running) = engine_info(engine);
    mixer.snapshot_with_ndi(channels, ndi_inputs, running, scenes_or_empty(app))
}

#[tauri::command]
pub fn mixer_state(
    app: AppHandle,
    mixer: State<'_, MixerControl>,
    engine: State<'_, EngineState>,
) -> MixerState {
    build_state(&app, mixer.inner(), engine.inner())
}

#[tauri::command]
pub fn mixer_set_strip_param(
    mixer: State<'_, MixerControl>,
    index: u32,
    key: String,
    value: f32,
) -> Result<(), String> {
    mixer.set_strip_param(index, &key, value)
}

#[tauri::command]
pub fn mixer_set_strip_bool(
    mixer: State<'_, MixerControl>,
    index: u32,
    key: String,
    value: bool,
) -> Result<(), String> {
    mixer.set_strip_bool(index, &key, value)
}

#[tauri::command]
pub fn mixer_rename_strip(
    mixer: State<'_, MixerControl>,
    index: u32,
    name: String,
) -> Result<(), String> {
    mixer.rename_strip(index, &name)
}

#[tauri::command]
pub fn mixer_set_bus_param(
    mixer: State<'_, MixerControl>,
    bus: String,
    key: String,
    value: f32,
) -> Result<(), String> {
    mixer.set_bus_param(&bus, &key, value)
}

#[tauri::command]
pub fn mixer_set_bus_bool(
    mixer: State<'_, MixerControl>,
    bus: String,
    key: String,
    value: bool,
) -> Result<(), String> {
    mixer.set_bus_bool(&bus, &key, value)
}

#[tauri::command]
pub fn mixer_list_scenes(app: AppHandle) -> Result<Vec<String>, String> {
    list_scene_names(&app)
}

#[tauri::command]
pub fn mixer_save_scene(
    app: AppHandle,
    mixer: State<'_, MixerControl>,
    name: String,
) -> Result<(), String> {
    let path = scene_path(&app, &name)?;
    let dir = scenes_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create the scenes folder: {e}"))?;
    let scene = mixer.capture_scene(&name);
    let json = serde_json::to_string_pretty(&scene).map_err(|e| e.to_string())?;
    // Write a temp file then rename, so a crash never leaves half a scene.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("cannot write scene: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot save scene: {e}")
    })
}

/// Load a saved scene into the mixer without notifying `SceneLoadedHook`.
pub(crate) fn load_scene_state(
    app: &AppHandle,
    mixer: &MixerControl,
    engine: &EngineState,
    name: &str,
) -> Result<MixerState, String> {
    let path = scene_path(app, name)?;
    let scene = read_scene_file(&path)?;
    mixer.apply_scene(&scene);
    Ok(build_state(app, mixer, engine))
}

#[tauri::command]
pub fn mixer_load_scene(
    app: AppHandle,
    mixer: State<'_, MixerControl>,
    engine: State<'_, EngineState>,
    name: String,
) -> Result<MixerState, String> {
    let state = load_scene_state(&app, mixer.inner(), engine.inner(), &name)?;
    if let Some(hook) = app.try_state::<SceneLoadedHook>() {
        (hook.0)(name.as_str());
    }
    Ok(state)
}

#[tauri::command]
pub fn mixer_delete_scene(app: AppHandle, name: String) -> Result<(), String> {
    let path = scene_path(&app, &name)?;
    fs::remove_file(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "scene not found".to_string()
        } else {
            format!("cannot delete scene: {e}")
        }
    })
}
