//! Automation rules: map Lumina events to audio scenes (pure, no I/O).
//!
//! A rule is `{ id, enabled, when: {event, contentKind?, itemType?},
//! then: {loadScene} }`; the first enabled rule that matches and whose scene
//! exists wins. `Debouncer` then holds a target until it has been stable for
//! `DEBOUNCE_MS`, so fast slide changes never flap the mix, and never
//! re-applies the scene that is already loaded.
use crate::link::protocol::{self, LuminaEvent};
use crate::mixer_control::scene_slug;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DEBOUNCE_MS: u64 = 1_500;
pub const MAX_RULES: usize = 50;
pub const MAX_RULE_ID_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleWhen {
    pub event: String,
    #[serde(default)]
    pub content_kind: Option<String>,
    #[serde(default)]
    pub item_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleThen {
    pub load_scene: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub when: RuleWhen,
    pub then: RuleThen,
}

fn default_true() -> bool {
    true
}

fn rule(id: &str, event: &str, content_kind: Option<&str>, scene: &str) -> Rule {
    Rule {
        id: id.to_string(),
        enabled: true,
        when: RuleWhen {
            event: event.to_string(),
            content_kind: content_kind.map(str::to_string),
            item_type: None,
        },
        then: RuleThen {
            load_scene: scene.to_string(),
        },
    }
}

/// The editable defaults from the integrations design (decision 8).
pub fn default_rules() -> Vec<Rule> {
    vec![
        rule("lyrics-worship", protocol::EVENT_SLIDE_CHANGED, Some("lyrics"), "Worship"),
        rule("scripture-sermon", protocol::EVENT_SLIDE_CHANGED, Some("scripture"), "Sermon"),
        rule("announcement-sermon", protocol::EVENT_SLIDE_CHANGED, Some("announcement"), "Sermon"),
        rule("countdown-walkin", protocol::EVENT_COUNTDOWN_STARTED, None, "Walk-in"),
    ]
}

fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// Validate a rule list from the UI; returns trimmed copies.
pub fn validate_rules(rules: &[Rule]) -> Result<Vec<Rule>, String> {
    if rules.len() > MAX_RULES {
        return Err(format!("at most {MAX_RULES} rules are allowed"));
    }
    let mut seen = HashSet::new();
    let mut cleaned = Vec::with_capacity(rules.len());
    for (i, r) in rules.iter().enumerate() {
        let n = i + 1;
        let id = r.id.trim().to_string();
        if id.is_empty() || id.chars().count() > MAX_RULE_ID_CHARS {
            return Err(format!("rule {n}: id must be 1-{MAX_RULE_ID_CHARS} characters"));
        }
        if !seen.insert(id.clone()) {
            return Err(format!("rule {n}: duplicate id '{id}'"));
        }
        let event = r.when.event.trim().to_string();
        if !protocol::is_valid_event_name(&event) {
            return Err(format!("rule {n}: event name is not valid"));
        }
        let scene = r.then.load_scene.trim().to_string();
        scene_slug(&scene).map_err(|e| format!("rule {n}: {e}"))?;
        let content_kind = clean_opt(&r.when.content_kind);
        let item_type = clean_opt(&r.when.item_type);
        for value in [&content_kind, &item_type].into_iter().flatten() {
            if value.chars().count() > protocol::MAX_FIELD_CHARS {
                return Err(format!("rule {n}: condition is too long"));
            }
        }
        cleaned.push(Rule {
            id,
            enabled: r.enabled,
            when: RuleWhen {
                event,
                content_kind,
                item_type,
            },
            then: RuleThen { load_scene: scene },
        });
    }
    Ok(cleaned)
}

/// What a rule is matched against.
#[derive(Debug, Clone, Copy)]
pub struct RuleInput<'a> {
    pub event: &'a str,
    pub content_kind: Option<&'a str>,
    pub item_type: Option<&'a str>,
}

impl<'a> RuleInput<'a> {
    pub fn from_event(ev: &'a LuminaEvent) -> Self {
        RuleInput {
            event: &ev.event,
            content_kind: ev.content_kind(),
            item_type: ev.item_type.as_deref(),
        }
    }
}

/// `lumina.slide.changed` and `slide.changed` name the same event.
fn normalize_event(event: &str) -> String {
    let lower = event.trim().to_ascii_lowercase();
    let stripped = lower.strip_prefix("lumina.").map(str::to_string);
    stripped.unwrap_or(lower)
}

