//! One OBS connection's lifetime: connect with timeout, reconnect with
//! backoff, read the initial state, then follow OBS events until it goes away
//! or the config changes (the manager's generation moves on).
use super::config::ObsConfig;
use super::manager::ObsManager;
use super::status;
use futures_util::StreamExt;
use obws::events::Event;
use obws::Client;
use std::sync::Arc;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(1);

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

pub(super) async fn run_loop(mgr: ObsManager, gen: u64) {
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
            mark_failed(mgr, gen, status::friendly_error(&e));
            return;
        }
    };
    let mut events = Box::pin(events);
    if let Err(e) = refresh_all(mgr, gen, &client).await {
        mark_failed(mgr, gen, status::friendly_error(&e));
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
                if !handle_event(mgr, gen, event).await {
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

/// The session failed during setup: show the error, not "connecting". The
/// run loop sets `connecting` again when it retries after the backoff.
fn mark_failed(mgr: &ObsManager, gen: u64, message: String) {
    mgr.set_status(gen, move |s| {
        s.reset_connection();
        s.connecting = false;
        s.last_error = Some(message);
    });
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
async fn handle_event(mgr: &ObsManager, gen: u64, event: Event) -> bool {
    match event {
        Event::CurrentProgramSceneChanged { id } => {
            let name = id.name;
            let for_status = name.clone();
            mgr.set_status(gen, move |s| s.current_scene = Some(for_status));
            if mgr.is_current(gen) {
                // Scene loads read files and touch the mixer, so run them on
                // the blocking pool; awaiting here keeps scene changes in order.
                let follower = mgr.clone();
                let joined =
                    tauri::async_runtime::spawn_blocking(move || follower.follow_obs_scene(&name))
                        .await;
                if let Err(e) = joined {
                    log::warn!("obs: scene link task failed: {e}");
                }
            }
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
