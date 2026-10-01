//! Integration test of the writer thread on a temp dir: frames go in
//! through the ring exactly as the callback pushes them, the thread writes
//! and finalizes, and the files are read back (sizes, samples,
//! session.json). Also `EngineRecorder` start/stop/marker round trip.
use super::engine::EngineRecorder;
use super::frames::{interleave, push_whole_frames, RecordLayout};
use super::session::TrackKind;
use super::wav::HEADER_LEN;
use super::writer::{self, SessionStamp, TrackPlan, WriterEvent, WriterPlan};
use super::{record_ring, RecordShared, SessionControl};
use ringbuf::traits::Split;
use ringbuf::HeapRb;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tiwaton-rec-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn samples(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(u32_at(&bytes, 54) as usize, bytes.len() - HEADER_LEN, "data size patched");
    assert_eq!(u32_at(&bytes, 4) as usize, bytes.len() - 8, "RIFF size patched");
    bytes[HEADER_LEN..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn track(base: &str, kind: TrackKind, source: &str, first: usize, channels: usize) -> TrackPlan {
    TrackPlan {
        base: base.to_string(),
        name: base.to_string(),
        kind,
        source: source.to_string(),
        strip: if kind == TrackKind::Strip { Some(first as u32) } else { None },
        first_channel: first,
        channels,
    }
}

fn plan(dir: &Path, layout: RecordLayout, tracks: Vec<TrackPlan>) -> WriterPlan {
    WriterPlan {
        dir: dir.to_path_buf(),
        sample_rate: 48_000,
        layout,
        tracks,
        stamp: SessionStamp {
            app_version: "test".to_string(),
            title: Some("Test".to_string()),
            started_local: "2026-10-01T10:30:00+01:00".to_string(),
            started_utc: "2026-10-01T09:30:00Z".to_string(),
        },
        patch_interval: Duration::from_millis(5),
        status_interval: Duration::from_millis(5),
    }
}

/// `frames` frames of 2 strips + Stream + Main with recognisable values.
fn block(start: usize, frames: usize) -> Vec<f32> {
    let s0: Vec<f32> = (start..start + frames).map(|i| i as f32).collect();
    let s1: Vec<f32> = s0.iter().map(|v| v + 1000.0).collect();
    let sl: Vec<f32> = s0.iter().map(|v| -v).collect();
    let sr: Vec<f32> = s0.iter().map(|v| v * 0.5).collect();
    let ml: Vec<f32> = s0.iter().map(|v| v + 0.25).collect();
    let mr: Vec<f32> = s0.iter().map(|v| v - 0.25).collect();
    let mut out = vec![0.0f32; frames * 6];
    interleave(&[&s0[..], &s1[..], &sl[..], &sr[..], &ml[..], &mr[..]], frames, &mut out);
    out
}

#[test]
fn writer_thread_writes_finalizes_and_reads_back() {
    let dir = temp_dir("writer");
    let layout = RecordLayout { strips: 2 };
    let (mut producer, consumer) = HeapRb::<f32>::new(48_000 * 6).split();
    // Stale data from an earlier run must not reach the files.
    assert_eq!(push_whole_frames(&mut producer, &[9.0; 6], 6, 1), 0);

    let shared = Arc::new(RecordShared::new());
    let control = Arc::new(SessionControl::new());
    let events: Arc<Mutex<Vec<WriterEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let tracks = vec![
        track("02 Keys", TrackKind::Strip, "hardware", 1, 1),
        track("Stream Mix", TrackKind::Bus, "stream", layout.stream_channel(), 2),
    ];
    let handle = writer::start(
        consumer,
        shared.clone(),
        control.clone(),
        plan(&dir, layout, tracks),
        Box::new(move |event: WriterEvent| sink.lock().unwrap().push(event)),
    )
    .map_err(|e| e.message)
    .unwrap();

    shared.active.store(true, Ordering::Release);
    for chunk in 0..10 {
        let data = block(chunk * 100, 100);
        assert_eq!(push_whole_frames(&mut producer, &data, 6, 100), 0);
        std::thread::sleep(Duration::from_millis(2));
    }
    control.add_marker(super::session::Marker::new(0.01, "Sermon", "scene"));
    shared.active.store(false, Ordering::Release);
    control.stop.store(true, Ordering::Release);
    let exit = handle.join().unwrap();

    let outcome = exit.outcome;
    assert_eq!(outcome.error, None);
    assert_eq!(outcome.markers, 1);
    assert!((outcome.duration_seconds - 1000.0 / 48_000.0).abs() < 1e-9);
    assert!(!shared.active.load(Ordering::Acquire));

    let keys = samples(&dir.join("02 Keys.wav"));
    assert_eq!(keys.len(), 1000);
    assert_eq!(keys[0], 1000.0);
    assert_eq!(keys[999], 1999.0);
    let stream = samples(&dir.join("Stream Mix.wav"));
    assert_eq!(stream.len(), 2000);
    assert_eq!(&stream[2..4], &[-1.0, 0.5]);
    assert!(!dir.join("Main Mix.wav").exists());

    let text = std::fs::read_to_string(dir.join("session.json")).unwrap();
    let session: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(session["tracks"][0]["file"], "02 Keys.wav");
    assert_eq!(session["tracks"][1]["channels"], 2);
    assert_eq!(session["markers"][0]["label"], "Sermon");

    let events = events.lock().unwrap();
    assert!(matches!(events.last(), Some(WriterEvent::Finished(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_cannot_be_created_hands_the_consumer_back() {
    let dir = temp_dir("blocked");
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    // A file where the recording folder should be.
    std::fs::write(&dir, b"not a folder").unwrap();
    let (_producer, consumer) = HeapRb::<f32>::new(64).split();
    let layout = RecordLayout { strips: 1 };
    let result = writer::start(
        consumer,
        Arc::new(RecordShared::new()),
        Arc::new(SessionControl::new()),
        plan(&dir, layout, vec![track("01 A", TrackKind::Strip, "hardware", 0, 1)]),
        Box::new(|_: WriterEvent| {}),
    );
    match result {
        Ok(_) => panic!("expected an error"),
        Err(err) => {
            assert!(err.consumer.is_some());
            assert!(!err.message.is_empty());
        }
    }
    let _ = std::fs::remove_file(&dir);
}

#[test]
fn engine_recorder_starts_marks_and_stops() {
    let dir = temp_dir("engine");
    let (mut tap, port) = record_ring(1, 48_000, 64);
    let mut recorder = EngineRecorder::new(port);
    assert!(recorder.add_marker("x", "operator").is_err(), "not recording");

    let layout = recorder.layout();
    let tracks = vec![track("01 Pastor", TrackKind::Strip, "hardware", 0, 1)];
    recorder.start(plan(&dir, layout, tracks.clone()), Box::new(|_: WriterEvent| {})).unwrap();
    assert!(recorder.is_recording());
    assert!(recorder.start(plan(&dir, layout, tracks), Box::new(|_: WriterEvent| {})).is_err());

    let strips = vec![vec![0.5f32; 64]];
    let bus = [0.0f32; 64];
    tap.capture_raw(&strips, 0, 48);
    tap.push(&strips, (&bus, &bus), (&bus, &bus), 48);
    let marker = recorder.add_marker("Sermon", "operator").unwrap();
    assert!((marker.time_seconds - 0.001).abs() < 1e-9, "48 frames at 48 kHz");
    assert_eq!(recorder.marker_count(), 1);

    let outcome = recorder.stop().unwrap();
    assert_eq!(outcome.error, None);
    assert!(!recorder.is_recording());
    assert_eq!(samples(&dir.join("01 Pastor.wav")), vec![0.5f32; 48]);
    assert!(recorder.stop().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
