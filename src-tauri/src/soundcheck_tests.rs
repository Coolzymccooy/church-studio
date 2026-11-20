use crate::recorder::session::{
    SessionFile, SessionTrack, TrackKind, SESSION_FORMAT, SESSION_FORMAT_VERSION,
};
use crate::recorder::wav::header_bytes;
use crate::soundcheck::{open_session, playback_channel};
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

#[test]
fn session_stitches_rollover_and_resamples_an_aligned_frame() {
    let dir = TempDir::new();
    save(dir.path(), &session(dir.path()));
    let mut source = open_session(&dir.path().join("session.json"), 2, SR * 2).unwrap();
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
    assert!(open_session(&dir.path().join("session.json"), 2, SR).is_err());
    metadata.tracks[0].files[0] = "a.wav".into();
    metadata.tracks[1].strip = Some(0);
    save(dir.path(), &metadata);
    assert!(open_session(&dir.path().join("session.json"), 2, SR).is_err());
}

#[test]
fn seek_and_loop_keep_mapped_strips_in_step_and_leave_live_inputs_alone() {
    let dir = TempDir::new();
    save(dir.path(), &session(dir.path()));
    let (mut tap, mut control) = playback_channel(SR, 3).unwrap();
    control
        .load(open_session(&dir.path().join("session.json"), 3, SR).unwrap())
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
    assert_eq!(inputs[0][..3], [0.25, 0.5, 0.75]);
    assert_eq!(inputs[1][..3], [-0.25, -0.5, 0.0]);
    assert_eq!(inputs[2][..3], [0.9; 3]);
    tap.process(&mut inputs, 3);
    assert_eq!(inputs[0][..3], [0.0, 0.25, 0.5]);
    control.stop().unwrap();
    tap.process(&mut inputs, 3);
    assert_eq!(inputs[0][..3], [0.0; 3]);
}
