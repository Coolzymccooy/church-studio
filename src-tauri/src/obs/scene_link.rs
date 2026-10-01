//! Scene link: Studio scene ⇄ OBS scene mapping, as pure functions plus a
//! loop guard. Lookups are case-insensitive on trimmed names, and a target is
//! only returned when it exists in the other side's current scene list (with
//! that side's exact spelling).
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Changes we caused are ignored when they echo back within this window.
pub const ECHO_WINDOW: Duration = Duration::from_secs(2);
const MAX_ENTRIES: usize = 64;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SceneLinkConfig {
    pub enabled: bool,
    pub studio_to_obs: BTreeMap<String, String>,
    pub obs_to_studio: BTreeMap<String, String>,
}

pub fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

fn clean_map(map: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    map.iter()
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .take(MAX_ENTRIES)
        .collect()
}

impl SceneLinkConfig {
    /// Trimmed copy without empty rows (an empty dropdown means "no link").
    pub fn cleaned(&self) -> Self {
        Self {
            enabled: self.enabled,
            studio_to_obs: clean_map(&self.studio_to_obs),
            obs_to_studio: clean_map(&self.obs_to_studio),
        }
    }
}

/// Case-insensitive, trimmed map lookup; returns the trimmed mapped value.
pub fn lookup(map: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let wanted = normalize(key);
    if wanted.is_empty() {
        return None;
    }
    map.iter()
        .find(|(k, _)| normalize(k) == wanted)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The exact spelling of `wanted` in `available`, matched case-insensitively.
pub fn resolve(available: &[String], wanted: &str) -> Option<String> {
    let key = normalize(wanted);
    available.iter().find(|n| normalize(n) == key).cloned()
}

/// OBS scene to switch to when `studio_scene` is loaded.
pub fn studio_to_obs(cfg: &SceneLinkConfig, studio_scene: &str, obs_scenes: &[String]) -> Option<String> {
    if !cfg.enabled {
        return None;
    }
    let mapped = lookup(&cfg.studio_to_obs, studio_scene)?;
    resolve(obs_scenes, &mapped)
}

/// Studio scene to load when OBS switches to `obs_scene`.
pub fn obs_to_studio(cfg: &SceneLinkConfig, obs_scene: &str, studio_scenes: &[String]) -> Option<String> {
    if !cfg.enabled {
        return None;
    }
    let mapped = lookup(&cfg.obs_to_studio, obs_scene)?;
    resolve(studio_scenes, &mapped)
}

pub fn same_scene(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// Which side a change was made on. Studio-side loads made by the link go
/// through `load_scene_state`, which never notifies the hook, so today only
/// `Obs` is recorded; `Studio` keeps the guard symmetric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum LinkSide {
    Obs,
    Studio,
}

/// Remembers scene changes we caused so their echo doesn't bounce back.
#[derive(Debug)]
pub struct SceneLinkGuard {
    window: Duration,
    pending: Vec<(LinkSide, String, Instant)>,
}

impl Default for SceneLinkGuard {
    fn default() -> Self {
        Self::new(ECHO_WINDOW)
    }
}

impl SceneLinkGuard {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            pending: Vec::new(),
        }
    }

    fn prune(&mut self, now: Instant) {
        let window = self.window;
        self.pending
            .retain(|(_, _, at)| now.saturating_duration_since(*at) <= window);
    }

    /// We are about to switch `side` to `scene`.
    pub fn note_caused(&mut self, side: LinkSide, scene: &str, now: Instant) {
        self.prune(now);
        let key = normalize(scene);
        self.pending.retain(|(s, n, _)| !(*s == side && *n == key));
        self.pending.push((side, key, now));
        if self.pending.len() > MAX_ENTRIES {
            self.pending.remove(0);
        }
    }

    /// True (once) if `side` changing to `scene` is the echo of our own
    /// change within the window.
    pub fn is_echo(&mut self, side: LinkSide, scene: &str, now: Instant) -> bool {
        self.prune(now);
        let key = normalize(scene);
        match self.pending.iter().position(|(s, n, _)| *s == side && *n == key) {
            Some(index) => {
                self.pending.remove(index);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn cfg() -> SceneLinkConfig {
        SceneLinkConfig {
            enabled: true,
            studio_to_obs: map(&[("Worship", "Band Wide"), ("Sermon ", " Pulpit Cam")]),
            obs_to_studio: map(&[("Pulpit Cam", "Sermon"), ("Walk-in Loop", "Walk-in")]),
        }
    }

    #[test]
    fn lookup_is_case_insensitive_and_trimmed() {
        let m = map(&[(" Worship ", " Band Wide ")]);
        assert_eq!(lookup(&m, "worship").as_deref(), Some("Band Wide"));
        assert_eq!(lookup(&m, "  WORSHIP"), Some("Band Wide".to_string()));
        assert_eq!(lookup(&m, "Sermon"), None);
        assert_eq!(lookup(&m, "   "), None);
    }

    #[test]
    fn studio_to_obs_uses_obs_spelling() {
        let obs = names(&["BAND WIDE", "Pulpit Cam"]);
        assert_eq!(studio_to_obs(&cfg(), "worship", &obs).as_deref(), Some("BAND WIDE"));
        assert_eq!(studio_to_obs(&cfg(), "sermon", &obs).as_deref(), Some("Pulpit Cam"));
    }

    #[test]
    fn missing_target_scene_means_no_switch() {
        let obs = names(&["Pulpit Cam"]);
        assert_eq!(studio_to_obs(&cfg(), "Worship", &obs), None);
        assert_eq!(studio_to_obs(&cfg(), "Unknown", &obs), None);
    }

    #[test]
    fn obs_to_studio_maps_reverse() {
        let studio = names(&["Sermon", "Walk-in", "Worship"]);
        assert_eq!(obs_to_studio(&cfg(), " pulpit cam ", &studio).as_deref(), Some("Sermon"));
        assert_eq!(obs_to_studio(&cfg(), "Walk-in Loop", &studio).as_deref(), Some("Walk-in"));
        assert_eq!(obs_to_studio(&cfg(), "Band Wide", &studio), None);
    }

    #[test]
    fn disabled_link_never_maps() {
        let mut c = cfg();
        c.enabled = false;
        assert_eq!(studio_to_obs(&c, "Worship", &names(&["Band Wide"])), None);
        assert_eq!(obs_to_studio(&c, "Pulpit Cam", &names(&["Sermon"])), None);
    }

    #[test]
    fn cleaned_drops_empty_rows_and_trims() {
        let c = SceneLinkConfig {
            enabled: true,
            studio_to_obs: map(&[(" A ", " X "), ("B", "  "), ("", "Y")]),
            obs_to_studio: BTreeMap::new(),
        }
        .cleaned();
        assert_eq!(c.studio_to_obs, map(&[("A", "X")]));
    }

    #[test]
    fn config_serializes_camel_case() {
        let json = serde_json::to_value(cfg()).unwrap();
        assert!(json.get("studioToObs").is_some());
        assert!(json.get("obsToStudio").is_some());
        let back: SceneLinkConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, cfg());
        let empty: SceneLinkConfig = serde_json::from_str("{}").unwrap();
        assert!(!empty.enabled);
    }

    #[test]
    fn guard_swallows_echo_once_within_window() {
        let t0 = Instant::now();
        let mut g = SceneLinkGuard::default();
        g.note_caused(LinkSide::Obs, "Band Wide", t0);
        assert!(g.is_echo(LinkSide::Obs, "band wide ", t0 + Duration::from_millis(300)));
        // Consumed: a later genuine change to the same scene goes through.
        assert!(!g.is_echo(LinkSide::Obs, "Band Wide", t0 + Duration::from_millis(400)));
    }

    #[test]
    fn guard_expires_after_window() {
        let t0 = Instant::now();
        let mut g = SceneLinkGuard::default();
        g.note_caused(LinkSide::Obs, "Band Wide", t0);
        assert!(!g.is_echo(LinkSide::Obs, "Band Wide", t0 + Duration::from_millis(2100)));
    }

    #[test]
    fn guard_is_per_side_and_per_scene() {
        let t0 = Instant::now();
        let mut g = SceneLinkGuard::default();
        g.note_caused(LinkSide::Obs, "Band Wide", t0);
        assert!(!g.is_echo(LinkSide::Studio, "Band Wide", t0));
        assert!(!g.is_echo(LinkSide::Obs, "Pulpit Cam", t0));
        assert!(g.is_echo(LinkSide::Obs, "Band Wide", t0));
    }

    #[test]
    fn same_scene_ignores_case_and_space() {
        assert!(same_scene(" Worship", "worship "));
        assert!(!same_scene("Worship", "Sermon"));
    }
}