fn condition_matches(want: &Option<String>, have: Option<&str>) -> bool {
    match want {
        None => true,
        Some(want) => have.map_or(false, |have| have.trim().eq_ignore_ascii_case(want.trim())),
    }
}

pub fn rule_matches(rule: &Rule, input: &RuleInput<'_>) -> bool {
    rule.enabled
        && normalize_event(&rule.when.event) == normalize_event(input.event)
        && condition_matches(&rule.when.content_kind, input.content_kind)
        && condition_matches(&rule.when.item_type, input.item_type)
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchedRule {
    pub rule_id: String,
    /// The scene's saved name (as returned by the lookup).
    pub scene: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MatchResult {
    pub target: Option<MatchedRule>,
    /// Rules that matched but whose scene does not exist (in rule order).
    pub skipped: Vec<MatchedRule>,
}

/// First enabled matching rule whose scene exists. `scene_lookup` maps a
/// rule's scene name to the saved scene's name, or `None` when missing.
pub fn match_rules(
    rules: &[Rule],
    input: &RuleInput<'_>,
    scene_lookup: &dyn Fn(&str) -> Option<String>,
) -> MatchResult {
    let mut skipped = Vec::new();
    for r in rules.iter().filter(|r| rule_matches(r, input)) {
        match scene_lookup(&r.then.load_scene) {
            Some(scene) => {
                return MatchResult {
                    target: Some(MatchedRule {
                        rule_id: r.id.clone(),
                        scene,
                    }),
                    skipped,
                }
            }
            None => skipped.push(MatchedRule {
                rule_id: r.id.clone(),
                scene: r.then.load_scene.clone(),
            }),
        }
    }
    MatchResult {
        target: None,
        skipped,
    }
}

/// Events whose arrival means "the live content changed": when no rule
/// matches one of these, a pending scene change is cancelled.
pub fn changes_live_content(event: &str) -> bool {
    matches!(
        event,
        protocol::EVENT_SLIDE_CHANGED | protocol::EVENT_ITEM_STARTED
    )
}

/// A scene change the rule engine wants applied.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneToLoad {
    pub scene: String,
    pub rule_id: String,
    pub event: String,
    pub content_kind: Option<String>,
}

fn same_scene(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Debounce state machine, fed with `(target, now_ms)` and a timer `tick`.
/// Time is caller-supplied milliseconds on any monotonic clock.
#[derive(Debug)]
pub struct Debouncer {
    stable_ms: u64,
    pending: Option<(SceneToLoad, u64)>,
    current: Option<String>,
}

impl Debouncer {
    pub fn new(stable_ms: u64) -> Self {
        Debouncer {
            stable_ms,
            pending: None,
            current: None,
        }
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn pending(&self) -> Option<&SceneToLoad> {
        self.pending.as_ref().map(|(t, _)| t)
    }

    /// Record the scene that is actually loaded (manual load, undo, or a
    /// failed apply). A pending change to that same scene is dropped.
    pub fn set_current(&mut self, scene: Option<String>) {
        let drop_pending = match (&self.pending, &scene) {
            (Some((t, _)), Some(s)) => same_scene(&t.scene, s),
            _ => false,
        };
        if drop_pending {
            self.pending = None;
        }
        self.current = scene;
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Feed one event's target (`None`: live content with no rule, which
    /// cancels any pending change). Returns a scene when it is due now.
    pub fn on_event(&mut self, target: Option<SceneToLoad>, now_ms: u64) -> Option<SceneToLoad> {
        let target = match target {
            Some(t) => t,
            None => {
                self.pending = None;
                return None;
            }
        };
        let is_current = self
            .current
            .as_deref()
            .map_or(false, |c| same_scene(c, &target.scene));
        if is_current {
            self.pending = None;
            return None;
        }
        let same_pending = self
            .pending
            .as_ref()
            .map_or(false, |(p, _)| same_scene(&p.scene, &target.scene));
        if !same_pending {
            self.pending = Some((target, now_ms));
        }
        self.tick(now_ms)
    }

    /// Fire the pending change once it has been stable for `stable_ms`.
    pub fn tick(&mut self, now_ms: u64) -> Option<SceneToLoad> {
        let due = match &self.pending {
            Some((_, since)) => now_ms.saturating_sub(*since) >= self.stable_ms,
            None => false,
        };
        if !due {
            return None;
        }
        let (target, _) = self.pending.take()?;
        self.current = Some(target.scene.clone());
        Some(target)
    }
}
