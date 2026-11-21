use super::auth::{Auth, Device, PairStatus};
use super::meters::{scope, MeterView, StripTap};
use super::tls::{self, ListenerSettings};
use super::{extensions, settings};
use super::{
    router_with_identity, Board, Change, Control, MixerView, Role, Status, Target, ViewControl,
};
use crate::audio::{EngineState, RunningEngine};
use crate::mixer_control::{bus_index, MixerControl};
use crate::mixer_layout::StripLayout;
use crate::mixer_meters::LatestMeters;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct RemoteMixer {
    listener: Mutex<Option<Listening>>,
    last_settings: Mutex<Option<ListenerSettings>>,
    start: tokio::sync::Mutex<()>,
    held_faders: Mutex<FaderHolds>,
}

const FADER_HOLD_TTL: Duration = Duration::from_secs(2);

#[derive(Default)]
struct FaderHolds(HashMap<String, Instant>);

impl FaderHolds {
    fn set(&mut self, id: String, down: bool, now: Instant) {
        if down {
            self.0.retain(|_, until| *until > now);
            self.0.insert(id, now + FADER_HOLD_TTL);
        } else {
            self.0.remove(&id);
        }
    }

    fn contains(&self, id: &str, now: Instant) -> bool {
        self.0.get(id).is_some_and(|until| *until > now)
    }
}

impl RemoteMixer {
    pub fn load(app: &AppHandle) -> Self {
        Self {
            last_settings: Mutex::new(settings::load(app)),
            ..Self::default()
        }
    }
}

struct Listening {
    url: String,
    fingerprint: String,
    auth: Arc<Mutex<Auth>>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    url: Option<String>,
    devices: Vec<Device>,
    pairing: Option<PairStatus>,
    fingerprint: Option<String>,
    settings: Option<ListenerSettings>,
}

fn layout(engine: Option<&RunningEngine>) -> (StripLayout, String) {
    let Some(running) = engine else {
        return (StripLayout::new(1, false, 0), "stopped".into());
    };
    let layout = running.strip_layout();
    let signature = layout_signature(
        layout,
        &running.input_device_identity,
        running
            .ndi_inputs
            .iter()
            .map(|source| (source.strip, source.source.as_str())),
    );
    (layout, signature)
}

fn layout_signature<'a>(
    layout: StripLayout,
    device: &str,
    ndi: impl IntoIterator<Item = (usize, &'a str)>,
) -> String {
    let mut digest = DefaultHasher::new();
    ("input", layout.hardware, device).hash(&mut digest);
    ("network slots", layout.net).hash(&mut digest);
    ("computer", layout.computer).hash(&mut digest);
    for (strip, source) in ndi {
        ("ndi", strip, source).hash(&mut digest);
    }
    format!("{:016x}", digest.finish())
}

fn bus_name(id: &str) -> &str {
    match id {
        "main" => "Main",
        "stream" => "Stream",
        "monitor" => "Mon 1",
        "monitor2" => "Mon 2",
        "monitor3" => "Mon 3",
        _ => id,
    }
}

struct DesktopBoard(AppHandle);

/// A meter reading older than this is a stopped or stalled engine: send none.
const METER_MAX_AGE: Duration = Duration::from_millis(500);

impl DesktopBoard {
    fn read(
        &self,
        role: Role,
        with_meters: bool,
    ) -> Result<(MixerView, Option<MeterView>), Status> {
        let engine = self.0.state::<EngineState>();
        let running = engine.try_lock().map_err(|_| Status::SERVICE_UNAVAILABLE)?;
        let (layout, signature) = layout(running.as_ref());
        let is_running = running.is_some();
        let state = self
            .0
            .state::<MixerControl>()
            .snapshot_layout(layout, is_running, Vec::new());
        drop(running);
        let meters = with_meters && is_running;
        let meters = meters
            .then(|| self.0.try_state::<LatestMeters>()?.fresh(METER_MAX_AGE))
            .flatten()
            .map(|latest| {
                let taps: Vec<StripTap> = state
                    .strips
                    .iter()
                    .map(|strip| StripTap {
                        muted: strip.mute || strip.group_muted,
                        post_fader: role == Role::Monitor1 && strip.monitor_post_fader,
                        soloed: strip.solo,
                    })
                    .collect();
                scope(role, &latest.strips, &taps, &latest.buses)
            });
        let extras = if role == Role::Desk {
            extensions::read(&state)?
        } else {
            Vec::new()
        };

        let mut controls: Vec<_> = state
            .strips
            .into_iter()
            .map(|strip| {
                let db = match role {
                    Role::Monitor1 => strip.send_monitor_db,
                    Role::Monitor2 => strip.send_monitor2_db,
                    Role::Monitor3 => strip.send_monitor3_db,
                    Role::Desk => strip.fader_db,
                };
                ViewControl {
                    target: Target::Strip { index: strip.index },
                    name: strip.name,
                    db,
                    muted: (role == Role::Desk).then_some(strip.mute),
                }
            })
            .collect();
        if role == Role::Desk {
            controls.extend(state.buses.iter().map(|bus| ViewControl {
                name: bus_name(&bus.id).into(),
                target: Target::Bus { id: bus.id.clone() },
                db: bus.fader_db,
                muted: Some(bus.mute),
            }));
        }
        if let Some(bus) = state
            .buses
            .iter()
            .find(|bus| role.monitor_bus() == Some(bus.id.as_str()))
        {
            controls.push(ViewControl {
                target: Target::Personal,
                name: bus_name(&bus.id).into(),
                db: 0.0,
                muted: Some(bus.mute),
            });
        }
        controls.extend(extras);
        let view = MixerView {
            role,
            running: is_running,
            layout: signature,
            controls,
        };
        Ok((view, meters))
    }
}

