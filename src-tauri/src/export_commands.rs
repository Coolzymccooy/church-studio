//! Tauri commands for MP3 / M4A export (issue #29). The work runs on a
//! blocking thread (`spawn_blocking`), never the audio thread, one export at
//! a time, and reports progress through the `export-progress` event.
//!
//! - `recorder_export({ track, format, bitrateKbps? })`: a track of the last
//!   recording, written next to its WAV with the same base name.
//! - `editor_export(<WAV bytes>)`, headers `x-export-format` and
//!   `x-export-dest` (`encodeURIComponent` of the save-dialog path): the
//!   editor's audio.
use crate::export::request::{check_dest, find_track, percent_decode, read_session, tag_title};
use crate::export::wav_read::WavReader;
use crate::export::{self, ExportFormat, ExportJob, ExportOutcome, DEFAULT_BITRATE_KBPS};
use crate::recorder_commands::RecorderControl;
use serde::Serialize;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, State};

pub const PROGRESS_EVENT: &str = "export-progress";

static EXPORTING: AtomicBool = AtomicBool::new(false);

/// Held for the life of one export.
#[derive(Debug)]
struct ExportSlot;

impl ExportSlot {
    fn claim() -> Result<Self, String> {
        EXPORTING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ExportSlot)
            .map_err(|_| "an export is already running".to_string())
    }
}

