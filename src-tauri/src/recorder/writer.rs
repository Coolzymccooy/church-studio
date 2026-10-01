//! The writer thread (design decisions 3–5). It owns the record ring's
//! consumer for the length of one recording: it pops whole frames,
//! de-interleaves each track out of them and writes it through its own
//! `TrackWriter`. Headers are patched every `patch_interval`; progress is
//! reported every `status_interval`.
//!
//! `prepare` creates the folder and every file (no engine lock needed);
//! `launch` clears stale ring data and spawns the thread; `start` does
//! both. All of it happens before the caller sets the recording flag, so a
//! file error is reported before any audio is captured.
//! Stopping: the caller clears the flag, then sets `stop`; the thread
//! drains what is left, finalizes every file and writes `session.json`.
//! A disk error clears the flag itself, keeps what was written and ends the
//! recording with the error.
use super::frames::{extract_channels, frames_to_seconds, RecordLayout};
use super::names::part_file_name;
use super::session::{session_to_json, SessionFile, SessionTrack, TrackKind, SESSION_FILE};
use super::session::{SESSION_FORMAT, SESSION_FORMAT_VERSION};
use super::wav::TrackWriter;
use super::{RecordShared, SessionControl};
use ringbuf::traits::Consumer;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Frames popped per pass.
const SCRATCH_FRAMES: usize = 4096;
/// Sleep when the ring is empty.
const IDLE_SLEEP: Duration = Duration::from_millis(10);

/// One track to write: `channels` adjacent ring channels from
/// `first_channel`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPlan {
    /// File name without `.wav` (already sanitised).
    pub base: String,
    pub name: String,
    pub kind: TrackKind,
    pub source: String,
    pub strip: Option<u32>,
    pub first_channel: usize,
    pub channels: usize,
}

/// Everything `session.json` needs besides what the thread measures.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionStamp {
    pub app_version: String,
    pub title: Option<String>,
    pub started_local: String,
    pub started_utc: String,
}

pub struct WriterPlan {
    pub dir: PathBuf,
    pub sample_rate: u32,
    pub layout: RecordLayout,
    pub tracks: Vec<TrackPlan>,
    pub stamp: SessionStamp,
    pub patch_interval: Duration,
    pub status_interval: Duration,
}

/// Sent to `on_status`: progress every `status_interval` while recording,
/// then `Finished` once, however the recording ended.
#[derive(Debug, Clone, PartialEq)]
pub enum WriterEvent {
    Progress {
        elapsed_seconds: f64,
        dropped_frames: u64,
        markers: usize,
    },
    Finished(WriterOutcome),
}

/// How a recording ended.
#[derive(Debug, Clone, PartialEq)]
pub struct WriterOutcome {
    pub folder: PathBuf,
    pub tracks: Vec<SessionTrack>,
    pub duration_seconds: f64,
    pub dropped_frames: u64,
    pub markers: usize,
    pub error: Option<String>,
}

/// The consumer comes back with the outcome, ready for the next recording.
pub struct WriterExit<C> {
    pub consumer: C,
    pub outcome: WriterOutcome,
}

pub type StatusFn = Box<dyn FnMut(WriterEvent) + Send>;

struct OpenTrack {
    plan: TrackPlan,
    writer: TrackWriter,
    scratch: Vec<f32>,
}

/// Why `start` failed. `consumer` is handed back unless the thread itself
/// could not be spawned (it moved into the closure that failed to start).
pub struct StartError<C> {
    pub consumer: Option<C>,
    pub message: String,
}

/// The opened files of a recording, made by `prepare` (file I/O, run
/// without the engine lock) and handed to `launch`.
pub struct PreparedTracks(Vec<OpenTrack>);

/// Create the recording folder (it must not exist yet, so two starts can
/// never share or truncate one folder) and every track file.
pub fn prepare(plan: &WriterPlan) -> Result<PreparedTracks, String> {
    open_tracks(plan).map(PreparedTracks)
}

