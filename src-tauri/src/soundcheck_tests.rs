use crate::recorder::session::{
    SessionFile, SessionTrack, TrackKind, SESSION_FORMAT, SESSION_FORMAT_VERSION,
};
use crate::recorder::wav::header_bytes;
use crate::soundcheck::{open_session, playback_channel, PlaybackInput};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const SR: u32 = 48_000;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "tiwaton-soundcheck-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn track(dir: &Path, name: &str, signal: &[f32]) {
    let mut file = File::create(dir.join(name)).unwrap();
    file.write_all(&header_bytes(1, SR, signal.len() as u32 * 4))
        .unwrap();
    for sample in signal {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }
}

fn session(dir: &Path) -> SessionFile {
    track(dir, "a.wav", &[0.0, 0.25]);
    track(dir, "a2.wav", &[0.5, 0.75]);
    track(dir, "b.wav", &[0.0, -0.25, -0.5]);
    SessionFile {
        format: SESSION_FORMAT.into(),
        format_version: SESSION_FORMAT_VERSION,
        app_version: "1.5.1".into(),
        title: Some("Sunday".into()),
        sample_rate: SR,
        started_local: String::new(),
        started_utc: String::new(),
        duration_seconds: 1.0,
        dropped_frames: 2,
        tracks: ["a.wav", "b.wav"]
            .into_iter()
            .enumerate()
            .map(|(i, file)| SessionTrack {
                file: file.into(),
                files: if i == 0 {
                    vec![file.into(), "a2.wav".into()]
                } else {
                    vec![file.into()]
                },
                name: file.into(),
                kind: TrackKind::Strip,
                channels: 1,
                source: "hardware".into(),
                strip: Some(i as u32),
            })
            .collect(),
        markers: Vec::new(),
        error: None,
    }
}

fn save(dir: &Path, session: &SessionFile) {
    std::fs::write(
        dir.join("session.json"),
        serde_json::to_vec(session).unwrap(),
    )
    .unwrap();
}

fn inputs(active: usize) -> Vec<PlaybackInput> {
    ["a.wav", "b.wav", "Live mic"]
        .into_iter()
        .take(active)
        .enumerate()
        .map(|(index, name)| PlaybackInput {
            index,
            name: name.into(),
            source: "hardware",
        })
        .collect()
}

fn fixture_open(dir: &Path, active: usize, rate: u32) -> Result<crate::soundcheck::Source, String> {
    open_session(&dir.join("session.json"), &inputs(active), rate)
}

#[test]
fn session_stitches_rollover_and_resamples_an_aligned_frame() {
    let dir = TempDir::new();
    save(dir.path(), &session(dir.path()));
    let mut source = fixture_open(dir.path(), 2, SR * 2).unwrap();
    assert_eq!(source.info.frames, 8);
    for (i, expected) in [
        [0.0, 0.0],
        [0.125, -0.125],
        [0.25, -0.25],
        [0.375, -0.375],
        [0.5, -0.5],
        [0.625, -0.25],
        [0.75, 0.0],
        [0.375, 0.0],
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(source.frame(i as u64).unwrap()[..2], expected);
    }
    assert_eq!(source.frame(2).unwrap()[..2], [0.25, -0.25]);
}

#[test]
fn invalid_manifest_keeps_files_inside_the_session() {
    let dir = TempDir::new();
    let mut metadata = session(dir.path());
    metadata.tracks[0].files[0] = "../a.wav".into();
    save(dir.path(), &metadata);
    assert!(fixture_open(dir.path(), 2, SR).is_err());
    metadata.tracks[0].files[0] = "a.wav".into();
    metadata.tracks[1].strip = Some(0);
    save(dir.path(), &metadata);
    assert!(fixture_open(dir.path(), 2, SR).is_err());
}

#[test]
fn seek_and_loop_keep_mapped_strips_in_step_and_leave_live_inputs_alone() {
    let dir = TempDir::new();
    save(dir.path(), &session(dir.path()));
    let (mut tap, mut control) = playback_channel(SR, 3).unwrap();
    control
        .load(fixture_open(dir.path(), 3, SR).unwrap())
        .unwrap();
    control.set_source(0, true).unwrap();
    control.set_source(1, true).unwrap();
    assert!(control.set_source(2, true).is_err());
    let mut inputs = vec![vec![0.0; 480], vec![0.0; 480], vec![0.9; 480]];
    tap.process(&mut inputs, 480);
    control.set_loop(true).unwrap();
    control.locate(1.0 / SR as f64).unwrap();
    control.play().unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        tap.process(&mut inputs, 3);
        if inputs[0][0] != 0.0 {
            break;
        }
        assert!(Instant::now() < deadline, "playback never became ready");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(inputs[0][..3], [0.125, 0.25, 0.0]);
    assert_eq!(inputs[1][..3], [-0.125, -0.25, 0.0]);
    assert_eq!(inputs[2][..3], [0.9; 3]);
    tap.process(&mut inputs, 3);
    assert_eq!(inputs[0][..3], [0.0, 0.25, 0.25]);
    control.stop().unwrap();
    tap.process(&mut inputs, 3);
    assert!(
        inputs[0][0] > 0.0 && inputs[0][0] < 0.25,
        "stop must fade from the last playing sample"
    );
    assert!(inputs[0][2] < inputs[0][0]);
    tap.process(&mut inputs, 480);
    assert_eq!(inputs[0][479], 0.0);
}

