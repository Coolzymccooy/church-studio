//! The control side of recording, held by `RunningEngine` (design decision
//! 5). It owns the ring consumer between recordings and the writer thread
//! during one. `RunningEngine` stops it before the audio streams, so a
//! recording is always finalized before its engine goes away.
use super::frames::{frames_to_seconds, recorded_frames, RecordLayout};
use super::session::Marker;
use super::writer::{self, PreparedTracks, StatusFn, WriterExit, WriterOutcome, WriterPlan};
use super::{RecordPort, RecordShared, SessionControl};
use parking_lot::Mutex;
use ringbuf::HeapCons;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// After clearing the recording flag, wait this long before telling the
/// writer to drain, so a callback that latched the flag just before can
/// finish its push (callbacks are a few ms; this is many of them).
const STOP_GRACE: Duration = Duration::from_millis(100);

/// What the UI shows about the recording in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingInfo {
    pub folder: PathBuf,
    pub track_names: Vec<String>,
}

type WriterHandle = JoinHandle<WriterExit<HeapCons<f32>>>;
/// Where the ring consumer waits between recordings. Shared with a
/// `PendingStop` so the consumer comes back without the engine lock.
type IdleSlot = Arc<Mutex<Option<HeapCons<f32>>>>;

struct ActiveRecording {
    control: Arc<SessionControl>,
    handle: WriterHandle,
    info: RecordingInfo,
}

/// A recording whose flag is cleared but whose writer is still draining.
/// `finish` blocks (grace period, drain, finalize, join), so callers run
/// it without holding the engine lock (`recorder_stop`).
pub struct PendingStop {
    control: Arc<SessionControl>,
    handle: WriterHandle,
    idle: IdleSlot,
}

impl PendingStop {
    /// Let in-flight callbacks finish, tell the writer to drain and
    /// finalize, join it and put the consumer back for the next recording.
    pub fn finish(self) -> Option<WriterOutcome> {
        std::thread::sleep(STOP_GRACE);
        self.control.stop.store(true, Ordering::Release);
        collect(&self.idle, self.handle)
    }
}

pub struct EngineRecorder {
    shared: Arc<RecordShared>,
    layout: RecordLayout,
    sample_rate: u32,
    /// The ring consumer while idle; `None` while recording or finishing
    /// a stop (the writer has it) or after a writer was lost.
    idle: IdleSlot,
    active: Option<ActiveRecording>,
}

impl EngineRecorder {
    pub fn new(port: RecordPort) -> Self {
        EngineRecorder {
            shared: port.shared,
            layout: port.layout,
            sample_rate: port.sample_rate,
            idle: Arc::new(Mutex::new(Some(port.consumer))),
            active: None,
        }
    }

