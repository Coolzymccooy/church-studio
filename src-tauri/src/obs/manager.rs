//! Owns the OBS connection: config, status snapshot, the live `obws` client
//! and the reconnect loop. All async work runs on Tauri's runtime.
//!
//! Every (re)start bumps `generation`; a loop whose generation is stale stops
//! at its next check and its status writes are ignored, so changing the
//! config never races an old connection.
use super::config::{self, ObsConfig, ObsConfigView};
use super::scene_link::{self, LinkSide, SceneLinkGuard};
use super::session;
use super::status::{self, ObsStatus};
use crate::audio::EngineState;
use crate::mixer_control::MixerControl;
use obws::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

pub const STATUS_EVENT: &str = "obs-status";
/// Emitted after the scene link loads a Studio scene, so the mixer UI refetches.
pub const SCENE_LOADED_EVENT: &str = "mixer-scene-loaded";

struct Inner {
    app: AppHandle,
    config_path: Option<PathBuf>,
    config: Mutex<ObsConfig>,
    status: Mutex<ObsStatus>,
    client: Mutex<Option<Arc<Client>>>,
    generation: AtomicU64,
    guard: Mutex<SceneLinkGuard>,
}

#[derive(Clone, serde::Serialize)]
struct SceneLoadedPayload {
    name: String,
    source: &'static str,
}

#[derive(Clone)]
pub struct ObsManager {
    inner: Arc<Inner>,
}

impl ObsManager {
    pub fn new(app: AppHandle) -> Self {
        let config_path = app
            .path()
            .app_data_dir()
            .ok()
            .map(|dir| dir.join(config::CONFIG_FILE));
        let cfg = match &config_path {
            Some(path) => config::load_config(path),
            None => ObsConfig::default(),
        };
        let status = ObsStatus {
            enabled: cfg.enabled,
            ..ObsStatus::default()
        };
        Self {
            inner: Arc::new(Inner {
                app,
                config_path,
                config: Mutex::new(cfg),
                status: Mutex::new(status),
                client: Mutex::new(None),
                generation: AtomicU64::new(0),
                guard: Mutex::new(SceneLinkGuard::default()),
            }),
        }
    }

    /// Called once at startup: connect if the operator left OBS enabled.
    pub fn start(&self) {
        if self.config().enabled {
            self.restart();
        }
    }

