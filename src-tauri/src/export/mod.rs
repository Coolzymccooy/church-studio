//! Export a recording (or editor audio) to MP3 or AAC (.m4a), on a
//! background thread, never the audio thread (issue #29).
//!
//! - MP3: LAME 3.100 (`mp3`), every platform.
//! - AAC: the system encoder, AudioToolbox on macOS (`aac_macos`) and Media
//!   Foundation on Windows (`aac_windows`). Other systems get a clear error.
//!
//! The output is written to `<name>.<ext>.part` and renamed into place only
//! when complete, so a failed or interrupted export never leaves a file that
//! looks finished.
pub mod mp3;
pub mod request;
pub mod wav_read;
#[cfg(target_os = "macos")]
mod aac_macos;
#[cfg(windows)]
mod aac_windows;
#[cfg(test)]
mod tests;

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use wav_read::{WavFormat, WavReader};

pub const DEFAULT_BITRATE_KBPS: u32 = 192;
/// Frames read and encoded per step.
pub const CHUNK_FRAMES: usize = 8192;
/// Progress is reported in steps of at least this fraction.
pub const PROGRESS_STEP: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Mp3,
    M4a,
}

impl ExportFormat {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "mp3" => Ok(ExportFormat::Mp3),
            "m4a" | "aac" => Ok(ExportFormat::M4a),
            other => Err(format!("unknown export format '{other}' (use mp3 or m4a)")),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Mp3 => "mp3",
            ExportFormat::M4a => "m4a",
        }
    }
}

/// What the encoder is told about the audio it will get.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodeSpec {
    pub channels: u16,
    pub sample_rate: u32,
    pub bitrate_kbps: u32,
    /// MP3 title tag.
    pub title: Option<String>,
}

/// Where encoded audio goes. `write` takes interleaved frames.
pub trait AudioSink {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String>;
    fn finish(self: Box<Self>) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportOutcome {
    pub path: PathBuf,
    pub duration_seconds: f64,
    pub bytes: u64,
}

/// Samples clamped to ±1 (non-finite → 0) into `out`.
pub fn clamp_into(samples: &[f32], out: &mut Vec<f32>) {
    mp3::clamp_into(samples, out)
}

/// `<dir>/Stream Mix.wav` → `<dir>/Stream Mix.mp3`.
pub fn output_path(first_wav: &Path, format: ExportFormat) -> PathBuf {
    first_wav.with_extension(format.extension())
}

/// `<dest>.part`: where the encoder writes before the rename.
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

fn open_sink(format: ExportFormat, path: &Path, spec: &EncodeSpec) -> Result<Box<dyn AudioSink>, String> {
    match format {
        ExportFormat::Mp3 => Ok(Box::new(mp3::Mp3Sink::create(path, spec)?)),
        ExportFormat::M4a => open_aac(path, spec),
    }
}

#[cfg(target_os = "macos")]
fn open_aac(path: &Path, spec: &EncodeSpec) -> Result<Box<dyn AudioSink>, String> {
    Ok(Box::new(aac_macos::AacSink::create(path, spec)?))
}

#[cfg(windows)]
fn open_aac(path: &Path, spec: &EncodeSpec) -> Result<Box<dyn AudioSink>, String> {
    Ok(Box::new(aac_windows::AacSink::create(path, spec)?))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn open_aac(_path: &Path, _spec: &EncodeSpec) -> Result<Box<dyn AudioSink>, String> {
    Err("m4a export is not available on this system; export mp3 instead".to_string())
}

/// Reports progress in `PROGRESS_STEP` steps, always ending at 1.0.
pub struct ProgressGate<'a> {
    total_frames: u64,
    done_frames: u64,
    last_sent: f64,
    on_progress: &'a mut dyn FnMut(f64),
}

impl<'a> ProgressGate<'a> {
    pub fn new(total_frames: u64, on_progress: &'a mut dyn FnMut(f64)) -> Self {
        ProgressGate {
            total_frames,
            done_frames: 0,
            last_sent: 0.0,
            on_progress,
        }
    }

