//! `LinkRuntime`: the Tauri-managed handle behind the `link_*` commands.
//! It owns the server's lifetime and exposes snapshots for the panel.
use crate::link::activity::{ActivityEntry, NewEntry};
use crate::link::config::{self, LinkConfig};
use crate::link::lock;
use crate::link::rules::{self, Rule};
use crate::link::runtime::{now_epoch_ms, LinkShared};
use crate::link::server::{self, LinkBackend, RunningServer, BRIDGE_PATH};
use serde::Serialize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

/// `link_get_config` result. Contains the token: for the Studio UI only
/// (deliberately no `Debug`, so it cannot end up in a log line).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSnapshot {
    pub port: Option<u16>,
    pub url: Option<String>,
    pub token: String,
    pub enabled: bool,
    pub rules: Vec<Rule>,
    pub paused: bool,
    pub last_event_at: Option<u64>,
    pub server_error: Option<String>,
}

/// `link_activity` result, newest entry first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySnapshot {
    pub entries: Vec<ActivityEntry>,
    pub last_event_at: Option<u64>,
    pub paused: bool,
    pub current_scene: Option<String>,
}

/// Tauri-managed handle to the Link runtime.
pub struct LinkRuntime {
    shared: Arc<LinkShared>,
    server: Mutex<Option<RunningServer>>,
    server_error: Mutex<Option<String>>,
}

impl LinkRuntime {
    /// Load (or create) `link.json`, start the server when enabled and start
    /// the debounce timer. Never fails: problems are logged and shown in the
    /// panel (`serverError`).
    pub fn init(app: &AppHandle) -> LinkRuntime {
        let path = app
            .path()
            .app_data_dir()
            .map(|dir| dir.join(config::CONFIG_FILE))
            .map_err(|e| format!("cannot resolve the app data directory: {e}"));
        let loaded = path.clone().and_then(|p| config::load_or_create(&p));
        let config = loaded.unwrap_or_else(|e| {
            log::error!("link: {e}; Link settings will not be saved");
            LinkConfig::fresh().unwrap_or_else(|_| LinkConfig {
                token: String::new(),
                enabled: false,
                rules: rules::default_rules(),
            })
        });
        let enabled = config.enabled;
        let shared = LinkShared::start(app, path.unwrap_or_default(), config);
        let runtime = LinkRuntime {
            shared,
            server: Mutex::new(None),
            server_error: Mutex::new(None),
        };
        if enabled {
            let _ = runtime.start_server();
        }
        runtime
    }

    fn start_server(&self) -> Result<u16, String> {
        let mut slot = lock(&self.server);
        if let Some(running) = slot.as_ref() {
            return Ok(running.port());
        }
        let backend: Arc<dyn LinkBackend> = self.shared.clone();
        match server::start(backend) {
            Ok(running) => {
                let port = running.port();
                *slot = Some(running);
                *lock(&self.server_error) = None;
                Ok(port)
            }
            Err(e) => {
                let message = format!("cannot open a local port (4460-4469): {e}");
                log::error!("link: {message}");
                *lock(&self.server_error) = Some(message.clone());
                Err(message)
            }
        }
    }

    fn stop_server(&self) {
        let running = lock(&self.server).take();
        if running.is_some() {
            log::info!("link: server stopped");
        }
    }

    fn port(&self) -> Option<u16> {
        lock(&self.server).as_ref().map(RunningServer::port)
    }

    pub fn snapshot(&self) -> LinkSnapshot {
        let config = lock(&self.shared.config).clone();
        let port = self.port();
        LinkSnapshot {
            port,
            url: port.map(|p| format!("http://127.0.0.1:{p}{BRIDGE_PATH}")),
            token: config.token,
            enabled: config.enabled,
            rules: config.rules,
            paused: self.shared.paused.load(Relaxed),
            last_event_at: self.shared.last_event_at(),
            server_error: lock(&self.server_error).clone(),
        }
    }

    pub fn activity(&self) -> ActivitySnapshot {
        ActivitySnapshot {
            entries: lock(&self.shared.activity).list(),
            last_event_at: self.shared.last_event_at(),
            paused: self.shared.paused.load(Relaxed),
            current_scene: self.shared.current_scene(),
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<LinkSnapshot, String> {
        self.shared.update_config(|c| {
            Ok(LinkConfig {
                enabled,
                ..c.clone()
            })
        })?;
        if enabled {
            self.start_server()?;
        } else {
            self.stop_server();
            lock(&self.shared.debouncer).cancel();
        }
        Ok(self.snapshot())
    }

    pub fn set_rules(&self, new_rules: Vec<Rule>) -> Result<Vec<Rule>, String> {
        let cleaned = rules::validate_rules(&new_rules)?;
        let saved = self.shared.update_config(|c| {
            Ok(LinkConfig {
                rules: cleaned,
                ..c.clone()
            })
        })?;
        Ok(saved.rules)
    }

    pub fn regenerate_token(&self) -> Result<LinkSnapshot, String> {
        let token = config::generate_token()?;
        self.shared.update_config(|c| {
            Ok(LinkConfig {
                token,
                ..c.clone()
            })
        })?;
        log::info!("link: token regenerated");
        Ok(self.snapshot())
    }

    pub fn set_automation_paused(&self, paused: bool) -> LinkSnapshot {
        let was = self.shared.paused.swap(paused, Relaxed);
        if paused {
            lock(&self.shared.debouncer).cancel();
        }
        if was != paused {
            let action = if paused { "automation paused" } else { "automation resumed" };
            self.shared.log(NewEntry {
                ts: now_epoch_ms(),
                event: "operator".to_string(),
                action: action.to_string(),
                ..NewEntry::default()
            });
        }
        self.shared.emit_status();
        self.snapshot()
    }

    pub fn undo(&self, entry_id: u64) -> Result<ActivityEntry, String> {
        self.shared.undo(entry_id)
    }
}