    pub fn config(&self) -> ObsConfig {
        match self.inner.config.lock() {
            Ok(cfg) => cfg.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn config_view(&self) -> ObsConfigView {
        self.config().view()
    }

    pub fn status(&self) -> ObsStatus {
        match self.inner.status.lock() {
            Ok(s) => s.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn client(&self) -> Option<Arc<Client>> {
        match self.inner.client.lock() {
            Ok(c) => c.clone(),
            Err(_) => None,
        }
    }

    pub fn require_client(&self) -> Result<Arc<Client>, String> {
        self.client()
            .ok_or_else(|| status::MSG_NOT_CONNECTED.to_string())
    }

    /// Persist a new config; reconnect only if connection fields changed.
    pub fn update_config(&self, next: ObsConfig) -> Result<ObsConfigView, String> {
        if let Some(path) = &self.inner.config_path {
            config::save_config(path, &next)?;
        } else {
            return Err("cannot resolve the app data directory".to_string());
        }
        let restart = {
            let mut cfg = match self.inner.config.lock() {
                Ok(c) => c,
                Err(poisoned) => poisoned.into_inner(),
            };
            let restart = cfg.connection_differs(&next);
            *cfg = next.clone();
            restart
        };
        if restart {
            self.restart();
        }
        Ok(next.view())
    }

    pub(super) fn is_current(&self, gen: u64) -> bool {
        self.inner.generation.load(Ordering::SeqCst) == gen
    }

    /// Drop any connection and, when enabled, start a fresh connect loop.
    fn restart(&self) {
        let gen = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut client) = self.inner.client.lock() {
            *client = None; // dropping the Client disconnects it
        }
        let enabled = self.config().enabled;
        self.apply_status(|s| {
            s.enabled = enabled;
            s.last_error = None;
            s.reset_connection();
        });
        if enabled {
            let mgr = self.clone();
            tauri::async_runtime::spawn(async move {
                session::run_loop(mgr, gen).await;
            });
        }
    }

    /// Change the status and emit `obs-status` if anything changed.
    fn apply_status<F: FnOnce(&mut ObsStatus)>(&self, change: F) {
        let snapshot = {
            let mut s = match self.inner.status.lock() {
                Ok(s) => s,
                Err(poisoned) => poisoned.into_inner(),
            };
            let before = s.clone();
            change(&mut *s);
            if *s == before {
                return;
            }
            s.clone()
        };
        let _ = self.inner.app.emit(STATUS_EVENT, snapshot);
    }

    /// Like `apply_status`, but ignored when `gen` is no longer current.
    pub(super) fn set_status<F: FnOnce(&mut ObsStatus)>(&self, gen: u64, change: F) {
        if self.is_current(gen) {
            self.apply_status(change);
        }
    }

    pub fn report_error(&self, message: String) {
        self.apply_status(|s| s.last_error = Some(message));
    }

    fn note_caused(&self, side: LinkSide, scene: &str) {
        if let Ok(mut guard) = self.inner.guard.lock() {
            guard.note_caused(side, scene, Instant::now());
        }
    }

    fn is_echo(&self, side: LinkSide, scene: &str) -> bool {
        match self.inner.guard.lock() {
            Ok(mut guard) => guard.is_echo(side, scene, Instant::now()),
            Err(_) => false,
        }
    }

    /// Studio → OBS. Called through `SceneLoadedHook` after the operator
    /// loads a Studio scene; switches OBS to the mapped scene, if any.
    pub fn on_studio_scene_loaded(&self, studio_scene: &str) {
        let link = self.config().scene_link;
        let snapshot = self.status();
        if !snapshot.connected {
            return;
        }
        let Some(target) = scene_link::studio_to_obs(&link, studio_scene, &snapshot.scenes) else {
            return;
        };
        let already_there = snapshot
            .current_scene
            .as_deref()
            .map_or(false, |current| scene_link::same_scene(current, &target));
        if already_there {
            return;
        }
        let Some(client) = self.client() else {
            return;
        };
        self.note_caused(LinkSide::Obs, &target);
        let mgr = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = client
                .scenes()
                .set_current_program_scene(target.as_str())
                .await;
            if let Err(e) = result {
                mgr.report_error(status::friendly_error(&e));
            }
        });
    }

    /// OBS → Studio. Loads the mapped Studio scene unless this OBS change is
    /// the echo of one we just made.
    pub(super) fn follow_obs_scene(&self, obs_scene: &str) {
        if self.is_echo(LinkSide::Obs, obs_scene) {
            return;
        }
        let link = self.config().scene_link;
        if !link.enabled {
            return;
        }
        let app = &self.inner.app;
        let studio_scenes = match crate::mixer_commands::list_scene_names(app) {
            Ok(names) => names,
            Err(e) => {
                log::warn!("obs: scene link cannot list Studio scenes: {e}");
                return;
            }
        };
        let Some(target) = scene_link::obs_to_studio(&link, obs_scene, &studio_scenes) else {
            return;
        };
        let mixer = app.state::<MixerControl>();
        let engine = app.state::<EngineState>();
        match crate::mixer_commands::load_scene_state(app, mixer.inner(), engine.inner(), &target) {
            Ok(_) => {
                log::info!("obs: scene link loaded Studio scene \"{target}\"");
                let payload = SceneLoadedPayload {
                    name: target,
                    source: "obs",
                };
                let _ = app.emit(SCENE_LOADED_EVENT, payload);
            }
            Err(e) => self.report_error(format!("Scene link couldn't load \"{target}\": {e}")),
        }
    }

    pub(super) fn set_client(&self, gen: u64, client: Option<Arc<Client>>) {
        if !self.is_current(gen) {
            return;
        }
        if let Ok(mut slot) = self.inner.client.lock() {
            *slot = client;
        }
    }
}
