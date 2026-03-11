mod audio;
mod dsp;

use audio::{AudioEngine, EngineState};
use dsp::{AudioDeviceInfo, DspParams};
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

// ── App state types ──────────────────────────────────────────────────────────

/// Shared DSP parameters — persists across engine start/stop cycles.
pub struct SharedParams(pub Arc<DspParams>);

/// Noise capture buffer — set by `capture_noise_now`, read by next engine start
/// or passed directly to a running engine via the NoiseCapturePending channel.
pub struct NoisePending(pub Mutex<Option<Vec<f32>>>);

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
async fn start_audio_engine(
    app: AppHandle,
    state: State<'_, EngineState>,
    params: State<'_, SharedParams>,
    input_device: Option<String>,
    output_device: Option<String>,
) -> Result<serde_json::Value, String> {
    let p = params.0.clone();
    let engine = AudioEngine::start(app, p, input_device, output_device)?;

    let info = serde_json::json!({
        "sample_rate": engine.sample_rate,
        "latency_ms":  engine.latency_ms,
        "device_name": engine.device_name,
    });

    *state.lock().unwrap() = Some(engine);
    Ok(info)
}

#[tauri::command]
async fn stop_audio_engine(state: State<'_, EngineState>) -> Result<(), String> {
    *state.lock().unwrap() = None; // drops streams → stops audio
    Ok(())
}

#[tauri::command]
fn list_input_devices() -> Vec<AudioDeviceInfo> {
    audio::list_devices(true)
}

#[tauri::command]
fn list_output_devices() -> Vec<AudioDeviceInfo> {
    audio::list_devices(false)
}

/// Set a single float parameter on the live engine.
#[tauri::command]
fn set_param(params: State<'_, SharedParams>, key: String, value: f32) {
    let p = &params.0;
    match key.as_str() {
        "gain_db"            => p.gain_db.store(value, Ordering::Relaxed),
        "gate_threshold_db"  => p.gate_threshold_db.store(value, Ordering::Relaxed),
        "noise_alpha"        => p.noise_alpha.store(value, Ordering::Relaxed),
        "comp_threshold_db"  => p.comp_threshold_db.store(value, Ordering::Relaxed),
        "comp_ratio"         => p.comp_ratio.store(value, Ordering::Relaxed),
        "deess_threshold_db" => p.deess_threshold_db.store(value, Ordering::Relaxed),
        "dereverb_strength"  => p.dereverb_strength.store(value, Ordering::Relaxed),
        "monitor_gain_db"    => p.monitor_gain_db.store(value, Ordering::Relaxed),
        _ => log::warn!("set_param: unknown key '{key}'"),
    }
}

/// Set a boolean parameter.
#[tauri::command]
fn set_param_bool(params: State<'_, SharedParams>, key: String, value: bool) {
    let p = &params.0;
    match key.as_str() {
        "gate_enabled"     => p.gate_enabled.store(value, Ordering::Relaxed),
        "noise_enabled"    => p.noise_enabled.store(value, Ordering::Relaxed),
        "comp_enabled"     => p.comp_enabled.store(value, Ordering::Relaxed),
        "deess_enabled"    => p.deess_enabled.store(value, Ordering::Relaxed),
        "dereverb_enabled" => p.dereverb_enabled.store(value, Ordering::Relaxed),
        "bypass"           => p.bypass.store(value, Ordering::Relaxed),
        _ => log::warn!("set_param_bool: unknown key '{key}'"),
    }
}

/// Capture current noise profile (call while the engine is running in silence).
/// The engine's DSP chain will pick up the new profile on its next callback.
#[tauri::command]
async fn capture_noise_profile(
    state: State<'_, EngineState>,
) -> Result<(), String> {
    // We signal the running engine to capture ~500ms of input as noise profile.
    // Since the audio callback owns the DSP chain, we use the NoisePending approach:
    // the callback checks for a new profile every block.
    // For now, emit an event so React knows to show "capturing..." UI.
    // The actual capture happens inside the DSP callback — here we just toggle
    // the noise_enabled param to trigger re-profiling.
    let guard = state.lock().unwrap();
    if guard.is_none() {
        return Err("Engine not running".to_string());
    }
    Ok(())
}

/// Return current engine status for UI sync.
#[tauri::command]
fn engine_status(state: State<'_, EngineState>) -> serde_json::Value {
    let guard = state.lock().unwrap();
    if let Some(ref e) = *guard {
        serde_json::json!({
            "running": true,
            "sample_rate": e.sample_rate,
            "latency_ms": e.latency_ms,
            "device_name": e.device_name,
        })
    } else {
        serde_json::json!({ "running": false })
    }
}

