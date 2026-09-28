//! Owns the OBS connection: config, status snapshot, the live `obws` client
//! and the reconnect loop. All async work runs on Tauri's runtime.
//!
//! Every (re)start bumps `generation`; a loop whose generation is stale stops
//! at its next check and its status writes are ignored, so changing the
//! config never races an old connection.
use super::config::{self, ObsConfig, ObsConfigView};
use super::status::{self, ObsStatus};
use futures_util::StreamExt;
use obws::events::Event;
use obws::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const STATUS_EVENT: &str = "obs-status";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(1);

struct Inner {
    app: AppHandle,
    config_path: Option<PathBuf>,
    config: Mutex<ObsConfig>,
    status: Mutex<ObsStatus>,
    client: Mutex<Option<Arc<Client>>>,
    generation: AtomicU64,
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

    fn is_current(&self, gen: u64) -> bool {
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
                run_loop(mgr, gen).await;
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
            change(&mut s);
            if *s == before {
                return;
            }
            s.clone()
        };
        let _ = self.inner.app.emit(STATUS_EVENT, snapshot);
    }

    /// Like `apply_status`, but ignored when `gen` is no longer current.
    fn set_status<F: FnOnce(&mut ObsStatus)>(&self, gen: u64, change: F) {
        if self.is_current(gen) {
            self.apply_status(change);
        }
    }

    pub fn report_error(&self, message: String) {
        self.apply_status(|s| s.last_error = Some(message));
    }

    fn set_client(&self, gen: u64, client: Option<Arc<Client>>) {
        if !self.is_current(gen) {
            return;
        }
        if let Ok(mut slot) = self.inner.client.lock() {
            *slot = client;
        }
    }
}

async fn connect_once(cfg: &ObsConfig) -> Result<Client, String> {
    let password: Option<&str> = if cfg.password.is_empty() {
        None
    } else {
        Some(cfg.password.as_str())
    };
    let connect = Client::connect(cfg.host.as_str(), cfg.port, password);
    match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Err(_) => Err(status::MSG_NOT_RUNNING.to_string()),
        Ok(Err(e)) => Err(status::friendly_error(&e)),
        Ok(Ok(client)) => Ok(client),
    }
}

