//! The device watcher (`docs/specs/2026-11-20-usb-interfaces.md` §2, §3).
//!
//! One thread for the life of the app. Every second it lists the devices;
//! when a list changes it emits `audio-devices-changed`. While an engine
//! runs it marks a working device that is no longer listed as lost, and
//! reopens a lost or waiting one once it has been listed again for two
//! ticks (`health::Reconnector`). A link-state change emits
//! `audio-device-health`.
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::device_names::{device_entries, list_devices, match_listed, ListedDevice};
use super::health::{LinkState, Reconnector, Step, StreamHealth};
use super::EngineState;

const TICK: Duration = Duration::from_secs(1);

/// What the watcher remembers between ticks.
#[derive(Default)]
pub(super) struct Watch {
    inputs: Vec<ListedDevice>,
    outputs: Vec<ListedDevice>,
    engine_id: u64,
    reconnectors: Vec<Reconnector>,
    states: Vec<LinkState>,
}

pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    let spawned = std::thread::Builder::new()
        .name("device-watch".to_string())
        .spawn(move || {
            let host = cpal::default_host();
            let mut watch = Watch {
                inputs: device_entries(&host, true).unwrap_or_default(),
                outputs: device_entries(&host, false).unwrap_or_default(),
                ..Watch::default()
            };
            loop {
                std::thread::sleep(TICK);
                tick(&app, &host, &mut watch);
            }
        });
    if let Err(err) = spawned {
        log::error!("device watcher did not start: {err}");
    }
}

pub(super) fn tick<R: Runtime>(app: &AppHandle<R>, host: &cpal::Host, watch: &mut Watch) {
    // A failed listing keeps the previous lists: it is not an unplug.
    let inputs = device_entries(host, true).unwrap_or_else(|| watch.inputs.clone());
    let outputs = device_entries(host, false).unwrap_or_else(|| watch.outputs.clone());
    if inputs != watch.inputs || outputs != watch.outputs {
        let names = |list: &[ListedDevice]| list.iter().map(|d| d.name.clone()).collect::<Vec<_>>();
        log::info!("audio devices changed: inputs {:?}, outputs {:?}", names(&inputs), names(&outputs));
        let _ = app.emit(
            "audio-devices-changed",
            serde_json::json!({
                "inputs": list_devices(true),
                "outputs": list_devices(false),
            }),
        );
        watch.inputs = inputs;
        watch.outputs = outputs;
    }
    check_engine(app, watch);
}

/// Device ids bound by the engine's other working streams in one direction.
fn taken_ids(streams: &[(usize, &str, bool, &StreamHealth)], index: usize, input: bool) -> Vec<String> {
    streams
        .iter()
        .filter(|(i, _, is_input, health)| *i != index && *is_input == input && health.state() == LinkState::Ok)
        .filter_map(|(_, _, _, health)| health.bound())
        .collect()
}

fn check_engine<R: Runtime>(app: &AppHandle<R>, watch: &mut Watch) {
    // Take what is needed and let the engine lock go: a reopen can take a
    // while, and engine_status and the mixer commands wait on that lock.
    let (id, links, reopener) = {
        let state = app.state::<EngineState>();
        let guard = state.inner().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(engine) = guard.as_ref() else {
            watch.engine_id = 0;
            return;
        };
        (engine.id, engine.links.clone(), engine.reopener())
    };
    let streams = links.streams();
    if id != watch.engine_id {
        watch.engine_id = id;
        watch.reconnectors = vec![Reconnector::default(); streams.len()];
        watch.states = Vec::new();
    }
    for &(index, name, input, health) in &streams {
        let list = if input { &watch.inputs } else { &watch.outputs };
        let bound = health.bound();
        let taken = taken_ids(&streams, index, input);
        let target = match_listed(name, bound.as_deref(), list, &taken).cloned();
        match watch.reconnectors[index].tick(health.state(), target.is_some()) {
            Step::Wait => {}
            Step::MarkLost => {
                log::warn!("'{name}' is no longer listed: marked lost");
                health.fail(LinkState::Lost, "Disconnected");
            }
            Step::Reopen => {
                if let Some(target) = target {
                    if let Err(err) = reopener.reopen(index, target) {
                        log::warn!("'{name}' did not reopen: {err}");
                    }
                }
            }
        }
    }
    let states = links.states();
    if states != watch.states {
        let _ = app.emit("audio-device-health", links.status_json());
        watch.states = states;
    }
}
