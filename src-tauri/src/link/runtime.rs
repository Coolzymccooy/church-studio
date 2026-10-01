//! Tauri glue for Tiwaton Link: `LinkShared` processes Lumina events (it is
//! the server's `LinkBackend`), runs the debounce timer and loads scenes
//! through the mixer's own recall path (`mixer_commands::load_scene_from_app`).
//! `handle::LinkRuntime` wraps it for the Tauri commands.
//!
//! Events emitted to the UI:
//! - `link-activity`: one `ActivityEntry` per log entry;
//! - `link-status`: `{lastEventAt, paused, pendingScene}` on every request;
//! - `mixer-state-changed`: the new `MixerState` after an automatic load or
//!   an undo, so an open mixer view refreshes.
use crate::audio::EngineState;
use crate::link::activity::{ActivityEntry, ActivityLog, NewEntry};
use crate::link::config::{self, LinkConfig};
use crate::link::lock;
use crate::link::protocol::{self, LuminaEvent};
use crate::link::rules::{self, Debouncer, RuleInput, SceneToLoad};
use crate::link::server::{self, LinkBackend, StatusInfo};
use crate::mixer_commands::{self, CurrentScene};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT_ACTIVITY: &str = "link-activity";
pub const EVENT_STATUS: &str = "link-status";
pub const EVENT_MIXER_STATE: &str = "mixer-state-changed";
const TICK: Duration = Duration::from_millis(200);

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn same_scene(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    last_event_at: Option<u64>,
    paused: bool,
    pending_scene: Option<String>,
}

pub struct LinkShared {
    app: AppHandle,
    config_path: PathBuf,
    pub(super) config: Mutex<LinkConfig>,
    pub(super) paused: AtomicBool,
    /// Unix ms of the last request from Lumina; 0 = never.
    last_event_at: AtomicU64,
    pub(super) debouncer: Mutex<Debouncer>,
    pub(super) activity: Mutex<ActivityLog>,
    clock: Instant,
}

impl LinkShared {
    /// Build the shared state and start its debounce timer.
    pub(super) fn start(app: &AppHandle, config_path: PathBuf, config: LinkConfig) -> Arc<Self> {
        let shared = Arc::new(LinkShared {
            app: app.clone(),
            config_path,
            config: Mutex::new(config),
            paused: AtomicBool::new(false),
            last_event_at: AtomicU64::new(0),
            debouncer: Mutex::new(Debouncer::new(rules::DEBOUNCE_MS)),
            activity: Mutex::new(ActivityLog::default()),
            clock: Instant::now(),
        });
        spawn_ticker(&shared);
        shared
    }

    fn mono_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub(super) fn last_event_at(&self) -> Option<u64> {
        match self.last_event_at.load(Relaxed) {
            0 => None,
            ts => Some(ts),
        }
    }

    pub(super) fn current_scene(&self) -> Option<String> {
        self.app.try_state::<CurrentScene>().and_then(|c| c.get())
    }

    fn engine_running(&self) -> bool {
        self.app
            .try_state::<EngineState>()
            .map_or(false, |engine| {
                let running = lock(engine.inner()).is_some();
                running
            })
    }

    pub(super) fn emit_status(&self) {
        let payload = StatusEvent {
            last_event_at: self.last_event_at(),
            paused: self.paused.load(Relaxed),
            pending_scene: lock(&self.debouncer).pending().map(|t| t.scene.clone()),
        };
        let _ = self.app.emit(EVENT_STATUS, payload);
    }

    pub(super) fn log(&self, entry: NewEntry) -> ActivityEntry {
        let stored = lock(&self.activity).push(entry);
        let _ = self.app.emit(EVENT_ACTIVITY, &stored);
        stored
    }

    /// Log unless the newest entry already says the same thing (a missing
    /// scene would otherwise be reported on every lyric slide).
    fn log_once(&self, entry: NewEntry) {
        let repeated = lock(&self.activity).last_action() == Some(entry.action.as_str());
        if !repeated {
            self.log(entry);
        }
    }

    fn entry_for(&self, ev: &LuminaEvent, action: String) -> NewEntry {
        NewEntry {
            ts: now_epoch_ms(),
            event: ev.event.clone(),
            content_kind: ev.content_kind().map(str::to_string),
            action,
            previous_scene: None,
            loaded_scene: None,
        }
    }

    fn process(&self, ev: LuminaEvent) -> String {
        self.last_event_at.store(now_epoch_ms(), Relaxed);
        let reply = self.route(&ev);
        self.emit_status();
        reply
    }