impl Board for DesktopBoard {
    fn view(&self, role: Role) -> Result<MixerView, Status> {
        Ok(self.read(role, false)?.0)
    }

    fn live(&self, role: Role) -> Result<(MixerView, Option<MeterView>), Status> {
        self.read(role, true)
    }

    fn apply(&self, role: Role, control: Control) -> Result<(), Status> {
        let mixer = self.0.state::<MixerControl>();
        let engine = self.0.state::<EngineState>();
        let guard = engine.try_lock().map_err(|_| Status::SERVICE_UNAVAILABLE)?;
        if guard.is_none() {
            return Err(Status::SERVICE_UNAVAILABLE);
        }
        let (layout, signature) = layout(guard.as_ref());
        if control.layout != signature {
            return Err(Status::CONFLICT);
        }
        if role == Role::Desk && matches!(control.change, Change::Level { .. }) {
            let fader = match &control.target {
                Target::Strip { index } => format!("strip:{index}"),
                Target::Bus { id } => format!("bus:{id}"),
                Target::Return { id } => format!("bus:{id}"),
                Target::Dca { index } => format!("dca:{index}"),
                Target::Personal => return Err(Status::FORBIDDEN),
            };
            let remote = self.0.state::<RemoteMixer>();
            let held = remote.held_faders.lock();
            if held.contains(&fader, Instant::now()) {
                return Err(Status::LOCKED);
            }
            return write_control(&mixer, layout, role, control);
        }
        write_control(&mixer, layout, role, control)
    }

    fn notify(&self) {
        let _ = self.0.emit("mixer-state-changed", ());
    }
}

// Inherent DCA methods take precedence when the optional group bank is present.
#[allow(dead_code)]
trait OptionalDca {
    fn set_dca_fader(&self, _: usize, _: f32) -> Result<(), String> {
        Err("DCA unavailable".into())
    }
    fn set_dca_mute(&self, _: usize, _: bool) -> Result<(), String> {
        Err("DCA unavailable".into())
    }
}
impl OptionalDca for MixerControl {}

fn write_control(
    mixer: &MixerControl,
    layout: StripLayout,
    role: Role,
    control: Control,
) -> Result<(), Status> {
    if !role.permits(&control) {
        return Err(Status::FORBIDDEN);
    }

    let result = match (control.target, control.change) {
        (Target::Strip { index }, change) if (index as usize) < layout.total() => match change {
            Change::Level { db } => {
                mixer.set_strip_param(index, role.monitor_key().unwrap_or("fader_db"), db)
            }
            Change::Mute { muted } => mixer.set_strip_bool(index, "mute", muted),
        },
        (Target::Bus { id }, change) if bus_index(&id).is_ok() => match change {
            Change::Level { db } => mixer.set_bus_param(&id, "fader_db", db),
            Change::Mute { muted } => mixer.set_bus_bool(&id, "mute", muted),
        },
        (Target::Return { id }, change) if ["fx_a", "fx_b"].contains(&id.as_str()) => {
            match change {
                Change::Level { db } => mixer.set_bus_param(&id, "fader_db", db),
                Change::Mute { muted } => mixer.set_bus_bool(&id, "mute", muted),
            }
        }
        (Target::Dca { index }, change) if index < 8 => match change {
            Change::Level { db } => mixer.set_dca_fader(index as usize, db),
            Change::Mute { muted } => mixer.set_dca_mute(index as usize, muted),
        },
        (Target::Personal, Change::Mute { muted }) => {
            mixer.set_bus_bool(role.monitor_bus().ok_or(Status::FORBIDDEN)?, "mute", muted)
        }
        _ => return Err(Status::BAD_REQUEST),
    };
    result.map_err(|_| Status::BAD_REQUEST)
}