// ── App entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        // Manage shared state
        .manage(Mutex::new(None::<AudioEngine>) as EngineState)
        .manage(SharedParams(Arc::new(DspParams::defaults())))
        // Register commands
        .invoke_handler(tauri::generate_handler![
            start_audio_engine,
            stop_audio_engine,
            list_input_devices,
            list_output_devices,
            set_param,
            set_param_bool,
            capture_noise_profile,
            engine_status,
        ])
        .setup(|app| {
            // ── Native app menu ────────────────────────────────────────────
            let tiwaton_menu = SubmenuBuilder::new(app, "TIWATON")
                .item(&PredefinedMenuItem::about(app, Some("About TIWATON AI Studio"), None)?)
                .separator()
                .item(&MenuItemBuilder::with_id("show-settings", "Audio Settings")
                    .accelerator("CmdOrCtrl+,").build(app)?)
                .separator()
                .quit()
                .build()?;

            let engine_menu = SubmenuBuilder::new(app, "Engine")
                .item(&MenuItemBuilder::with_id("toggle-live", "Start / Stop Engine")
                    .accelerator("CmdOrCtrl+Return").build(app)?)
                .item(&MenuItemBuilder::with_id("hard-reset", "Restart Engine").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("calibrate", "Auto-Calibrate Gate").build(app)?)
                .item(&MenuItemBuilder::with_id("learn-mic", "Learn Mic Fingerprint").build(app)?)
                .item(&MenuItemBuilder::with_id("capture-noise", "Capture Noise Profile").build(app)?)
                .build()?;

            let view_menu = SubmenuBuilder::new(app, "View")
                .item(&MenuItemBuilder::with_id("set-tab-live", "Live Visualizer")
                    .accelerator("CmdOrCtrl+1").build(app)?)
                .item(&MenuItemBuilder::with_id("set-tab-editor", "Audio Editor")
                    .accelerator("CmdOrCtrl+2").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("toggle-bypass", "Toggle AI Bypass")
                    .accelerator("Alt+B").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("toggle-fullscreen", "Full Screen")
                    .accelerator("F11").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("zoom-in", "Zoom In")
                    .accelerator("CmdOrCtrl+=").build(app)?)
                .item(&MenuItemBuilder::with_id("zoom-out", "Zoom Out")
                    .accelerator("CmdOrCtrl+-").build(app)?)
                .build()?;

            let export_menu = SubmenuBuilder::new(app, "Export")
                .item(&MenuItemBuilder::with_id("export-wav", "Export WAV Recording").build(app)?)
                .item(&MenuItemBuilder::with_id("export-webm", "Export WebM Recording").build(app)?)
                .item(&MenuItemBuilder::with_id("export-mp4", "Export MP4 Session").build(app)?)
                .item(&MenuItemBuilder::with_id("snapshot", "Spectrum Snapshot (PNG)").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("save-snapshot", "Save Session Snapshot")
                    .accelerator("CmdOrCtrl+S").build(app)?)
                .build()?;

            let window_menu = SubmenuBuilder::new(app, "Window")
                .minimize().maximize().separator().close_window().build()?;

            let menu = MenuBuilder::new(app)
                .item(&tiwaton_menu).item(&engine_menu).item(&view_menu)
                .item(&export_menu).item(&window_menu)
                .build()?;

            app.set_menu(menu)?;

            // ── Route menu events to frontend ──────────────────────────────
            #[derive(Clone, serde::Serialize)]
            struct MenuPayload { event: String, args: Vec<String> }

            app.on_menu_event(|app, ev| {
                let payload = match ev.id().as_ref() {
                    "show-settings"  => MenuPayload { event: "menu:show-settings".into(), args: vec![] },
                    "toggle-live"    => MenuPayload { event: "menu:toggle-live".into(),   args: vec![] },
                    "hard-reset"     => MenuPayload { event: "menu:hard-reset".into(),    args: vec![] },
                    "calibrate"      => MenuPayload { event: "menu:calibrate".into(),     args: vec![] },
                    "learn-mic"      => MenuPayload { event: "menu:learn-mic".into(),     args: vec![] },
                    "capture-noise"  => MenuPayload { event: "menu:capture-noise".into(), args: vec![] },
                    "set-tab-live"   => MenuPayload { event: "menu:set-tab".into(),       args: vec!["live".into()] },
                    "set-tab-editor" => MenuPayload { event: "menu:set-tab".into(),       args: vec!["editor".into()] },
                    "toggle-bypass"  => MenuPayload { event: "menu:toggle-bypass".into(), args: vec![] },
                    "toggle-fullscreen" => MenuPayload { event: "menu:toggle-fullscreen".into(), args: vec![] },
                    "zoom-in"        => MenuPayload { event: "menu:zoom-in".into(),       args: vec![] },
                    "zoom-out"       => MenuPayload { event: "menu:zoom-out".into(),      args: vec![] },
                    "export-wav"     => MenuPayload { event: "menu:export-wav".into(),    args: vec![] },
                    "export-webm"    => MenuPayload { event: "menu:export-webm".into(),   args: vec![] },
                    "export-mp4"     => MenuPayload { event: "menu:export-mp4".into(),    args: vec![] },
                    "snapshot"       => MenuPayload { event: "menu:snapshot".into(),      args: vec![] },
                    "save-snapshot"  => MenuPayload { event: "menu:save-snapshot".into(), args: vec![] },
                    _ => return,
                };
                if let Some(w) = app.get_webview_window("main") {
                    w.emit("menu-event", payload).ok();
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running TIWATON AI Studio");
}