    pub fn layout(&self) -> RecordLayout {
        self.layout
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// True while a recording runs. Collects a writer that ended on its own
    /// (disk error) first.
    pub fn is_recording(&mut self) -> bool {
        self.reap_finished();
        self.active.is_some()
    }

    pub fn info(&self) -> Option<&RecordingInfo> {
        self.active.as_ref().map(|a| &a.info)
    }

    pub fn elapsed_seconds(&self) -> f64 {
        frames_to_seconds(self.shared.frames_captured.load(Ordering::Relaxed), self.sample_rate)
    }

    /// Position in the files: frames that reached the ring (seen minus
    /// dropped), so a marker lands where its audio is in every track.
    pub fn recorded_seconds(&self) -> f64 {
        let captured = self.shared.frames_captured.load(Ordering::Relaxed);
        let dropped = self.shared.dropped_frames.load(Ordering::Relaxed);
        frames_to_seconds(recorded_frames(captured, dropped), self.sample_rate)
    }

    pub fn dropped_frames(&self) -> u64 {
        self.shared.dropped_frames.load(Ordering::Relaxed)
    }

    pub fn marker_count(&self) -> usize {
        self.active.as_ref().map_or(0, |a| a.control.marker_count())
    }

    /// True when this recorder belongs to the engine that owns `shared`
    /// (an engine restarted in between has a new ring and layout).
    pub fn owns(&self, shared: &Arc<RecordShared>) -> bool {
        Arc::ptr_eq(&self.shared, shared)
    }

    /// This engine's ring state, to check `owns` after the lock was released.
    pub fn shared(&self) -> Arc<RecordShared> {
        self.shared.clone()
    }

    /// Open the files and start capturing in one call (tests; the app's
    /// `recorder_start` uses `writer::prepare` + `start_prepared` instead).
    #[cfg(test)]
    pub fn start(&mut self, plan: WriterPlan, on_status: StatusFn) -> Result<(), String> {
        if self.is_recording() {
            return Err("A recording is already running".to_string());
        }
        let prepared = writer::prepare(&plan)?;
        self.start_prepared(plan, prepared, on_status)
    }

    /// Start capturing into files `writer::prepare` already created. On
    /// failure the files are removed again. The writer clears stale ring
    /// data before the flag is set.
    pub fn start_prepared(
        &mut self,
        plan: WriterPlan,
        prepared: PreparedTracks,
        on_status: StatusFn,
    ) -> Result<(), String> {
        if self.is_recording() {
            writer::abandon(&plan, prepared);
            return Err("A recording is already running".to_string());
        }
        let Some(consumer) = self.idle.lock().take() else {
            writer::abandon(&plan, prepared);
            return Err(
                "The recorder is not ready (the last recording is still finishing, or the engine needs a restart)"
                    .to_string(),
            );
        };
        let info = RecordingInfo {
            folder: plan.dir.clone(),
            track_names: plan.tracks.iter().map(|t| t.name.clone()).collect(),
        };
        let control = Arc::new(SessionControl::new());
        match writer::launch(consumer, self.shared.clone(), control.clone(), plan, prepared, on_status) {
            Ok(handle) => {
                self.shared.reset_counters();
                self.shared.active.store(true, Ordering::Release);
                self.active = Some(ActiveRecording {
                    control,
                    handle,
                    info,
                });
                Ok(())
            }
            Err(err) => {
                *self.idle.lock() = err.consumer;
                Err(err.message)
            }
        }
    }

    /// Stop capturing and hand back the blocking rest of the stop. Cheap:
    /// call it under the engine lock, then `finish` it after the lock is
    /// released. `None` when nothing was recording, so a second call (or
    /// an engine stop after a command stop) is a no-op.
    pub fn begin_stop(&mut self) -> Option<PendingStop> {
        let active = self.active.take()?;
        self.shared.active.store(false, Ordering::Release);
        Some(PendingStop {
            control: active.control,
            handle: active.handle,
            idle: self.idle.clone(),
        })
    }

    /// Stop, drain and finalize synchronously (engine stop and Drop, so
    /// the files are finished before the streams go away).
    pub fn stop(&mut self) -> Option<WriterOutcome> {
        self.begin_stop()?.finish()
    }

    /// Add a marker at the current recording time.
    pub fn add_marker(&mut self, label: &str, source: &str) -> Result<Marker, String> {
        if !self.is_recording() {
            return Err("Not recording".to_string());
        }
        let marker = Marker::new(self.recorded_seconds(), label, source);
        if let Some(active) = self.active.as_ref() {
            active.control.add_marker(marker.clone());
        }
        Ok(marker)
    }

    /// Join a writer that has already finished (it stopped on a disk
    /// error) and take the consumer back.
    fn reap_finished(&mut self) -> Option<WriterOutcome> {
        let finished = self.active.as_ref().map_or(false, |a| a.handle.is_finished());
        if !finished {
            return None;
        }
        let active = self.active.take()?;
        collect(&self.idle, active.handle)
    }
}

/// Join the writer and return the consumer to the idle slot.
fn collect(idle: &Mutex<Option<HeapCons<f32>>>, handle: WriterHandle) -> Option<WriterOutcome> {
    match handle.join() {
        Ok(exit) => {
            *idle.lock() = Some(exit.consumer);
            Some(exit.outcome)
        }
        Err(_) => {
            log::error!("recorder: the writer thread panicked; recording unavailable until the engine restarts");
            None
        }
    }
}

impl Drop for EngineRecorder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