#[tauri::command]
pub fn remote_mixer_fader_touch(remote: State<'_, RemoteMixer>, id: String, down: bool) {
    let Some((kind, key)) = id.split_once(':') else {
        return;
    };
    let valid = match kind {
        "strip" => key
            .parse::<usize>()
            .is_ok_and(|index| index < crate::mixer_control::MAX_STRIPS),
        "bus" => bus_index(key).is_ok() || ["fx_a", "fx_b"].contains(&key),
        "dca" => key.parse::<usize>().is_ok_and(|index| index < 8),
        _ => false,
    };
    if valid {
        remote.held_faders.lock().set(id, down, Instant::now());
    }
}

#[tauri::command]
pub async fn remote_mixer_start(
    app: AppHandle,
    remote: State<'_, RemoteMixer>,
    settings: ListenerSettings,
) -> Result<String, String> {
    let _start = remote
        .start
        .try_lock()
        .map_err(|_| "Remote mixer is starting")?;
    if remote.listener.lock().is_some() {
        return Err("Remote mixer already on".into());
    }
    let dir = settings::data_dir(&app)?;
    let saved = settings.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let prepared = saved.clone().prepare()?;
        settings::save(&dir, &saved)?;
        Ok::<_, String>(prepared)
    })
    .await
    .map_err(|_| "Cannot start remote mixer")??;
    let listener = tokio::net::TcpListener::from_std(prepared.listener)
        .map_err(|_| "Cannot start remote mixer listener")?;
    let url = format!("https://{}", prepared.host);
    let auth = Arc::new(Mutex::new(Auth::default()));
    let router = router_with_identity(
        Arc::new(DesktopBoard(app)),
        auth.clone(),
        &prepared.host,
        prepared.fingerprint.clone(),
    );
    let task =
        tauri::async_runtime::spawn(tls::serve(listener, prepared.tls, router, auth.clone()));
    *remote.last_settings.lock() = Some(settings);
    *remote.listener.lock() = Some(Listening {
        url: url.clone(),
        fingerprint: prepared.fingerprint,
        auth,
        task,
    });
    Ok(url)
}

#[tauri::command]
pub fn remote_mixer_stop(remote: State<'_, RemoteMixer>) {
    let listener = remote.listener.lock().take();
    drop(listener);
}

#[tauri::command]
pub fn remote_mixer_status(remote: State<'_, RemoteMixer>) -> SettingsView {
    let mut state = remote.listener.lock();
    if state
        .as_ref()
        .is_some_and(|server| server.task.inner().is_finished())
    {
        *state = None;
    }
    let mut view = SettingsView {
        url: None,
        fingerprint: None,
        pairing: None,
        devices: Vec::new(),
        settings: remote.last_settings.lock().clone(),
    };
    if let Some(server) = state.as_ref() {
        let mut auth = server.auth.lock();
        view.url = Some(server.url.clone());
        view.fingerprint = Some(server.fingerprint.clone());
        view.devices = auth.devices();
        view.pairing = Some(auth.pair_status());
    }
    view
}

#[tauri::command]
pub async fn remote_mixer_tailnet_address() -> Option<settings::TailnetAddress> {
    settings::detect().await
}

#[tauri::command]
pub async fn remote_mixer_tailnet_certificate(
    app: AppHandle,
    port: u16,
) -> Result<ListenerSettings, String> {
    settings::certificate(&app, port).await
}

fn with_auth<T>(remote: &RemoteMixer, change: impl FnOnce(&mut Auth) -> T) -> Result<T, String> {
    let state = remote.listener.lock();
    let server = state.as_ref().ok_or("Start the remote mixer")?;
    let result = change(&mut server.auth.lock());
    Ok(result)
}

#[tauri::command]
pub fn remote_mixer_pair_code(
    remote: State<'_, RemoteMixer>,
    role: Role,
) -> Result<String, String> {
    with_auth(remote.inner(), |auth| auth.issue(role))?
}

#[tauri::command]
pub fn remote_mixer_cancel_code(remote: State<'_, RemoteMixer>) -> Result<(), String> {
    with_auth(remote.inner(), Auth::cancel)
}

