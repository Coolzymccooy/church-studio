//! Checks on what the UI asks to export, kept apart from Tauri so they can
//! be tested: which recorded track (only files `session.json` lists, inside
//! the recording's own folder) and where an editor export may be saved.
use super::ExportFormat;
use crate::recorder::session::{SessionFile, SESSION_FILE};
use std::path::{Path, PathBuf};

/// A bare file name: no folders, no `..`, not empty.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && Path::new(name).file_name().is_some_and(|n| n == name)
}

/// The track whose first file is `file`, as full paths of every part.
pub struct RecordedTrack {
    pub name: String,
    pub parts: Vec<PathBuf>,
    pub title: Option<String>,
}

pub fn find_track(folder: &Path, session: &SessionFile, file: &str) -> Result<RecordedTrack, String> {
    let track = session
        .tracks
        .iter()
        .find(|t| t.file == file)
        .ok_or_else(|| format!("{file} is not a track of the last recording"))?;
    let names = if track.files.is_empty() { vec![track.file.clone()] } else { track.files.clone() };
    if !names.iter().all(|n| is_plain_file_name(n)) {
        return Err("session.json lists a file outside the recording folder".to_string());
    }
    Ok(RecordedTrack {
        name: track.name.clone(),
        parts: names.iter().map(|n| folder.join(n)).collect(),
        title: session.title.clone(),
    })
}

pub fn read_session(folder: &Path) -> Result<SessionFile, String> {
    let path = folder.join(SESSION_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// MP3 title tag: "Sunday · Stream Mix", or the track name alone.
pub fn tag_title(session_title: Option<&str>, track: &str) -> String {
    match session_title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(title) => format!("{title} · {track}"),
        None => track.to_string(),
    }
}

/// Decode a `%XX`-escaped UTF-8 header value (from `encodeURIComponent`).
pub fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or("bad %-escape in the export path")?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "the export path is not UTF-8".to_string())
}

/// An editor export target: a full path whose extension matches the
/// format, in a folder that exists.
pub fn check_dest(dest: &str, format: ExportFormat) -> Result<PathBuf, String> {
    let path = PathBuf::from(dest);
    if !path.is_absolute() {
        return Err("choose where to save the export".to_string());
    }
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(format.extension()));
    if !ext_ok {
        return Err(format!("the file name must end in .{}", format.extension()));
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err("that folder does not exist".to_string());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::session::{SessionTrack, TrackKind, SESSION_FORMAT, SESSION_FORMAT_VERSION};

    fn session(files: Vec<&str>) -> SessionFile {
        SessionFile {
            format: SESSION_FORMAT.to_string(),
            format_version: SESSION_FORMAT_VERSION,
            app_version: "1.5.1".to_string(),
            title: Some("Sunday".to_string()),
            sample_rate: 48_000,
            started_local: String::new(),
            started_utc: String::new(),
            duration_seconds: 1.0,
            dropped_frames: 0,
            tracks: vec![SessionTrack {
                file: files[0].to_string(),
                files: files.iter().map(|f| f.to_string()).collect(),
                name: "Stream Mix".to_string(),
                kind: TrackKind::Bus,
                channels: 2,
                source: "stream".to_string(),
                strip: None,
            }],
            markers: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn plain_file_names_only() {
        assert!(is_plain_file_name("Stream Mix.wav"));
        assert!(is_plain_file_name("01 Pastor part 2.wav"));
        for bad in ["", ".", "..", "../x.wav", "a/b.wav", "a\\b.wav", "C:x.wav", "/etc/passwd"] {
            assert!(!is_plain_file_name(bad), "{bad}");
        }
    }

    #[test]
    fn finds_a_track_and_all_its_parts() {
        let folder = Path::new("/rec/Sunday");
        let track = find_track(folder, &session(vec!["Stream Mix.wav", "Stream Mix part 2.wav"]), "Stream Mix.wav").unwrap();
        assert_eq!(track.name, "Stream Mix");
        assert_eq!(track.parts, vec![folder.join("Stream Mix.wav"), folder.join("Stream Mix part 2.wav")]);
        assert_eq!(track.title.as_deref(), Some("Sunday"));
    }

    #[test]
    fn refuses_unknown_tracks_and_escaping_parts() {
        let folder = Path::new("/rec/Sunday");
        assert!(find_track(folder, &session(vec!["Stream Mix.wav"]), "Other.wav").is_err());
        let err = find_track(folder, &session(vec!["Stream Mix.wav", "../../secret.wav"]), "Stream Mix.wav")
            .err()
            .unwrap();
        assert!(err.contains("outside"), "{err}");
    }

    #[test]
    fn titles_join_the_service_and_the_track() {
        assert_eq!(tag_title(Some("Sunday"), "Stream Mix"), "Sunday · Stream Mix");
        assert_eq!(tag_title(Some("  "), "Stream Mix"), "Stream Mix");
        assert_eq!(tag_title(None, "Pastor"), "Pastor");
    }

    #[test]
    fn percent_decoding_round_trips_encode_uri_component() {
        assert_eq!(percent_decode("%2FUsers%2Fsegun%2FS%C3%A9rmon%20mix.mp3").unwrap(), "/Users/segun/Sérmon mix.mp3");
        assert_eq!(percent_decode("plain").unwrap(), "plain");
        assert!(percent_decode("%2").is_err());
        assert!(percent_decode("%FF").is_err());
    }

    #[test]
    fn editor_destinations_are_checked() {
        let dir = std::env::temp_dir();
        let ok = dir.join("edited.mp3");
        assert_eq!(check_dest(ok.to_str().unwrap(), ExportFormat::Mp3).unwrap(), ok);
        assert!(check_dest(dir.join("edited.M4A").to_str().unwrap(), ExportFormat::M4a).is_ok());
        assert!(check_dest("edited.mp3", ExportFormat::Mp3).unwrap_err().contains("choose"));
        assert!(check_dest(dir.join("edited.wav").to_str().unwrap(), ExportFormat::Mp3).is_err());
        let missing = dir.join("no-such-folder-tiwaton").join("x.mp3");
        assert!(check_dest(missing.to_str().unwrap(), ExportFormat::Mp3).unwrap_err().contains("folder"));
    }
}