/// Remove what `prepare` created for a recording that never started:
/// the first part of every track and the (then empty) folder. Only call
/// it after `prepare` succeeded, so the folder is this recording's own.
pub fn abandon(plan: &WriterPlan, prepared: PreparedTracks) {
    drop(prepared);
    for track in plan.tracks.iter() {
        let path = plan.dir.join(part_file_name(&track.base, 1));
        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!("recorder: cannot remove unused {}: {e}", path.display());
        }
    }
    if let Err(e) = std::fs::remove_dir(&plan.dir) {
        log::warn!("recorder: cannot remove unused {}: {e}", plan.dir.display());
    }
}

/// Create the files, clear stale ring data and spawn the thread (tests).
#[cfg(test)]
pub fn start<C>(
    consumer: C,
    shared: Arc<RecordShared>,
    control: Arc<SessionControl>,
    plan: WriterPlan,
    on_status: StatusFn,
) -> Result<JoinHandle<WriterExit<C>>, StartError<C>>
where
    C: Consumer<Item = f32> + Send + 'static,
{
    match prepare(&plan) {
        Ok(prepared) => launch(consumer, shared, control, plan, prepared, on_status),
        Err(message) => Err(StartError {
            consumer: Some(consumer),
            message,
        }),
    }
}

/// Clear stale ring data and spawn the writer over already-created files.
/// Runs on the control thread before the caller sets the recording flag.
pub fn launch<C>(
    mut consumer: C,
    shared: Arc<RecordShared>,
    control: Arc<SessionControl>,
    plan: WriterPlan,
    prepared: PreparedTracks,
    on_status: StatusFn,
) -> Result<JoinHandle<WriterExit<C>>, StartError<C>>
where
    C: Consumer<Item = f32> + Send + 'static,
{
    let stale = consumer.occupied_len();
    let _ = consumer.skip(stale);
    let tracks = prepared.0;
    std::thread::Builder::new()
        .name("recorder-writer".to_string())
        .spawn(move || run(consumer, shared, control, plan, tracks, on_status))
        .map_err(|e| StartError {
            consumer: None,
            message: format!("cannot start the recorder writer: {e}"),
        })
}

fn open_tracks(plan: &WriterPlan) -> Result<Vec<OpenTrack>, String> {
    if let Some(parent) = plan.dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::create_dir(&plan.dir)
        .map_err(|e| format!("cannot create {}: {e}", plan.dir.display()))?;
    let mut tracks = Vec::with_capacity(plan.tracks.len());
    for track in plan.tracks.iter() {
        let writer = TrackWriter::create(&plan.dir, &track.base, track.channels, plan.sample_rate)
            .map_err(|e| format!("cannot create {}.wav: {e}", track.base))?;
        tracks.push(OpenTrack {
            plan: track.clone(),
            writer,
            scratch: Vec::with_capacity(SCRATCH_FRAMES * track.channels),
        });
    }
    Ok(tracks)
}

fn run<C: Consumer<Item = f32>>(
    mut consumer: C,
    shared: Arc<RecordShared>,
    control: Arc<SessionControl>,
    plan: WriterPlan,
    mut tracks: Vec<OpenTrack>,
    mut on_status: StatusFn,
) -> WriterExit<C> {
    let stride = plan.layout.channels();
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * stride];
    let mut frames_written = 0u64;
    let mut error = None;
    let mut last_patch = Instant::now();
    let mut last_status = Instant::now();
    loop {
        let stopping = control.stop.load(Ordering::Acquire);
        let popped = pop_whole_frames(&mut consumer, &mut scratch, stride);
        if popped > 0 {
            if let Err(e) = write_block(&mut tracks, &scratch[..popped * stride], stride) {
                error = Some(e);
                break;
            }
            frames_written += popped as u64;
        }
        if last_patch.elapsed() >= plan.patch_interval {
            if let Err(e) = patch_all(&mut tracks) {
                error = Some(e);
                break;
            }
            last_patch = Instant::now();
        }
        if last_status.elapsed() >= plan.status_interval {
            on_status(live_progress(&shared, &control, plan.sample_rate));
            last_status = Instant::now();
        }
        if popped == 0 {
            if stopping {
                break;
            }
            std::thread::sleep(IDLE_SLEEP);
        }
    }
    // Stop capturing (a disk error ends the recording; a normal stop has
    // already cleared the flag).
    shared.active.store(false, Ordering::Release);
    let outcome = finalize(&plan, &shared, &control, tracks, frames_written, error);
    on_status(WriterEvent::Finished(outcome.clone()));
    WriterExit { consumer, outcome }
}