    pub fn advance(&mut self, frames: u64) {
        self.done_frames += frames;
        let fraction = if self.total_frames == 0 {
            0.0
        } else {
            (self.done_frames as f64 / self.total_frames as f64).min(1.0)
        };
        if fraction - self.last_sent >= PROGRESS_STEP {
            self.last_sent = fraction;
            (self.on_progress)(fraction);
        }
    }

    pub fn complete(&mut self) {
        if self.last_sent < 1.0 {
            self.last_sent = 1.0;
            (self.on_progress)(1.0);
        }
    }
}

/// Open every part of one track and check they share one format.
pub fn open_parts(paths: &[PathBuf]) -> Result<Vec<WavReader<BufReader<File>>>, String> {
    if paths.is_empty() {
        return Err("nothing to export".to_string());
    }
    let readers = paths
        .iter()
        .map(|path| {
            let file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
            WavReader::new(BufReader::with_capacity(256 * 1024, file))
                .map_err(|e| format!("cannot read {}: {e}", path.display()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    check_same_format(readers.iter().map(|r| r.format()))?;
    Ok(readers)
}

fn check_same_format(mut formats: impl Iterator<Item = WavFormat>) -> Result<WavFormat, String> {
    let first = formats.next().ok_or("nothing to export")?;
    for next in formats {
        if next.channels != first.channels || next.sample_rate != first.sample_rate {
            return Err("the parts of this track do not share one format".to_string());
        }
    }
    Ok(first)
}

/// What to encode and where.
pub struct ExportJob<'a> {
    pub format: ExportFormat,
    pub dest: &'a Path,
    pub bitrate_kbps: u32,
    pub title: Option<String>,
}

/// Encode `readers` (parts of one track, in order) into `job.dest`.
pub fn encode<R: Read + Seek>(
    mut readers: Vec<WavReader<R>>,
    job: &ExportJob<'_>,
    on_progress: &mut dyn FnMut(f64),
) -> Result<ExportOutcome, String> {
    let format = check_same_format(readers.iter().map(|r| r.format()))?;
    let total_frames: u64 = readers.iter().map(|r| r.total_frames()).sum();
    if total_frames == 0 {
        return Err("this track holds no audio".to_string());
    }
    let spec = EncodeSpec {
        channels: format.channels,
        sample_rate: format.sample_rate,
        bitrate_kbps: job.bitrate_kbps,
        title: job.title.clone(),
    };
    let partial = partial_path(job.dest);
    let result = encode_into(&mut readers, job.format, &partial, &spec, total_frames, on_progress)
        .and_then(|()| {
            std::fs::rename(&partial, job.dest)
                .map_err(|e| format!("cannot save {}: {e}", job.dest.display()))
        });
    if let Err(err) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(err);
    }
    let bytes = std::fs::metadata(job.dest).map(|m| m.len()).unwrap_or(0);
    Ok(ExportOutcome {
        path: job.dest.to_path_buf(),
        duration_seconds: total_frames as f64 / format.sample_rate as f64,
        bytes,
    })
}

fn encode_into<R: Read + Seek>(
    readers: &mut [WavReader<R>],
    format: ExportFormat,
    partial: &Path,
    spec: &EncodeSpec,
    total_frames: u64,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let mut sink = open_sink(format, partial, spec)?;
    let mut gate = ProgressGate::new(total_frames, on_progress);
    let mut buf = Vec::with_capacity(CHUNK_FRAMES * spec.channels as usize);
    for reader in readers.iter_mut() {
        loop {
            let frames = reader
                .read_frames(CHUNK_FRAMES, &mut buf)
                .map_err(|e| format!("cannot read the recording: {e}"))?;
            if frames == 0 {
                break;
            }
            sink.write(&buf)?;
            gate.advance(frames as u64);
        }
    }
    sink.finish()?;
    gate.complete();
    Ok(())
}
