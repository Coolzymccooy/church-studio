//! Which tracks a recording writes (design decision 1) and where (decision
//! 6). Pure: the Tauri command gathers strip names, NDI strips, config and
//! the clock, and this turns them into a `WriterPlan`.
use super::config::RecorderConfig;
use super::frames::RecordLayout;
use super::names::{civil_from_unix, folder_name, iso_local, iso_utc, strip_track_base};
use super::session::TrackKind;
use super::writer::{SessionStamp, TrackPlan};

pub const STREAM_MIX_BASE: &str = "Stream Mix";
pub const MAIN_MIX_BASE: &str = "Main Mix";
/// Largest accepted UTC offset (±14 h).
const MAX_OFFSET_MINUTES: i32 = 14 * 60;

/// Armed strips (raw, mono), then the Stream mix, then Main when included.
/// `names[i]` is strip i's name; `ndi_strips` lists the strips fed by NDI.
pub fn build_tracks(
    layout: RecordLayout,
    names: &[String],
    ndi_strips: &[usize],
    config: &RecorderConfig,
) -> Vec<TrackPlan> {
    let mut tracks = Vec::with_capacity(layout.strips + 2);
    for index in (0..layout.strips).filter(|&i| config.is_armed(i)) {
        let name = names
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("Ch {}", index + 1));
        let source = if ndi_strips.contains(&index) { "ndi" } else { "hardware" };
        tracks.push(TrackPlan {
            base: strip_track_base(index, &name),
            name,
            kind: TrackKind::Strip,
            source: source.to_string(),
            strip: Some(index as u32),
            first_channel: index,
            channels: 1,
        });
    }
    tracks.push(bus_track(STREAM_MIX_BASE, "stream", layout.stream_channel()));
    if config.include_main {
        tracks.push(bus_track(MAIN_MIX_BASE, "main", layout.main_channel()));
    }
    tracks
}

fn bus_track(base: &str, source: &str, first_channel: usize) -> TrackPlan {
    TrackPlan {
        base: base.to_string(),
        name: base.to_string(),
        kind: TrackKind::Bus,
        source: source.to_string(),
        strip: None,
        first_channel,
        channels: 2,
    }
}

/// Clamp a UTC offset from the UI to a real one.
pub fn clamp_offset(offset_minutes: Option<i32>) -> i32 {
    offset_minutes
        .unwrap_or(0)
        .clamp(-MAX_OFFSET_MINUTES, MAX_OFFSET_MINUTES)
}

/// The recording folder's name and the session timestamps for a start at
/// `unix_secs` with the local `offset_minutes`.
pub fn stamp_for(
    unix_secs: i64,
    offset_minutes: i32,
    title: Option<&str>,
    app_version: &str,
) -> (String, SessionStamp) {
    let local = civil_from_unix(unix_secs + offset_minutes as i64 * 60);
    let title = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let folder = folder_name(&local, title.as_deref());
    let stamp = SessionStamp {
        app_version: app_version.to_string(),
        title,
        started_local: iso_local(unix_secs, offset_minutes),
        started_utc: iso_utc(unix_secs),
    };
    (folder, stamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_strip_and_the_stream_mix_by_default() {
        let layout = RecordLayout { strips: 3 };
        let tracks = build_tracks(layout, &names(&["Pastor", "Choir", "Zoom"]), &[2], &RecorderConfig::default());
        let bases: Vec<&str> = tracks.iter().map(|t| t.base.as_str()).collect();
        assert_eq!(bases, vec!["01 Pastor", "02 Choir", "03 Zoom", "Stream Mix"]);
        assert_eq!(tracks[0].source, "hardware");
        assert_eq!(tracks[2].source, "ndi");
        assert_eq!(tracks[2].first_channel, 2);
        assert_eq!((tracks[3].first_channel, tracks[3].channels), (3, 2));
        assert_eq!(tracks[3].kind, TrackKind::Bus);
    }

    #[test]
    fn disarmed_strips_are_skipped_and_main_is_optional() {
        let layout = RecordLayout { strips: 3 };
        let config = RecorderConfig {
            include_main: true,
            armed_strips: Some(vec![1]),
            ..RecorderConfig::default()
        };
        let tracks = build_tracks(layout, &names(&["A", "B/C"]), &[], &config);
        let bases: Vec<&str> = tracks.iter().map(|t| t.base.as_str()).collect();
        assert_eq!(bases, vec!["02 BC", "Stream Mix", "Main Mix"]);
        assert_eq!(tracks[0].name, "B/C", "the display name is kept");
        assert_eq!(tracks[2].first_channel, 5);
    }

    #[test]
    fn missing_names_fall_back() {
        let tracks = build_tracks(RecordLayout { strips: 1 }, &[], &[], &RecorderConfig::default());
        assert_eq!(tracks[0].base, "01 Ch 1");
    }

    #[test]
    fn stamps_use_local_time_for_the_folder() {
        let (folder, stamp) = stamp_for(1_790_847_005, 60, Some(" Sunday AM "), "1.5.0");
        assert_eq!(folder, "2026-10-01 1030 Sunday AM");
        assert_eq!(stamp.title.as_deref(), Some("Sunday AM"));
        assert_eq!(stamp.started_utc, "2026-10-01T09:30:05Z");
        assert_eq!(stamp.started_local, "2026-10-01T10:30:05+01:00");
        let (folder, stamp) = stamp_for(1_790_847_005, 0, Some("  "), "1.5.0");
        assert_eq!(folder, "2026-10-01 0930");
        assert_eq!(stamp.title, None);
    }

    #[test]
    fn offsets_are_clamped() {
        assert_eq!(clamp_offset(None), 0);
        assert_eq!(clamp_offset(Some(60)), 60);
        assert_eq!(clamp_offset(Some(100_000)), 840);
        assert_eq!(clamp_offset(Some(-100_000)), -840);
    }
}
