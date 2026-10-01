//! The control side of recording, held by `RunningEngine` (design decision
//! 5). It owns the ring consumer between recordings and the writer thread
//! during one. `RunningEngine` stops it before the audio streams, so a
//! recording is always finalized before its engine goes away.
use super::frames::{frames_to_seconds, RecordLayout};
use super::session::Marker;
use super::writer::{self, StatusFn, WriterExit, WriterOutcome, WriterPlan};
use super::{RecordPort, RecordShared, SessionControl};
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

struct ActiveRecording {
    control: Arc<SessionControl>,
    handle: JoinHandle<WriterExit<HeapCons<f32>>>,
    info: RecordingInfo,
}

pub struct EngineRecorder {
    shared: Arc<RecordShared>,
    layout: RecordLayout,
    sample_rate: u32,
    /// The ring consumer while idle; `None` while recording (the writer
    /// has it) or after a writer was lost.
    idle: Option<HeapCons<f32>>,
    active: Option<ActiveRecording>,
}

impl EngineRecorder {
    pub fn new(port: RecordPort) -> Self {
        EngineRecorder {
            shared: port.shared,
            layout: port.layout,
            sample_rate: port.sample_rate,
            idle: Some(port.consumer),
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

    pub fn dropped_frames(&self) -> u64 {
        self.shared.dropped_frames.load(Ordering::Relaxed)
    }

    pub fn marker_count(&self) -> usize {
        self.active.as_ref().map_or(0, |a| a.control.marker_count())
    }

    /// Open the files and start capturing. The writer clears stale ring
    /// data before the flag is set.
    pub fn start(&mut self, plan: WriterPlan, on_status: StatusFn) -> Result<(), String> {
        if self.is_recording() {
            return Err("A recording is already running".to_string());
        }
        let consumer = self.idle.take().ok_or_else(|| {
            "The recorder is unavailable until the engine restarts".to_string()
        })?;
        let info = RecordingInfo {
            folder: plan.dir.clone(),
            track_names: plan.tracks.iter().map(|t| t.name.clone()).collect(),
        };
        let control = Arc::new(SessionControl::new());
        match writer::start(consumer, self.shared.clone(), control.clone(), plan, on_status) {
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
                self.idle = err.consumer;
                Err(err.message)
            }
        }
    }

    /// Stop capturing, let the writer drain and finalize, and take the
    /// consumer back. `None` when nothing was recording.
    pub fn stop(&mut self) -> Option<WriterOutcome> {
        let active = self.active.take()?;
        self.shared.active.store(false, Ordering::Release);
        std::thread::sleep(STOP_GRACE);
        active.control.stop.store(true, Ordering::Release);
        self.collect(active.handle)
    }

    /// Add a marker at the current recording time.
    pub fn add_marker(&mut self, label: &str, source: &str) -> Result<Marker, String> {
        if !self.is_recording() {
            return Err("Not recording".to_string());
        }
        let marker = Marker::new(self.elapsed_seconds(), label, source);
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
        self.collect(active.handle)
    }

    fn collect(&mut self, handle: JoinHandle<WriterExit<HeapCons<f32>>>) -> Option<WriterOutcome> {
        match handle.join() {
            Ok(exit) => {
                self.idle = Some(exit.consumer);
                Some(exit.outcome)
            }
            Err(_) => {
                log::error!("recorder: the writer thread panicked; recording unavailable until the engine restarts");
                None
            }
        }
    }
}

impl Drop for EngineRecorder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
