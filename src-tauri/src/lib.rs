mod audio;
mod dsp;

use audio::{EngineState, RunningEngine};
use dsp::{AudioDeviceInfo, DspParams};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(all(desktop, not(debug_assertions)))]
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
#[cfg(all(desktop, not(debug_assertions)))]
use tauri_plugin_updater::UpdaterExt;

/// Shared DSP parameters that persist across engine start/stop cycles.
pub struct SharedParams(pub Arc<DspParams>);

#[cfg(all(desktop, not(debug_assertions)))]
const UPDATE_CHECK_TIMEOUT_SECS: u64 = 15;

#[cfg(all(desktop, not(debug_assertions)))]
async fn install_app_update(app: AppHandle) -> tauri_plugin_updater::Result<()> {
    let Some(update) = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(UPDATE_CHECK_TIMEOUT_SECS))
        .build()?
        .check()
        .await?
    else {
        return Ok(());
    };

    let mut downloaded = 0_u64;
    update
        .download_and_install(
            |chunk_length, content_length| {
                downloaded += u64::try_from(chunk_length).unwrap_or(0);
                log::info!(
                    "updater: downloaded {} bytes of {:?}",
                    downloaded,
                    content_length
                );
            },
            || {
                log::info!("updater: download complete");
            },
        )
        .await?;

    log::info!("updater: update installed successfully");

    #[cfg(not(windows))]
    {
        let restart_handle = app.clone();
        app.dialog()
            .message("TIWATON AI Studio was updated successfully. Restart now to use the new version.")
            .title("Update installed")
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Restart now".into(),
                "Later".into(),
            ))
            .show(move |restart_now| {
                if restart_now {
                    restart_handle.restart();
                }
            });
    }

    Ok(())
}

#[cfg(all(desktop, not(debug_assertions)))]
async fn check_for_app_update(app: AppHandle) {
    let update = match app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(UPDATE_CHECK_TIMEOUT_SECS))
        .build()
    {
        Ok(builder) => match builder.check().await {
            Ok(update) => update,
            Err(err) => {
                log::warn!("updater: failed to check for updates: {err}");
                return;
            }
        },
        Err(err) => {
            log::warn!("updater: failed to initialize updater: {err}");
            return;
        }
    };

    let Some(update) = update else {
        log::info!("updater: no update available");
        return;
    };

    let version = update.version.clone();
    let install_handle = app.clone();
    app.dialog()
        .message(format!(
            "TIWATON AI Studio {version} is available.\n\nInstall the update now? The app may close while the installer runs."
        ))
        .title("Update available")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install now".into(),
            "Later".into(),
        ))
        .show(move |install_now| {
            if !install_now {
                return;
            }

            let app = install_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = install_app_update(app.clone()).await {
                    log::error!("updater: failed to install update: {err}");
                    app.dialog()
                        .message(format!(
                            "TIWATON AI Studio could not install the update.\n\n{err}"
                        ))
                        .title("Update failed")
                        .kind(MessageDialogKind::Error)
                        .buttons(MessageDialogButtons::Ok)
                        .show(|_| {});
                }
            });
        });
}

#[tauri::command]
async fn start_audio_engine(
    app: AppHandle,
    state: State<'_, EngineState>,
    params: State<'_, SharedParams>,
    input_device: Option<String>,
    monitor_output_device: Option<String>,
    broadcast_output_device: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut guard = state.inner().lock().unwrap();
    if guard.is_some() {
        return Err("Engine already running".to_string());
    }

    let engine = RunningEngine::spawn(
        app,
        params.0.clone(),
        input_device,
        monitor_output_device,
        broadcast_output_device,
    )?;
    let info = serde_json::json!({
        "sample_rate": engine.sample_rate,
        "buffer_frames": engine.buffer_frames,
        "latency_ms": engine.latency_ms,
        "callback_avg_ms": engine.callback_avg_ms(),
        "callback_peak_ms": engine.callback_peak_ms(),
        "cpu_load_pct": engine.cpu_load_pct(),
        "dropped_output_samples": engine.dropped_output_samples(),
        "device_name": engine.input_device_name,
        "input_device_name": engine.input_device_name,
        "monitor_output_name": engine.monitor_output_name,
        "broadcast_output_name": engine.broadcast_output_name,
    });

    *guard = Some(engine);
    Ok(info)
}

