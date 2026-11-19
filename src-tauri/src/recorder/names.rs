//! File and folder names for a recording (design decision 6), plus the
//! clock maths for `session.json` timestamps. Pure; no I/O.
//!
//! - `sanitize_name`: no path separators, reserved Windows names, control
//!   characters or trailing dots/spaces, so a strip name or title can never
//!   address a path outside the recording folder.
//! - `folder_name`: `YYYY-MM-DD HHmm <optional title>`.
//! - `part_file_name`: `<base>.wav`, then `<base> part 2.wav` …

/// Longest sanitised name, in characters.
pub const MAX_NAME_CHARS: usize = 60;

const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A safe single path component from `raw`: forbidden and control
/// characters removed, whitespace collapsed, at most `MAX_NAME_CHARS`,
/// no leading/trailing dots or spaces, and a reserved device name gets a
/// trailing `_`. Empty results become `fallback`.
pub fn sanitize_name(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !FORBIDDEN.contains(c))
        .collect();
    let collapsed = cleaned.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_NAME_CHARS).collect();
    let trimmed = capped.trim_matches(|c| c == '.' || c == ' ').to_string();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    if is_reserved(&trimmed) {
        return format!("{trimmed}_");
    }
    trimmed
}

/// `CON`, `nul.txt`, `Com1` … are Windows device names.
fn is_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

/// `01 Pastor` for strip 0 named "Pastor".
pub fn strip_track_base(index: usize, name: &str) -> String {
    let fallback = format!("Ch {}", index + 1);
    format!("{:02} {}", index + 1, sanitize_name(name, &fallback))
}

/// Part 1 is `<base>.wav`; later parts are `<base> part N.wav`.
pub fn part_file_name(base: &str, part: u32) -> String {
    if part <= 1 {
        format!("{base}.wav")
    } else {
        format!("{base} part {part}.wav")
    }
}

/// A calendar date and time of day (no time zone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// Seconds since the Unix epoch → calendar time (proleptic Gregorian;
/// Howard Hinnant's `civil_from_days`).
pub fn civil_from_unix(secs: i64) -> CivilTime {
    let days = secs.div_euclid(86_400);
    let of_day = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month_i: i64 = if mp < 10 { mp + 3 } else { mp - 9 };
    let month = month_i as u32;
    let leap_shift: i64 = if month <= 2 { 1 } else { 0 };
    let year = yoe + era * 400 + leap_shift;
    CivilTime {
        year,
        month,
        day,
        hour: (of_day / 3_600) as u32,
        minute: (of_day % 3_600 / 60) as u32,
        second: (of_day % 60) as u32,
    }
}

/// `2026-10-01T09:30:05Z`.
pub fn iso_utc(secs: i64) -> String {
    let t = civil_from_unix(secs);
    format!("{}T{:02}:{:02}:{:02}Z", date_text(&t), t.hour, t.minute, t.second)
}

/// `2026-10-01T10:30:05+01:00` for a local offset of +60 minutes.
pub fn iso_local(secs: i64, offset_minutes: i32) -> String {
    let t = civil_from_unix(secs + offset_minutes as i64 * 60);
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let abs = offset_minutes.unsigned_abs();
    format!(
        "{}T{:02}:{:02}:{:02}{sign}{:02}:{:02}",
        date_text(&t),
        t.hour,
        t.minute,
        t.second,
        abs / 60,
        abs % 60
    )
}

fn date_text(t: &CivilTime) -> String {
    format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
}

/// `2026-10-01 1030 Sunday Service` (local time; the title is optional).
pub fn folder_name(local: &CivilTime, title: Option<&str>) -> String {
    let stamp = format!("{} {:02}{:02}", date_text(local), local.hour, local.minute);
    match title.map(|t| sanitize_name(t, "")).filter(|t| !t.is_empty()) {
        Some(title) => format!("{stamp} {title}"),
        None => stamp,
    }
}