fn live_progress(shared: &RecordShared, control: &SessionControl, sample_rate: u32) -> WriterEvent {
    WriterEvent::Progress {
        elapsed_seconds: frames_to_seconds(shared.frames_captured.load(Ordering::Relaxed), sample_rate),
        dropped_frames: shared.dropped_frames.load(Ordering::Relaxed),
        markers: control.marker_count(),
    }
}

/// Pop as many whole frames as are available and fit; returns the frames.
fn pop_whole_frames<C: Consumer<Item = f32>>(consumer: &mut C, scratch: &mut [f32], stride: usize) -> usize {
    let stride = stride.max(1);
    let available = consumer.occupied_len();
    let room = scratch.len() - scratch.len() % stride;
    let take = (available - available % stride).min(room);
    if take == 0 {
        return 0;
    }
    consumer.pop_slice(&mut scratch[..take]) / stride
}

fn write_block(tracks: &mut [OpenTrack], block: &[f32], stride: usize) -> Result<(), String> {
    for track in tracks.iter_mut() {
        let OpenTrack { plan, writer, scratch } = track;
        extract_channels(block, stride, plan.first_channel, plan.channels, scratch);
        writer
            .write_samples(&scratch[..])
            .map_err(|e| format!("cannot write {}: {e}", plan.base))?;
    }
    Ok(())
}

fn patch_all(tracks: &mut [OpenTrack]) -> Result<(), String> {
    for track in tracks.iter_mut() {
        track
            .writer
            .patch_header()
            .map_err(|e| format!("cannot update {}: {e}", track.plan.base))?;
    }
    Ok(())
}

/// Finish every file (best effort after an error), then write
/// `session.json`. The first error is the one reported.
fn finalize(
    plan: &WriterPlan,
    shared: &RecordShared,
    control: &SessionControl,
    tracks: Vec<OpenTrack>,
    frames_written: u64,
    mut error: Option<String>,
) -> WriterOutcome {
    let mut session_tracks = Vec::with_capacity(tracks.len());
    for track in tracks {
        let channels = track.writer.channels() as u16;
        let fallback = track.writer.files().to_vec();
        let files = match track.writer.finish() {
            Ok(files) => files,
            Err(e) => {
                error.get_or_insert(format!("cannot finish {}: {e}", track.plan.base));
                fallback
            }
        };
        session_tracks.push(session_track(track.plan, channels, files));
    }
    let markers = control.markers_snapshot();
    let session = SessionFile {
        format: SESSION_FORMAT.to_string(),
        format_version: SESSION_FORMAT_VERSION,
        app_version: plan.stamp.app_version.clone(),
        title: plan.stamp.title.clone(),
        sample_rate: plan.sample_rate,
        started_local: plan.stamp.started_local.clone(),
        started_utc: plan.stamp.started_utc.clone(),
        duration_seconds: frames_to_seconds(frames_written, plan.sample_rate),
        dropped_frames: shared.dropped_frames.load(Ordering::Relaxed),
        tracks: session_tracks,
        markers,
        error: error.clone(),
    };
    if let Err(e) = write_session(&plan.dir, &session) {
        error.get_or_insert(e);
    }
    WriterOutcome {
        folder: plan.dir.clone(),
        markers: session.markers.len(),
        duration_seconds: session.duration_seconds,
        dropped_frames: session.dropped_frames,
        tracks: session.tracks,
        error,
    }
}

fn session_track(plan: TrackPlan, channels: u16, files: Vec<String>) -> SessionTrack {
    SessionTrack {
        file: files.first().cloned().unwrap_or_else(|| format!("{}.wav", plan.base)),
        files,
        name: plan.name,
        kind: plan.kind,
        channels,
        source: plan.source,
        strip: plan.strip,
    }
}

fn write_session(dir: &Path, session: &SessionFile) -> Result<(), String> {
    crate::ndi::persist::write_atomic(&dir.join(SESSION_FILE), &session_to_json(session)?)
}