    fn route(&self, ev: &LuminaEvent) -> String {
        if !protocol::is_known_event(&ev.event) {
            log::info!("link: ignored unknown event '{}'", ev.event);
            self.log(self.entry_for(ev, "ignored: unknown event".to_string()));
            return "Unknown event ignored.".to_string();
        }
        let rule_list = lock(&self.config).rules.clone();
        let scenes = mixer_commands::list_scene_names(&self.app).unwrap_or_default();
        let lookup = |name: &str| -> Option<String> {
            scenes.iter().find(|s| same_scene(s, name)).cloned()
        };
        let result = rules::match_rules(&rule_list, &RuleInput::from_event(ev), &lookup);
        for skipped in &result.skipped {
            let action = format!(
                "skipped: scene '{}' not found (rule {})",
                skipped.scene, skipped.rule_id
            );
            log::warn!("link: {action}");
            self.log_once(self.entry_for(ev, action));
        }
        let target = result.target.map(|m| SceneToLoad {
            scene: m.scene,
            rule_id: m.rule_id,
            event: ev.event.clone(),
            content_kind: ev.content_kind().map(str::to_string),
        });
        if target.is_none() && !rules::changes_live_content(&ev.event) {
            return "Event received.".to_string();
        }
        let current = self.current_scene();
        if self.paused.load(Relaxed) {
            lock(&self.debouncer).cancel();
            if let Some(t) = target.as_ref() {
                if !current.as_deref().map_or(false, |c| same_scene(c, &t.scene)) {
                    self.log_once(self.entry_for(ev, format!("paused: would load {}", t.scene)));
                }
            }
            return "Automation is paused in the Studio.".to_string();
        }
        let due = {
            let mut debouncer = lock(&self.debouncer);
            debouncer.set_current(current);
            debouncer.on_event(target, self.mono_ms())
        };
        if let Some(t) = due {
            self.apply(t);
        }
        "Event accepted.".to_string()
    }

    /// Timer tick: fire a pending change that has become stable.
    fn tick(&self) {
        if self.paused.load(Relaxed) {
            return;
        }
        let due = lock(&self.debouncer).tick(self.mono_ms());
        if let Some(t) = due {
            self.apply(t);
            self.emit_status();
        }
    }

    fn apply(&self, t: SceneToLoad) {
        let previous = self.current_scene();
        match mixer_commands::load_scene_from_app(&self.app, &t.scene) {
            Ok(state) => {
                let _ = self.app.emit(EVENT_MIXER_STATE, &state);
                let action = format!("loaded {}", t.scene);
                log::info!("link: {action} (rule {})", t.rule_id);
                self.log(NewEntry {
                    ts: now_epoch_ms(),
                    event: t.event,
                    content_kind: t.content_kind,
                    action,
                    previous_scene: previous,
                    loaded_scene: Some(t.scene),
                });
            }
            Err(e) => {
                lock(&self.debouncer).set_current(previous);
                log::warn!("link: could not load scene '{}': {e}", t.scene);
                let action = format!("failed: {} ({e})", t.scene);
                self.log(NewEntry {
                    ts: now_epoch_ms(),
                    event: t.event,
                    content_kind: t.content_kind,
                    action,
                    previous_scene: None,
                    loaded_scene: None,
                });
            }
        }
    }

    pub(super) fn undo(&self, entry_id: u64) -> Result<ActivityEntry, String> {
        let entry = lock(&self.activity)
            .find(entry_id)
            .ok_or_else(|| "activity entry not found".to_string())?;
        let target = match (entry.undoable, entry.previous_scene) {
            (true, Some(scene)) => scene,
            _ => return Err("this entry cannot be undone".to_string()),
        };
        let previous = self.current_scene();
        let state = mixer_commands::load_scene_from_app(&self.app, &target)?;
        {
            let mut debouncer = lock(&self.debouncer);
            debouncer.cancel();
            debouncer.set_current(Some(target.clone()));
        }
        let _ = self.app.emit(EVENT_MIXER_STATE, &state);
        Ok(self.log(NewEntry {
            ts: now_epoch_ms(),
            event: "undo".to_string(),
            content_kind: None,
            action: format!("undo: loaded {target}"),
            previous_scene: previous,
            loaded_scene: Some(target),
        }))
    }

    pub(super) fn update_config<F>(&self, change: F) -> Result<LinkConfig, String>
    where
        F: FnOnce(&LinkConfig) -> Result<LinkConfig, String>,
    {
        let mut guard = lock(&self.config);
        let next = change(&*guard)?;
        config::save(&self.config_path, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

impl LinkBackend for LinkShared {
    fn token(&self) -> String {
        lock(&self.config).token.clone()
    }

    fn accepting(&self) -> bool {
        lock(&self.config).enabled
    }

    fn handle_event(&self, event: LuminaEvent) -> String {
        self.process(event)
    }

    fn status(&self) -> StatusInfo {
        StatusInfo {
            app: server::APP_ID.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            engine_running: self.engine_running(),
            current_scene: self.current_scene(),
            bus_loudness: None,
        }
    }
}

/// Debounce timer: a small thread that stops when the runtime is dropped.
fn spawn_ticker(shared: &Arc<LinkShared>) {
    let weak = Arc::downgrade(shared);
    let spawned = std::thread::Builder::new()
        .name("link-debounce".to_string())
        .spawn(move || loop {
            std::thread::sleep(TICK);
            match weak.upgrade() {
                Some(shared) => shared.tick(),
                None => break,
            }
        });
    if let Err(e) = spawned {
        log::error!("link: cannot start the debounce timer: {e}");
    }
}
