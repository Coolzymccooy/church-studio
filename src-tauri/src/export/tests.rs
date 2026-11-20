//! End-to-end export tests: recorder-style float WAVs in, real MP3 and (on
//! macOS) AAC files out.
use super::*;
use crate::recorder::wav::TrackWriter;
use std::cell::RefCell;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tiwaton-export-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `seconds` of a 440 Hz tone at -6 dBFS, written like the recorder does
/// (optionally split into parts of `part_bytes`).
fn tone_track(dir: &Path, base: &str, channels: usize, seconds: f64, part_bytes: Option<u64>) -> Vec<PathBuf> {
    let rate = 48_000u32;
    let mut track = match part_bytes {
        Some(limit) => TrackWriter::create_with_limit(dir, base, channels, rate, limit).unwrap(),
        None => TrackWriter::create(dir, base, channels, rate).unwrap(),
    };
    let frames = (seconds * rate as f64) as usize;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let v = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin();
            std::iter::repeat_n(v, channels)
        })
        .collect();
    track.write_samples(&samples).unwrap();
    track.finish().unwrap().into_iter().map(|name| dir.join(name)).collect()
}

fn run(parts: &[PathBuf], format: ExportFormat, title: Option<&str>) -> (Result<ExportOutcome, String>, Vec<f64>) {
    let dest = output_path(&parts[0], format);
    let seen = RefCell::new(Vec::new());
    let mut on_progress = |f: f64| seen.borrow_mut().push(f);
    let readers = open_parts(parts).unwrap();
    let job = ExportJob {
        format,
        dest: &dest,
        bitrate_kbps: DEFAULT_BITRATE_KBPS,
        title: title.map(str::to_string),
    };
    let result = encode(readers, &job, &mut on_progress);
    (result, seen.into_inner())
}

#[test]
fn format_names_and_paths() {
    assert_eq!(ExportFormat::parse("MP3").unwrap(), ExportFormat::Mp3);
    assert_eq!(ExportFormat::parse(" m4a ").unwrap(), ExportFormat::M4a);
    assert_eq!(ExportFormat::parse("aac").unwrap(), ExportFormat::M4a);
    assert!(ExportFormat::parse("flac").is_err());
    let wav = Path::new("/rec/2026-11-20 Sunday/Stream Mix.wav");
    assert_eq!(output_path(wav, ExportFormat::Mp3), Path::new("/rec/2026-11-20 Sunday/Stream Mix.mp3"));
    assert_eq!(output_path(wav, ExportFormat::M4a), Path::new("/rec/2026-11-20 Sunday/Stream Mix.m4a"));
    assert_eq!(
        partial_path(Path::new("/rec/Stream Mix.mp3")),
        Path::new("/rec/Stream Mix.mp3.part")
    );
}

#[test]
fn progress_steps_are_coarse_and_end_at_one() {
    let mut seen = Vec::new();
    let mut push = |f: f64| seen.push(f);
    let mut gate = ProgressGate::new(1000, &mut push);
    for _ in 0..1000 {
        gate.advance(1);
    }
    gate.complete();
    gate.complete();
    assert!(seen.len() <= 101, "{}", seen.len());
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "increasing");
    assert_eq!(*seen.last().unwrap(), 1.0);
}

#[test]
fn mp3_export_of_a_stereo_mix() {
    let dir = temp_dir("mp3-stereo");
    let parts = tone_track(&dir, "Stream Mix", 2, 3.0, None);
    let (result, progress) = run(&parts, ExportFormat::Mp3, None);
    let outcome = result.unwrap();
    assert_eq!(outcome.path, dir.join("Stream Mix.mp3"));
    assert!((outcome.duration_seconds - 3.0).abs() < 1e-6);
    // 192 kbps CBR for 3 s is 72 kB, give or take a frame of padding.
    assert!((70_000..80_000).contains(&outcome.bytes), "{}", outcome.bytes);
    let bytes = std::fs::read(&outcome.path).unwrap();
    // MPEG-1 Layer III, no CRC; bitrate index 1011 (192 kbps), 48 kHz (01).
    assert_eq!(bytes[0], 0xFF);
    assert_eq!(bytes[1], 0xFB);
    assert_eq!(bytes[2] & 0xFC, 0xB4, "header byte {:#x}", bytes[2]);
    assert!(!partial_path(&outcome.path).exists());
    assert_eq!(*progress.last().unwrap(), 1.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mp3_export_joins_parts_and_keeps_mono() {
    let dir = temp_dir("mp3-parts");
    // ~0.5 s of mono per part.
    let parts = tone_track(&dir, "01 Pastor", 1, 2.0, Some(96_000));
    assert!(parts.len() >= 4, "{parts:?}");
    let (result, _) = run(&parts, ExportFormat::Mp3, Some("Sunday"));
    let outcome = result.unwrap();
    assert_eq!(outcome.path, dir.join("01 Pastor.mp3"));
    assert!((outcome.duration_seconds - 2.0).abs() < 1e-6, "{}", outcome.duration_seconds);
    let bytes = std::fs::read(&outcome.path).unwrap();
    assert_eq!(&bytes[0..3], b"ID3", "the title tag comes first");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_export_leaves_no_file() {
    let dir = temp_dir("mp3-fail");
    let parts = tone_track(&dir, "Stream Mix", 2, 0.5, None);
    let dest = dir.join("Stream Mix.mp3");
    let job = ExportJob { format: ExportFormat::Mp3, dest: &dest, bitrate_kbps: 100, title: None };
    let err = encode(open_parts(&parts).unwrap(), &job, &mut |_| {}).unwrap_err();
    assert!(err.contains("bitrate"), "{err}");
    assert!(!dest.exists());
    assert!(!partial_path(&dest).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_track_is_refused() {
    let dir = temp_dir("empty");
    let parts = tone_track(&dir, "Stream Mix", 2, 0.0, None);
    let (result, _) = run(&parts, ExportFormat::Mp3, None);
    assert!(result.unwrap_err().contains("no audio"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parts_must_share_a_format() {
    let dir = temp_dir("mixed");
    let a = tone_track(&dir, "a", 1, 0.1, None);
    let b = tone_track(&dir, "b", 2, 0.1, None);
    let err = open_parts(&[a[0].clone(), b[0].clone()]).err().unwrap();
    assert!(err.contains("share one format"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "macos")]
#[test]
fn m4a_export_writes_an_mpeg4_aac_file_the_system_can_read() {
    let dir = temp_dir("m4a");
    let parts = tone_track(&dir, "Stream Mix", 2, 3.0, None);
    let (result, progress) = run(&parts, ExportFormat::M4a, None);
    let outcome = result.unwrap();
    assert_eq!(outcome.path, dir.join("Stream Mix.m4a"));
    let bytes = std::fs::read(&outcome.path).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
    assert_eq!(*progress.last().unwrap(), 1.0);
    // afinfo is part of macOS: it must see AAC, stereo, about 3 s.
    let info = std::process::Command::new("afinfo").arg(&outcome.path).output().unwrap();
    let text = String::from_utf8_lossy(&info.stdout);
    assert!(text.contains("aac"), "{text}");
    assert!(text.contains("2 ch"), "{text}");
    let duration: f64 = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("estimated duration:"))
        .and_then(|v| v.trim().split(' ').next())
        .and_then(|v| v.parse().ok())
        .unwrap();
    assert!((duration - 3.0).abs() < 0.05, "{duration}");
    let _ = std::fs::remove_dir_all(&dir);
}