#[tauri::command]
pub fn remote_mixer_revoke(remote: State<'_, RemoteMixer>, id: u32) -> Result<(), String> {
    with_auth(remote.inner(), |auth| auth.revoke(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_write_reaches_only_the_assigned_send_and_valid_strips() {
        let mixer = MixerControl::new();
        let layout = StripLayout::new(2, false, 0);
        let send = |index| Control {
            layout: String::new(),
            target: Target::Strip { index },
            change: Change::Level { db: -12.0 },
        };
        write_control(&mixer, layout, Role::Monitor2, send(1)).unwrap();
        let state = mixer.snapshot_layout(layout, true, vec![]);
        assert_eq!(state.strips[1].send_monitor2_db, -12.0);
        assert_eq!(state.strips[1].send_monitor3_db, -90.0);
        assert_eq!(state.strips[1].send_monitor_db, 0.0);
        assert_eq!(state.strips[1].fader_db, -90.0);
        assert_eq!(
            write_control(&mixer, layout, Role::Monitor2, send(2)),
            Err(Status::BAD_REQUEST)
        );

        let mute = || Control {
            layout: String::new(),
            target: Target::Bus { id: "main".into() },
            change: Change::Mute { muted: true },
        };
        assert_eq!(
            write_control(&mixer, layout, Role::Monitor2, mute()),
            Err(Status::FORBIDDEN)
        );
        write_control(&mixer, layout, Role::Desk, mute()).unwrap();
        assert!(mixer.snapshot_layout(layout, true, vec![]).buses[0].mute);
    }

    #[test]
    fn monitor_can_only_mute_its_own_mix() {
        let mixer = MixerControl::new();
        let layout = StripLayout::new(1, false, 0);
        let mute = || Control {
            layout: String::new(),
            target: Target::Personal,
            change: Change::Mute { muted: true },
        };
        write_control(&mixer, layout, Role::Monitor2, mute()).unwrap();
        assert_eq!(
            write_control(&mixer, layout, Role::Desk, mute()),
            Err(Status::FORBIDDEN)
        );
        let muted: Vec<_> = mixer
            .snapshot_layout(layout, true, vec![])
            .buses
            .into_iter()
            .filter(|bus| bus.mute)
            .map(|bus| bus.id)
            .collect();
        assert_eq!(muted, ["monitor2"]);
    }

    #[test]
    fn layout_signature_depends_on_sources_and_unknown_bus_names_are_kept() {
        let layout = StripLayout::with_net(2, true, 1, 1);
        let sig = |device, source| layout_signature(layout, device, [(5, source)]);
        assert_eq!(
            sig("Interface A", "Source A"),
            sig("Interface A", "Source A")
        );
        assert_ne!(
            sig("Interface A", "Source A"),
            sig("Interface B", "Source A")
        );
        assert_ne!(
            sig("Interface A", "Source A"),
            sig("Interface A", "Source B")
        );
        assert_eq!(bus_name("future"), "future");
    }

    #[test]
    fn optional_controls_are_writable_only_when_their_banks_exist() {
        let mixer = MixerControl::new();
        let layout = StripLayout::new(1, false, 0);
        let visible = extensions::read(&mixer.snapshot_layout(layout, true, vec![])).unwrap();
        let change = |target| Control {
            layout: String::new(),
            target,
            change: Change::Level { db: -10.0 },
        };
        let fx = write_control(
            &mixer,
            layout,
            Role::Desk,
            change(Target::Return { id: "fx_a".into() }),
        );
        let dca = write_control(&mixer, layout, Role::Desk, change(Target::Dca { index: 0 }));
        assert_eq!(
            fx.is_ok(),
            visible
                .iter()
                .any(|view| matches!(&view.target, Target::Return { id } if id == "fx_a"))
        );
        assert_eq!(
            dca.is_ok(),
            visible
                .iter()
                .any(|view| matches!(&view.target, Target::Dca { index: 0 }))
        );
    }

    #[test]
    fn fader_hold_expires_without_refresh_and_refresh_keeps_it_protected() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut holds = super::FaderHolds::default();
        holds.set("strip:0".into(), true, now);
        holds.set("strip:1".into(), true, now);
        assert!(holds.contains("strip:0", now + Duration::from_secs(1)));
        holds.set("strip:0".into(), true, now + Duration::from_millis(1_500));
        assert!(!holds.contains("strip:1", now + Duration::from_secs(2)));
        assert!(holds.contains("strip:0", now + Duration::from_secs(2)));
        holds.set("strip:0".into(), false, now + Duration::from_secs(2));
        assert!(!holds.contains("strip:0", now + Duration::from_secs(2)));
    }
}