#[test]
fn remaps_by_source_and_name_and_reports_skipped_tracks() {
    let dir = TempDir::new();
    let metadata = session(dir.path());
    save(dir.path(), &metadata);
    let desk = [
        PlaybackInput {
            index: 1,
            name: "a.wav".into(),
            source: "hardware",
        },
        PlaybackInput {
            index: 0,
            name: "b.wav".into(),
            source: "ndi",
        },
    ];
    let mut playback = open_session(&dir.path().join("session.json"), &desk, SR).unwrap();
    assert_eq!(playback.info.tracks[0].index, 1);
    assert_eq!(playback.info.warnings[0].reason, "Source kind differs");
    assert_eq!(playback.frame(2).unwrap()[..2], [0.0, 0.5]);

    let partial = fixture_open(dir.path(), 1, SR).unwrap();
    assert_eq!(partial.info.warnings[0].reason, "Strip unavailable");
}

#[test]
fn absolute_and_symlinked_external_parts_are_refused() {
    let dir = TempDir::new();
    let external = TempDir::new();
    track(external.path(), "elsewhere.wav", &[0.1; 2]);
    let outside = external.path().join("elsewhere.wav");
    let mut metadata = session(dir.path());
    metadata.tracks[0].file = outside.to_string_lossy().into_owned();
    metadata.tracks[0].files.clear();
    save(dir.path(), &metadata);
    assert!(fixture_open(dir.path(), 2, SR).is_err());

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside, dir.path().join("external.wav")).unwrap();
        metadata.tracks[0].file = "external.wav".into();
        save(dir.path(), &metadata);
        assert!(fixture_open(dir.path(), 2, SR).is_err());
    }
}

#[test]
fn short_wav_parts_use_available_complete_frames() {
    let dir = TempDir::new();
    save(dir.path(), &session(dir.path()));
    let path = dir.path().join("a2.wav");
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_len((crate::recorder::wav::HEADER_LEN + 5) as u64)
        .unwrap();
    let mut source = fixture_open(dir.path(), 2, SR).unwrap();
    assert_eq!(source.info.frames, 3);
    assert_eq!(source.frame(2).unwrap()[..2], [0.5, -0.5]);
}

#[test]
fn reading_failed_audio_stops_transport() {
    let dir = TempDir::new();
    let mut metadata = session(dir.path());
    metadata.tracks[0].files = vec!["a.wav".into()];
    track(dir.path(), "a.wav", &vec![0.1; SR as usize]);
    save(dir.path(), &metadata);
    let (_tap, mut control) = playback_channel(SR, 2).unwrap();
    control
        .load(fixture_open(dir.path(), 2, SR).unwrap())
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(dir.path().join("a.wav"))
        .unwrap()
        .set_len(crate::recorder::wav::HEADER_LEN as u64)
        .unwrap();
    control.locate(0.8).unwrap();
    control.play().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = serde_json::to_value(control.status()).unwrap();
        if status["error"] == "Playback read failed" {
            assert_eq!(status["playing"], false);
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn recorder_and_soundcheck_are_exclusive_and_restart_rejects_stale_load() {
    let dir = TempDir::new();
    save(dir.path(), &session(dir.path()));
    let (_tap, mut control) = playback_channel(SR, 2).unwrap();
    let old_identity = control.identity();
    let (_next, replacement) = playback_channel(SR, 2).unwrap();
    assert!(control.ensure_owner(&old_identity).is_ok());
    assert!(replacement.ensure_owner(&old_identity).is_err());
    assert!(crate::soundcheck_commands::require_idle_recorder(true).is_err());
    assert!(crate::soundcheck_commands::require_idle_recorder(false).is_ok());

    control
        .load(fixture_open(dir.path(), 2, SR).unwrap())
        .unwrap();
    assert!(control.ensure_recording_compatible().is_ok());
    control.set_source(0, true).unwrap();
    assert!(control.ensure_recording_compatible().is_err());
    control.set_source(0, false).unwrap();
    control.play().unwrap();
    assert!(control.ensure_recording_compatible().is_err());
    control.stop().unwrap();
    assert!(control.ensure_recording_compatible().is_ok());
}

#[test]
fn locate_while_playing_crossfades_both_tracks_without_a_jump() {
    let dir = TempDir::new();
    let mut metadata = session(dir.path());
    metadata.tracks[0].files = vec!["a.wav".into()];
    let audio: Vec<_> = (0..2 * SR)
        .map(|i| if i < SR { 0.25 } else { -0.25 })
        .collect();
    track(dir.path(), "a.wav", &audio);
    track(
        dir.path(),
        "b.wav",
        &audio.iter().map(|x| -*x).collect::<Vec<_>>(),
    );
    save(dir.path(), &metadata);
    let (mut tap, mut control) = playback_channel(SR, 2).unwrap();
    control
        .load(fixture_open(dir.path(), 2, SR).unwrap())
        .unwrap();
    control.set_source(0, true).unwrap();
    control.set_source(1, true).unwrap();
    control.play().unwrap();
    let mut inputs = vec![vec![0.0; 256]; 2];
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        tap.process(&mut inputs, 256);
        if inputs[0][255] == 0.25 {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }

    control.locate(1.5).unwrap();
    let mut previous = 0.25;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        tap.process(&mut inputs, 256);
        for (&left, &right) in inputs[0].iter().zip(&inputs[1]) {
            assert_eq!(left, -right);
            assert!((left - previous).abs() < 0.002, "{previous} -> {left}");
            previous = left;
        }
        if previous == -0.25 {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }

    control.set_loop(true).unwrap();
    tap.process(&mut inputs, 256);
    assert!((inputs[0][0] + 0.25).abs() < 0.002);
    assert!(inputs[0][239].abs() < 0.002);
}
