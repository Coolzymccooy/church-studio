use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    Manager,
};

#[derive(Clone, serde::Serialize)]
struct MenuPayload {
    event: String,
    args: Vec<String>,
}

impl MenuPayload {
    fn new(event: &str) -> Self {
        MenuPayload { event: event.to_string(), args: vec![] }
    }
    fn with_arg(event: &str, arg: &str) -> Self {
        MenuPayload { event: event.to_string(), args: vec![arg.to_string()] }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            // ── Build native application menu ───────────────────────────────
            let tiwaton_menu = SubmenuBuilder::new(app, "TIWATON")
                .item(&PredefinedMenuItem::about(app, Some("About TIWATON AI Studio"), None)?)
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
                .item(
                    &MenuItemBuilder::with_id("calibrate", "Auto-Calibrate Gate").build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("learn-mic", "Learn Mic Fingerprint").build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("capture-noise", "Capture Noise Profile")
                        .build(app)?,
                )
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
                .item(&PredefinedMenuItem::fullscreen(app, Some("Full Screen"))?)
                .separator()
                .item(&PredefinedMenuItem::zoom_in(app, Some("Zoom In"))?)
                .item(&PredefinedMenuItem::zoom_out(app, Some("Zoom Out"))?)
                .build()?;

            let export_menu = SubmenuBuilder::new(app, "Export")
                .item(
                    &MenuItemBuilder::with_id("export-wav", "Export WAV Recording").build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("export-webm", "Export WebM Recording")
                        .build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("export-mp4", "Export MP4 Session").build(app)?,
                )
                .item(
                    &MenuItemBuilder::with_id("snapshot", "Spectrum Snapshot (PNG)").build(app)?,
                )
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

            // ── Route native menu events to the frontend via Tauri events ───
            app.on_menu_event(|app, event| {
                let payload = match event.id().as_ref() {
                    "show-settings"   => MenuPayload::new("menu:show-settings"),
                    "toggle-live"     => MenuPayload::new("menu:toggle-live"),
                    "hard-reset"      => MenuPayload::new("menu:hard-reset"),
                    "calibrate"       => MenuPayload::new("menu:calibrate"),
                    "learn-mic"       => MenuPayload::new("menu:learn-mic"),
                    "capture-noise"   => MenuPayload::new("menu:capture-noise"),
                    "set-tab-live"    => MenuPayload::with_arg("menu:set-tab", "live"),
                    "set-tab-editor"  => MenuPayload::with_arg("menu:set-tab", "editor"),
                    "toggle-bypass"   => MenuPayload::new("menu:toggle-bypass"),
                    "export-wav"      => MenuPayload::new("menu:export-wav"),
                    "export-webm"     => MenuPayload::new("menu:export-webm"),
                    "export-mp4"      => MenuPayload::new("menu:export-mp4"),
                    "snapshot"        => MenuPayload::new("menu:snapshot"),
                    "save-snapshot"   => MenuPayload::new("menu:save-snapshot"),
                    _ => return,
                };
                if let Some(window) = app.get_webview_window("main") {
                    window.emit("menu-event", payload).ok();
                }
            });

            // ── Configure main window ────────────────────────────────────────
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("TIWATON AI Studio").ok();
                // Allow microphone access in the WebView
                #[cfg(target_os = "macos")]
                window.with_webview(|wv| {
                    use objc::runtime::Object;
                    use objc::{msg_send, sel, sel_impl};
                    unsafe {
                        let _: () = msg_send![wv.inner(), _allowMediaType: 3u32];
                    }
                })
                .ok();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running TIWATON AI Studio");
}