/// `name`, or `name (2)`, `name (3)` … for the first that `taken` rejects.
pub fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    (2..10_000)
        .map(|n| format!("{name} ({n})"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| format!("{name} (new)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_separators_and_control_characters() {
        assert_eq!(sanitize_name("Choir L/R", "x"), "Choir LR");
        assert_eq!(sanitize_name("..\\..\\evil", "x"), "evil");
        assert_eq!(sanitize_name("a:b*c?d\"e<f>g|h", "x"), "abcdefgh");
        assert_eq!(sanitize_name("Tab\there\u{7}", "x"), "Tab here");
        assert_eq!(sanitize_name("  lots   of   space  ", "x"), "lots of space");
    }

    #[test]
    fn sanitize_drops_trailing_dots_and_handles_empty() {
        assert_eq!(sanitize_name("Sermon...", "x"), "Sermon");
        assert_eq!(sanitize_name("Sermon. . ", "x"), "Sermon");
        assert_eq!(sanitize_name("...", "Ch 1"), "Ch 1");
        assert_eq!(sanitize_name("", "Ch 2"), "Ch 2");
        assert_eq!(sanitize_name("///", "Ch 3"), "Ch 3");
    }

    #[test]
    fn sanitize_escapes_reserved_windows_names() {
        assert_eq!(sanitize_name("CON", "x"), "CON_");
        assert_eq!(sanitize_name("nul", "x"), "nul_");
        assert_eq!(sanitize_name("Com1.wav", "x"), "Com1.wav_");
        assert_eq!(sanitize_name("Console", "x"), "Console");
    }

    #[test]
    fn sanitize_caps_the_length() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_name(&long, "x").chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn strip_bases_are_numbered_from_one() {
        assert_eq!(strip_track_base(0, "Pastor"), "01 Pastor");
        assert_eq!(strip_track_base(11, ""), "12 Ch 12");
    }

    #[test]
    fn rollover_names() {
        assert_eq!(part_file_name("01 Pastor", 1), "01 Pastor.wav");
        assert_eq!(part_file_name("01 Pastor", 2), "01 Pastor part 2.wav");
        assert_eq!(part_file_name("Stream Mix", 3), "Stream Mix part 3.wav");
    }

    #[test]
    fn civil_time_from_known_epochs() {
        assert_eq!(
            civil_from_unix(0),
            CivilTime { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0 }
        );
        // 2026-10-01T09:30:05Z
        let t = civil_from_unix(1_790_847_005);
        assert_eq!((t.year, t.month, t.day, t.hour, t.minute, t.second), (2026, 10, 1, 9, 30, 5));
        // Leap day.
        let t = civil_from_unix(951_782_400);
        assert_eq!((t.year, t.month, t.day), (2000, 2, 29));
    }

    #[test]
    fn iso_strings_in_utc_and_local() {
        assert_eq!(iso_utc(1_790_847_005), "2026-10-01T09:30:05Z");
        assert_eq!(iso_local(1_790_847_005, 60), "2026-10-01T10:30:05+01:00");
        assert_eq!(iso_local(1_790_847_005, -330), "2026-10-01T04:00:05-05:30");
    }

    #[test]
    fn folder_names_use_local_time_and_a_clean_title() {
        let t = civil_from_unix(1_790_847_005 + 3_600);
        assert_eq!(folder_name(&t, None), "2026-10-01 1030");
        assert_eq!(folder_name(&t, Some("  ")), "2026-10-01 1030");
        assert_eq!(folder_name(&t, Some("Sunday: Service/AM")), "2026-10-01 1030 Sunday ServiceAM");
    }

    #[test]
    fn unique_name_appends_a_counter() {
        assert_eq!(unique_name("A", |_| false), "A");
        assert_eq!(unique_name("A", |n| n == "A"), "A (2)");
        assert_eq!(unique_name("A", |n| n == "A" || n == "A (2)"), "A (3)");
    }
}