async fn run_loop(mgr: ObsManager, gen: u64) {
    let mut backoff = status::INITIAL_BACKOFF;
    loop {
        if !mgr.is_current(gen) {
            return;
        }
        mgr.set_status(gen, |s| s.connecting = true);
        let cfg = mgr.config();
        match connect_once(&cfg).await {
            Ok(client) => {
                backoff = status::INITIAL_BACKOFF;
                run_session(&mgr, gen, Arc::new(client)).await;
            }
            Err(message) => {
                log::info!("obs: connect to {}:{} failed: {message}", cfg.host, cfg.port);
                mgr.set_status(gen, |s| {
                    s.reset_connection();
                    s.last_error = Some(message);
                });
            }
        }
        if !mgr.is_current(gen) {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = status::next_backoff(backoff);
    }
}

/// One connected session; returns when OBS goes away or the config changes.
async fn run_session(mgr: &ObsManager, gen: u64, client: Arc<Client>) {
    // Subscribe before reading state so no change slips between the two.
    let events = match client.events() {
        Ok(events) => events,
        Err(e) => {
            mgr.set_status(gen, |s| s.last_error = Some(status::friendly_error(&e)));
            return;
        }
    };
    let mut events = Box::pin(events);
    if let Err(e) = refresh_all(mgr, gen, &client).await {
        mgr.set_status(gen, |s| s.last_error = Some(status::friendly_error(&e)));
        return;
    }
    mgr.set_client(gen, Some(Arc::clone(&client)));
    log::info!("obs: connected");

    loop {
        if !mgr.is_current(gen) {
            break;
        }
        match tokio::time::timeout(TICK, events.next()).await {
            Ok(Some(event)) => {
                if !handle_event(mgr, gen, event) {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => poll_timecodes(mgr, gen, &client).await,
        }
    }

    mgr.set_client(gen, None);
    mgr.set_status(gen, |s| {
        s.reset_connection();
        s.last_error = Some(status::MSG_LOST.to_string());
    });
    log::info!("obs: disconnected");
}

async fn refresh_all(mgr: &ObsManager, gen: u64, client: &Client) -> Result<(), obws::error::Error> {
    let version = client.general().version().await?;
    let scenes = client.scenes().list().await?;
    let stream = client.streaming().status().await?;
    let record = client.recording().status().await?;

    let names = status::order_scenes(
        scenes
            .scenes
            .iter()
            .map(|s| (s.id.name.clone(), s.index))
            .collect(),
    );
    let current = scenes.current_program_scene.map(|c| c.name);
    let obs_version = version.obs_version.to_string();
    let stream_tc = status::format_timecode(stream.timecode.whole_seconds());
    let record_tc = status::format_timecode(record.timecode.whole_seconds());

    mgr.set_status(gen, move |s| {
        s.connected = true;
        s.connecting = false;
        s.last_error = None;
        s.obs_version = Some(obs_version);
        s.scenes = names;
        s.current_scene = current;
        s.streaming = stream.active;
        s.recording = record.active;
        s.stream_timecode = if stream.active { Some(stream_tc) } else { None };
        s.record_timecode = if record.active { Some(record_tc) } else { None };
    });
    Ok(())
}

/// While live, refresh the stream/record timecodes once a second.
async fn poll_timecodes(mgr: &ObsManager, gen: u64, client: &Client) {
    let snapshot = mgr.status();
    if snapshot.streaming {
        if let Ok(stream) = client.streaming().status().await {
            let tc = status::format_timecode(stream.timecode.whole_seconds());
            mgr.set_status(gen, move |s| {
                if s.streaming {
                    s.stream_timecode = Some(tc);
                }
            });
        }
    }
    if snapshot.recording {
        if let Ok(record) = client.recording().status().await {
            let tc = status::format_timecode(record.timecode.whole_seconds());
            mgr.set_status(gen, move |s| {
                if s.recording {
                    s.record_timecode = Some(tc);
                }
            });
        }
    }
}

/// Apply one OBS event. Returns false when OBS is shutting down.
fn handle_event(mgr: &ObsManager, gen: u64, event: Event) -> bool {
    match event {
        Event::CurrentProgramSceneChanged { id } => {
            let name = id.name;
            mgr.set_status(gen, |s| s.current_scene = Some(name));
        }
        Event::SceneListChanged { scenes } => {
            let names = status::order_scenes(scenes.into_iter().map(|s| (s.name, s.index)).collect());
            mgr.set_status(gen, move |s| s.scenes = names);
        }
        Event::SceneNameChanged {
            old_name, new_name, ..
        } => {
            mgr.set_status(gen, move |s| {
                for scene in s.scenes.iter_mut() {
                    if *scene == old_name {
                        *scene = new_name.clone();
                    }
                }
                if s.current_scene.as_deref() == Some(old_name.as_str()) {
                    s.current_scene = Some(new_name);
                }
            });
        }
        Event::StreamStateChanged { active, .. } => {
            mgr.set_status(gen, move |s| {
                s.streaming = active;
                if !active {
                    s.stream_timecode = None;
                } else if s.stream_timecode.is_none() {
                    s.stream_timecode = Some(status::format_timecode(0));
                }
            });
        }
        Event::RecordStateChanged { active, .. } => {
            mgr.set_status(gen, move |s| {
                s.recording = active;
                if !active {
                    s.record_timecode = None;
                } else if s.record_timecode.is_none() {
                    s.record_timecode = Some(status::format_timecode(0));
                }
            });
        }
        Event::ExitStarted | Event::ServerStopping | Event::ServerStopped => return false,
        _ => {}
    }
    true
}
