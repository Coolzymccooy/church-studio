//! Tauri commands for network audio. The logic lives in `netaudio`.
//!
//! Settings take effect at the next engine start (rings, threads and Remote
//! strips are built with the engine); `net_audio_set` says when a restart
//! is needed. They are saved to `app_data_dir()/net_audio.json` and loaded
//! at startup (`load_saved`).
use crate::audio::EngineState;
use crate::netaudio::settings::{self, generate_passphrase, SETTINGS_FILE};
use crate::netaudio::{NetAudioConfig, NetAudioSettings};
use std::net::{IpAddr, UdpSocket};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve the app data directory: {e}"))?;
    Ok(base.join(SETTINGS_FILE))
}

/// Saved settings (network audio off with a fresh passphrase when the file
/// is missing or corrupt). A fresh passphrase is saved at once, so the one
/// the operator shares stays the same after a restart. Called once at app
/// setup.
pub fn load_saved(app: &AppHandle) -> NetAudioConfig {
    let path = match settings_path(app) {
        Ok(path) => path,
        Err(err) => {
            log::warn!("network audio settings not loaded: {err}; network audio is off");
            return NetAudioConfig::fresh();
        }
    };
    let existed = path.exists();
    let config = settings::load(&path);
    if !existed {
        if let Err(err) = settings::save(&path, &config) {
            log::warn!("network audio settings not saved: {err}");
        }
    }
    config
}

#[tauri::command]
pub fn net_audio_get(settings: State<'_, NetAudioSettings>) -> NetAudioConfig {
    settings.snapshot()
}

/// Store and save the settings. A failed save keeps them for this session
/// and is reported in `save_error`.
#[tauri::command]
pub fn net_audio_set(
    app: AppHandle,
    settings: State<'_, NetAudioSettings>,
    engine: State<'_, EngineState>,
    config: NetAudioConfig,
) -> serde_json::Value {
    let next = config.sanitized();
    settings.replace(next.clone());
    let save_error = settings_path(&app)
        .and_then(|path| settings::save(&path, &next))
        .err();
    if let Some(err) = save_error.as_ref() {
        log::warn!("network audio settings not saved: {err}");
    }
    let (running, restart_required) = match engine.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(running) => (true, running.net_audio.applied != next),
            None => (false, false),
        },
        Err(_) => (false, false),
    };
    serde_json::json!({
        "config": next,
        "running": running,
        "restart_required": restart_required,
        "problem": next.passphrase_problem().filter(|_| next.receive.enabled || next.send.enabled),
        "save_error": save_error,
    })
}

/// A new random passphrase (not stored until `net_audio_set`).
#[tauri::command]
pub fn net_audio_new_passphrase() -> String {
    generate_passphrase()
}

/// This machine's addresses a remote Studio can send to: the one on the
/// default route and the tailnet one, when there is one. Found by
/// "connecting" a UDP socket, which picks a route but sends nothing.
#[tauri::command]
pub fn net_audio_local_addresses() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for probe in ["192.0.2.1:9", "100.100.100.100:9"] {
        let Some(ip) = route_address(probe) else {
            continue;
        };
        let text = ip.to_string();
        if !ip.is_loopback() && !ip.is_unspecified() && !found.contains(&text) {
            found.push(text);
        }
    }
    found
}

fn route_address(probe: &str) -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect(probe).ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}