#[tauri::command]
async fn stop_audio_engine(state: State<'_, EngineState>) -> Result<(), String> {
    if let Some(engine) = state.inner().lock().unwrap().take() {
        engine.stop();
    }
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

#[tauri::command]
fn set_param(params: State<'_, SharedParams>, key: String, value: f32) {
    let p = &params.0;
    match key.as_str() {
        "gain_db" => p.gain_db.store(value, Ordering::Relaxed),
        "gate_threshold_db" => p.gate_threshold_db.store(value, Ordering::Relaxed),
        "noise_alpha" => p.noise_alpha.store(value, Ordering::Relaxed),
        "comp_threshold_db" => p.comp_threshold_db.store(value, Ordering::Relaxed),
        "comp_ratio" => p.comp_ratio.store(value, Ordering::Relaxed),
        "deess_threshold_db" => p.deess_threshold_db.store(value, Ordering::Relaxed),
        "dereverb_strength" => p.dereverb_strength.store(value, Ordering::Relaxed),
        "monitor_gain_db" => p.monitor_gain_db.store(value, Ordering::Relaxed),
        _ => log::warn!("set_param: unknown key '{key}'"),
    }
}

#[tauri::command]
fn set_param_bool(params: State<'_, SharedParams>, key: String, value: bool) {
    let p = &params.0;
    match key.as_str() {
        "gate_enabled" => p.gate_enabled.store(value, Ordering::Relaxed),
        "noise_enabled" => p.noise_enabled.store(value, Ordering::Relaxed),
        "comp_enabled" => p.comp_enabled.store(value, Ordering::Relaxed),
        "deess_enabled" => p.deess_enabled.store(value, Ordering::Relaxed),
        "dereverb_enabled" => p.dereverb_enabled.store(value, Ordering::Relaxed),
        "bypass" => p.bypass.store(value, Ordering::Relaxed),
        _ => log::warn!("set_param_bool: unknown key '{key}'"),
    }
}

#[tauri::command]
async fn capture_noise_profile(state: State<'_, EngineState>) -> Result<(), String> {
    let guard = state.inner().lock().unwrap();
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not running".to_string())?;
    engine.capture_noise_profile()
}

fn serialize_engine_status(engine: &RunningEngine) -> serde_json::Value {
    serde_json::json!({
        "running": true,
        "sample_rate": engine.sample_rate,
        "buffer_frames": engine.buffer_frames,
        "latency_ms": engine.latency_ms,
        "callback_avg_ms": engine.callback_avg_ms(),
        "callback_peak_ms": engine.callback_peak_ms(),
        "cpu_load_pct": engine.cpu_load_pct(),
        "device_name": engine.input_device_name,
        "input_device_name": engine.input_device_name,
        "monitor_output_name": engine.monitor_output_name,
        "broadcast_output_name": engine.broadcast_output_name,
        "noise_profile_ready": engine.noise_profile_ready(),
        "dropped_output_samples": engine.dropped_output_samples(),
    })
}

#[tauri::command]
fn engine_status(state: State<'_, EngineState>) -> serde_json::Value {
    let guard = state.inner().lock().unwrap();
    if let Some(engine) = guard.as_ref() {
        serialize_engine_status(engine)
    } else {
        serde_json::json!({
            "running": false,
            "buffer_frames": 0,
            "latency_ms": null,
            "callback_avg_ms": null,
            "callback_peak_ms": null,
            "cpu_load_pct": null,
            "input_device_name": null,
            "monitor_output_name": null,
            "broadcast_output_name": null,
            "noise_profile_ready": false,
            "dropped_output_samples": 0,
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .manage(Mutex::new(None::<RunningEngine>) as EngineState)
        .manage(SharedParams(Arc::new(DspParams::defaults())))
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
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            #[cfg(all(desktop, not(debug_assertions)))]
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    check_for_app_update(app_handle).await;
                });
            }

            let tiwaton_menu = SubmenuBuilder::new(app, "TIWATON")
                .item(&PredefinedMenuItem::about(
                    app,
                    Some("About TIWATON AI Studio"),
                    None,
                )?)
                .separator()
                .item(
                    &MenuItemBuilder::with_id("show-settings", "Audio Settings")
                        .accelerator("CmdOrCtrl+,")
                        .build(app)?,
                )
                .separator()
                .quit()
                .build()?;

            let engine_menu = SubmenuBuilder::new(app, "Engine")
                .item(
                    &MenuItemBuilder::with_id("toggle-live", "Start / Stop Engine")
                        .accelerator("CmdOrCtrl+Return")
                        .build(app)?,
                )
                .item(&MenuItemBuilder::with_id("hard-reset", "Restart Engine").build(app)?)
                .separator()
                .item(&MenuItemBuilder::with_id("calibrate", "Auto-Calibrate Gate").build(app)?)
                .item(&MenuItemBuilder::with_id("learn-mic", "Learn Mic Fingerprint").build(app)?)
                .item(&MenuItemBuilder::with_id("capture-noise", "Capture Noise Profile").build(app)?)
                .build()?;

            let view_menu = SubmenuBuilder::new(app, "View")
                .item(
                    &MenuItemBuilder::with_id("set-tab-live", "Live Visualizer")
                        .accelerator("CmdOrCtrl+1")
                        .build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("set-tab-editor", "Audio Editor")
                        .accelerator("CmdOrCtrl+2")
                        .build(app)?,
                )
                .separator()
                .item(
                    &MenuItemBuilder::with_id("toggle-bypass", "Toggle AI Bypass")
                        .accelerator("Alt+B")
                        .build(app)?,
                )
                .separator()
                .item(
                    &MenuItemBuilder::with_id("toggle-fullscreen", "Full Screen")
                        .accelerator("F11")
                        .build(app)?,
                )
                .separator()
                .item(
                    &MenuItemBuilder::with_id("zoom-in", "Zoom In")
                        .accelerator("CmdOrCtrl+=")
                        .build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("zoom-out", "Zoom Out")
                        .accelerator("CmdOrCtrl+-")
                        .build(app)?,
                )
                .build()?;

            let export_menu = SubmenuBuilder::new(app, "Export")
                .item(&MenuItemBuilder::with_id("export-wav", "Export WAV Recording").build(app)?)
                .item(&MenuItemBuilder::with_id("export-webm", "Export WebM Recording").build(app)?)
                .item(&MenuItemBuilder::with_id("export-mp4", "Export MP4 Session").build(app)?)
                .item(&MenuItemBuilder::with_id("snapshot", "Spectrum Snapshot (PNG)").build(app)?)
                .separator()
                .item(
                    &MenuItemBuilder::with_id("save-snapshot", "Save Session Snapshot")
                        .accelerator("CmdOrCtrl+S")
                        .build(app)?,
                )
                .build()?;

            let window_menu = SubmenuBuilder::new(app, "Window")
                .minimize()
                .maximize()
                .separator()
                .close_window()
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&tiwaton_menu)
                .item(&engine_menu)
                .item(&view_menu)
                .item(&export_menu)
                .item(&window_menu)
                .build()?;

            app.set_menu(menu)?;

            #[derive(Clone, serde::Serialize)]
            struct MenuPayload {
                event: String,
                args: Vec<String>,
            }

            app.on_menu_event(|app, ev| {
                let payload = match ev.id().as_ref() {
                    "show-settings" => MenuPayload {
                        event: "menu:show-settings".into(),
                        args: vec![],
                    },
                    "toggle-live" => MenuPayload {
                        event: "menu:toggle-live".into(),
                        args: vec![],
                    },
                    "hard-reset" => MenuPayload {
                        event: "menu:hard-reset".into(),
                        args: vec![],
                    },
                    "calibrate" => MenuPayload {
                        event: "menu:calibrate".into(),
                        args: vec![],
                    },
                    "learn-mic" => MenuPayload {
                        event: "menu:learn-mic".into(),
                        args: vec![],
                    },
                    "capture-noise" => MenuPayload {
                        event: "menu:capture-noise".into(),
                        args: vec![],
                    },
                    "set-tab-live" => MenuPayload {
                        event: "menu:set-tab".into(),
                        args: vec!["live".into()],
                    },
                    "set-tab-editor" => MenuPayload {
                        event: "menu:set-tab".into(),
                        args: vec!["editor".into()],
                    },
                    "toggle-bypass" => MenuPayload {
                        event: "menu:toggle-bypass".into(),
                        args: vec![],
                    },
                    "toggle-fullscreen" => MenuPayload {
                        event: "menu:toggle-fullscreen".into(),
                        args: vec![],
                    },
                    "zoom-in" => MenuPayload {
                        event: "menu:zoom-in".into(),
                        args: vec![],
                    },
                    "zoom-out" => MenuPayload {
                        event: "menu:zoom-out".into(),
                        args: vec![],
                    },
                    "export-wav" => MenuPayload {
                        event: "menu:export-wav".into(),
                        args: vec![],
                    },
                    "export-webm" => MenuPayload {
                        event: "menu:export-webm".into(),
                        args: vec![],
                    },
                    "export-mp4" => MenuPayload {
                        event: "menu:export-mp4".into(),
                        args: vec![],
                    },
                    "snapshot" => MenuPayload {
                        event: "menu:snapshot".into(),
                        args: vec![],
                    },
                    "save-snapshot" => MenuPayload {
                        event: "menu:save-snapshot".into(),
                        args: vec![],
                    },
                    _ => return,
                };

                if let Some(window) = app.get_webview_window("main") {
                    window.emit("menu-event", payload).ok();
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running TIWATON AI Studio");
}
