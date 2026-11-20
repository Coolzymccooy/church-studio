use super::auth::{Auth, Device};
use super::tls::{self, ListenerSettings};
use super::{router, Board, Change, Control, MixerView, Role, Status, Target, ViewControl};
use crate::audio::{EngineState, RunningEngine};
use crate::mixer_control::{bus_index, MixerControl};
use crate::mixer_layout::StripLayout;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct RemoteMixer(Mutex<Option<Listening>>);

struct Listening {
    url: String,
    auth: Arc<Mutex<Auth>>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Serialize)]
pub struct SettingsView {
    url: Option<String>,
    devices: Vec<Device>,
}

fn layout(engine: Option<&RunningEngine>) -> (StripLayout, String) {
    match engine {
        Some(running) => (
            running.strip_layout(),
            format!("{:016x}", running.remote_layout_generation),
        ),
        None => (StripLayout::new(1, false, 0), "stopped".into()),
    }
}

struct DesktopBoard(AppHandle);

impl Board for DesktopBoard {
    fn view(&self, role: Role) -> Result<MixerView, Status> {
        let engine = self.0.state::<EngineState>();
        let running = engine.lock().map_err(|_| Status::SERVICE_UNAVAILABLE)?;
        let (layout, signature) = layout(running.as_ref());
        let state =
            self.0
                .state::<MixerControl>()
                .snapshot_layout(layout, running.is_some(), Vec::new());

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
            controls.extend(state.buses.into_iter().map(|bus| {
                let name = match bus.id.as_str() {
                    "main" => "Main",
                    "stream" => "Stream",
                    "monitor" => "Mon 1",
                    "monitor2" => "Mon 2",
                    _ => "Mon 3",
                };
                ViewControl {
                    target: Target::Bus { id: bus.id },
                    name: name.into(),
                    db: bus.fader_db,
                    muted: Some(bus.mute),
                }
            }));
        }
        Ok(MixerView {
            role,
            running: running.is_some(),
            layout: signature,
            controls,
        })
    }

    fn apply(&self, role: Role, control: Control) -> Result<(), Status> {
        let mixer = self.0.state::<MixerControl>();
        let engine = self.0.state::<EngineState>();
        let guard = engine.lock().map_err(|_| Status::SERVICE_UNAVAILABLE)?;
        if guard.is_none() {
            return Err(Status::SERVICE_UNAVAILABLE);
        }
        let (layout, signature) = layout(guard.as_ref());
        if control.layout != signature {
            return Err(Status::CONFLICT);
        }
        write_control(&mixer, layout, role, control)?;
        drop(guard);
        let _ = self.0.emit(
            "mixer-state-changed",
            mixer.snapshot_layout(layout, true, Vec::new()),
        );
        Ok(())
    }
}

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
        _ => return Err(Status::BAD_REQUEST),
    };
    result.map_err(|_| Status::BAD_REQUEST)
}

#[tauri::command]
pub async fn remote_mixer_start(
    app: AppHandle,
    remote: State<'_, RemoteMixer>,
    settings: ListenerSettings,
) -> Result<String, String> {
    if remote.0.lock().is_some() {
        return Err("Remote mixer already on".into());
    }
    let prepared = tauri::async_runtime::spawn_blocking(move || settings.prepare())
        .await
        .map_err(|_| "Cannot start remote mixer")??;
    let listener = tokio::net::TcpListener::from_std(prepared.listener)
        .map_err(|_| "Cannot start remote mixer listener")?;
    let mut state = remote.0.lock();
    if state.is_some() {
        return Err("Remote mixer already on".into());
    }
    let url = format!("https://{}", prepared.host);
    let auth = Arc::new(Mutex::new(Auth::default()));
    let router = router(Arc::new(DesktopBoard(app)), auth.clone(), &prepared.host);
    let task = tauri::async_runtime::spawn(tls::serve(listener, prepared.tls, router));
    *state = Some(Listening {
        url: url.clone(),
        auth,
        task,
    });
    Ok(url)
}

#[tauri::command]
pub fn remote_mixer_stop(remote: State<'_, RemoteMixer>) {
    let listener = remote.0.lock().take();
    drop(listener);
}

#[tauri::command]
pub fn remote_mixer_status(remote: State<'_, RemoteMixer>) -> SettingsView {
    let mut state = remote.0.lock();
    if state
        .as_ref()
        .is_some_and(|server| server.task.inner().is_finished())
    {
        *state = None;
    }
    match state.as_ref() {
        Some(server) => SettingsView {
            url: Some(server.url.clone()),
            devices: server.auth.lock().devices(),
        },
        None => SettingsView {
            url: None,
            devices: Vec::new(),
        },
    }
}

fn with_auth<T>(remote: &RemoteMixer, change: impl FnOnce(&mut Auth) -> T) -> Result<T, String> {
    let state = remote.0.lock();
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
}