impl Drop for ExportSlot {
    fn drop(&mut self) {
        EXPORTING.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// The file being written (name only).
    pub file: String,
    pub format: String,
    /// 0..=1.
    pub fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub file: String,
    pub format: String,
    pub duration_seconds: f64,
    pub bytes: u64,
}

fn result_of(outcome: ExportOutcome, format: ExportFormat) -> ExportResult {
    ExportResult {
        file: file_name(&outcome.path),
        path: outcome.path.display().to_string(),
        format: format.extension().to_string(),
        duration_seconds: outcome.duration_seconds,
        bytes: outcome.bytes,
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn progress_emitter(app: AppHandle, format: ExportFormat) -> impl FnMut(&str, f64) {
    move |file, fraction| {
        let _ = app.emit(
            PROGRESS_EVENT,
            ExportProgress {
                file: file.to_string(),
                format: format.extension().to_string(),
                fraction,
            },
        );
    }
}

fn bitrate(kbps: Option<u32>) -> Result<u32, String> {
    let kbps = kbps.unwrap_or(DEFAULT_BITRATE_KBPS);
    if export::mp3::BITRATES.contains(&kbps) {
        Ok(kbps)
    } else {
        Err(format!("unsupported bitrate {kbps} kbps"))
    }
}

/// Export one track of the last recording next to its WAV.
#[tauri::command]
pub async fn recorder_export(
    app: AppHandle,
    control: State<'_, RecorderControl>,
    track: String,
    format: String,
    bitrate_kbps: Option<u32>,
) -> Result<ExportResult, String> {
    let format = ExportFormat::parse(&format)?;
    let bitrate_kbps = bitrate(bitrate_kbps)?;
    let folder = control
        .last_summary()
        .map(|summary| PathBuf::from(summary.folder))
        .ok_or("record something first")?;
    let slot = ExportSlot::claim()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _slot = slot;
        let mut on_progress = progress_emitter(app, format);
        export_recorded_track(&folder, &track, format, bitrate_kbps, &mut on_progress)
    })
    .await
    .map_err(|e| format!("the export did not complete: {e}"))?
}

/// The blocking part of `recorder_export`: one track of the recording in
/// `folder` (as listed in its `session.json`) to `<track>.<ext>` beside it.
pub fn export_recorded_track(
    folder: &Path,
    track: &str,
    format: ExportFormat,
    bitrate_kbps: u32,
    on_progress: &mut dyn FnMut(&str, f64),
) -> Result<ExportResult, String> {
    let session = read_session(folder)?;
    let recorded = find_track(folder, &session, track)?;
    let dest = export::output_path(&recorded.parts[0], format);
    let readers = export::open_parts(&recorded.parts)?;
    let job = ExportJob {
        format,
        dest: &dest,
        bitrate_kbps,
        title: Some(tag_title(recorded.title.as_deref(), &recorded.name)),
    };
    let file = file_name(&dest);
    let mut report = |fraction| on_progress(&file, fraction);
    let outcome = export::encode(readers, &job, &mut report).inspect_err(|err| {
        log::error!("export: {} failed: {err}", dest.display());
    })?;
    log::info!("export: wrote {} ({} bytes)", outcome.path.display(), outcome.bytes);
    Ok(result_of(outcome, format))
}

fn header(request: &Request<'_>, name: &str) -> Result<String, String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| format!("missing {name}"))
}

/// Encode WAV bytes from the editor to the path the save dialog chose.
#[tauri::command]
pub async fn editor_export(app: AppHandle, request: Request<'_>) -> Result<ExportResult, String> {
    let format = ExportFormat::parse(&header(&request, "x-export-format")?)?;
    let dest = check_dest(&percent_decode(&header(&request, "x-export-dest")?)?, format)?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("the editor sent no audio".to_string());
    };
    let slot = ExportSlot::claim()?;
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _slot = slot;
        let reader = WavReader::new(Cursor::new(bytes)).map_err(|e| format!("cannot read the editor audio: {e}"))?;
        let job = ExportJob {
            format,
            dest: &dest,
            bitrate_kbps: DEFAULT_BITRATE_KBPS,
            title: None,
        };
        let mut emit = progress_emitter(app, format);
        let file = file_name(&dest);
        let mut on_progress = |fraction| emit(&file, fraction);
        let outcome = export::encode(vec![reader], &job, &mut on_progress)?;
        Ok(result_of(outcome, format))
    })
    .await
    .map_err(|e| format!("the export did not complete: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_export_at_a_time() {
        let first = ExportSlot::claim().unwrap();
        assert!(ExportSlot::claim().unwrap_err().contains("already"));
        drop(first);
        assert!(ExportSlot::claim().is_ok());
    }

    #[test]
    fn bitrate_defaults_to_192() {
        assert_eq!(bitrate(None).unwrap(), 192);
        assert_eq!(bitrate(Some(320)).unwrap(), 320);
        assert!(bitrate(Some(1)).is_err());
    }

    /// Exports a real recording: `TIWATON_EXPORT_FOLDER=<recording folder>
    /// cargo test --lib export_a_real_recording -- --ignored --nocapture`.
    /// Writes `Stream Mix.mp3` and `.m4a` next to its WAV.
    #[test]
    #[ignore = "needs a real recording folder"]
    fn export_a_real_recording() {
        let folder = PathBuf::from(std::env::var("TIWATON_EXPORT_FOLDER").expect("TIWATON_EXPORT_FOLDER"));
        for format in [ExportFormat::Mp3, ExportFormat::M4a] {
            let started = std::time::Instant::now();
            let mut steps = 0;
            let mut on_progress = |_: &str, _: f64| steps += 1;
            let result =
                export_recorded_track(&folder, "Stream Mix.wav", format, DEFAULT_BITRATE_KBPS, &mut on_progress)
                    .unwrap();
            println!(
                "{}: {:.1} s of audio, {} bytes, {} progress events, encoded in {:.2} s",
                result.path,
                result.duration_seconds,
                result.bytes,
                steps,
                started.elapsed().as_secs_f64()
            );
        }
    }

    #[test]
    fn a_track_from_a_recording_folder() {
        use crate::recorder::session::{session_to_json, SessionFile, SessionTrack, TrackKind, SESSION_FILE};
        use crate::recorder::wav::TrackWriter;
        let dir = std::env::temp_dir().join(format!("tiwaton-export-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = TrackWriter::create(&dir, "Stream Mix", 2, 48_000).unwrap();
        writer.write_samples(&vec![0.1; 48_000 * 2]).unwrap();
        writer.finish().unwrap();
        let session = SessionFile {
            format: "tiwaton-multitrack".into(),
            format_version: 1,
            app_version: "test".into(),
            title: Some("Sunday".into()),
            sample_rate: 48_000,
            started_local: String::new(),
            started_utc: String::new(),
            duration_seconds: 1.0,
            dropped_frames: 0,
            tracks: vec![SessionTrack {
                file: "Stream Mix.wav".into(),
                files: vec!["Stream Mix.wav".into()],
                name: "Stream Mix".into(),
                kind: TrackKind::Bus,
                channels: 2,
                source: "stream".into(),
                strip: None,
            }],
            markers: Vec::new(),
            error: None,
        };
        std::fs::write(dir.join(SESSION_FILE), session_to_json(&session).unwrap()).unwrap();
        let mut events = Vec::new();
        let mut on_progress = |file: &str, f: f64| events.push((file.to_string(), f));
        let result = export_recorded_track(&dir, "Stream Mix.wav", ExportFormat::Mp3, 192, &mut on_progress).unwrap();
        assert_eq!(result.file, "Stream Mix.mp3");
        assert_eq!(result.format, "mp3");
        assert!(dir.join("Stream Mix.mp3").is_file());
        assert_eq!(events.last().unwrap(), &("Stream Mix.mp3".to_string(), 1.0));
        assert!(export_recorded_track(&dir, "Nope.wav", ExportFormat::Mp3, 192, &mut |_, _| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn payloads_are_camel_case() {
        let value = serde_json::to_value(ExportResult {
            path: "/r/a.mp3".into(),
            file: "a.mp3".into(),
            format: "mp3".into(),
            duration_seconds: 1.0,
            bytes: 2,
        })
        .unwrap();
        assert!(value.get("durationSeconds").is_some());
        let value = serde_json::to_value(ExportProgress { file: "a".into(), format: "mp3".into(), fraction: 0.5 }).unwrap();
        assert_eq!(value["fraction"], 0.5);
    }
}
