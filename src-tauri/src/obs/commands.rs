//! Tauri commands for OBS control. Every command returns `Result<_, String>`
//! with operator-facing text. Starting or stopping the stream/recording
//! requires `confirm: true` from the UI, because doing it by accident while
//! live is costly.
use super::config::{self, ObsConfigView};
use super::manager::ObsManager;
use super::scene_link::SceneLinkConfig;
use super::status::{self, ObsStatus};
use tauri::State;

/// Pure guard for the live-output commands.
pub fn require_confirm(confirm: bool) -> Result<(), String> {
    if confirm {
        Ok(())
    } else {
        Err(status::MSG_CONFIRM.to_string())
    }
}

fn clean_scene_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Choose an OBS scene".to_string());
    }
    Ok(name.to_string())
}

#[tauri::command]
pub fn obs_get_config(obs: State<'_, ObsManager>) -> Result<ObsConfigView, String> {
    Ok(obs.config_view())
}

/// `password`: `None` keeps the stored password, `Some("")` clears it.
/// `scene_link`: `None` keeps the stored scene link. Changing only the scene
/// link doesn't reconnect.
#[tauri::command]
pub fn obs_set_config(
    obs: State<'_, ObsManager>,
    host: String,
    port: u16,
    password: Option<String>,
    enabled: bool,
    scene_link: Option<SceneLinkConfig>,
) -> Result<ObsConfigView, String> {
    let mut next = obs.config();
    next.host = config::validate_host(&host)?;
    next.port = config::validate_port(port)?;
    if let Some(pw) = password {
        next.password = pw;
    }
    next.enabled = enabled;
    if let Some(link) = scene_link {
        next.scene_link = link.cleaned();
    }
    obs.update_config(next)
}

#[tauri::command]
pub fn obs_status(obs: State<'_, ObsManager>) -> Result<ObsStatus, String> {
    Ok(obs.status())
}

#[tauri::command]
pub async fn obs_set_scene(obs: State<'_, ObsManager>, name: String) -> Result<(), String> {
    let name = clean_scene_name(&name)?;
    let client = obs.require_client()?;
    client
        .scenes()
        .set_current_program_scene(name.as_str())
        .await
        .map_err(|e| status::friendly_error(&e))
}

#[tauri::command]
pub async fn obs_start_stream(obs: State<'_, ObsManager>, confirm: bool) -> Result<(), String> {
    require_confirm(confirm)?;
    let client = obs.require_client()?;
    client
        .streaming()
        .start()
        .await
        .map_err(|e| status::friendly_error(&e))
}

#[tauri::command]
pub async fn obs_stop_stream(obs: State<'_, ObsManager>, confirm: bool) -> Result<(), String> {
    require_confirm(confirm)?;
    let client = obs.require_client()?;
    client
        .streaming()
        .stop()
        .await
        .map_err(|e| status::friendly_error(&e))
}

#[tauri::command]
pub async fn obs_start_record(obs: State<'_, ObsManager>, confirm: bool) -> Result<(), String> {
    require_confirm(confirm)?;
    let client = obs.require_client()?;
    client
        .recording()
        .start()
        .await
        .map_err(|e| status::friendly_error(&e))
}

#[tauri::command]
pub async fn obs_stop_record(obs: State<'_, ObsManager>, confirm: bool) -> Result<(), String> {
    require_confirm(confirm)?;
    let client = obs.require_client()?;
    client
        .recording()
        .stop()
        .await
        .map(|_path| ())
        .map_err(|e| status::friendly_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_commands_need_confirm() {
        assert!(require_confirm(false).is_err());
        assert!(require_confirm(true).is_ok());
    }

    #[test]
    fn scene_names_are_trimmed_and_required() {
        assert_eq!(clean_scene_name("  Worship ").unwrap(), "Worship");
        assert!(clean_scene_name("   ").is_err());
    }
}
