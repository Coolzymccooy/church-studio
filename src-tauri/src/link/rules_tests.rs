//! Unit tests for `link::rules`: matching, validation and the debouncer.
use super::protocol::{BridgeHeaders, EVENT_COUNTDOWN_STARTED, EVENT_SLIDE_CHANGED};
use super::rules::*;

const SCENES: [&str; 3] = ["Worship", "Sermon", "Walk-in"];

fn lookup(name: &str) -> Option<String> {
    SCENES
        .iter()
        .find(|s| s.eq_ignore_ascii_case(name.trim()))
        .map(|s| s.to_string())
}

fn slide(kind: &str) -> RuleInput<'_> {
    RuleInput {
        event: EVENT_SLIDE_CHANGED,
        content_kind: Some(kind),
        item_type: None,
    }
}

fn target(scene: &str) -> Option<SceneToLoad> {
    Some(SceneToLoad {
        scene: scene.to_string(),
        rule_id: format!("rule-{scene}"),
        event: EVENT_SLIDE_CHANGED.to_string(),
        content_kind: None,
    })
}

#[test]
fn default_rules_map_content_to_scenes() {
    let rules = default_rules();
    let scene_for = |input: RuleInput| match_rules(&rules, &input, &lookup).target.map(|t| t.scene);
    assert_eq!(scene_for(slide("lyrics")).as_deref(), Some("Worship"));
    assert_eq!(scene_for(slide("scripture")).as_deref(), Some("Sermon"));
    assert_eq!(scene_for(slide("announcement")).as_deref(), Some("Sermon"));
    assert_eq!(scene_for(slide("media")), None);
    let countdown = RuleInput {
        event: EVENT_COUNTDOWN_STARTED,
        content_kind: None,
        item_type: None,
    };
    assert_eq!(scene_for(countdown).as_deref(), Some("Walk-in"));
    assert!(validate_rules(&rules).is_ok());
}

#[test]
fn first_match_wins_and_disabled_rules_are_ignored() {
    let mut rules = default_rules();
    let mut override_rule = rules[0].clone();
    override_rule.id = "lyrics-sermon".into();
    override_rule.then.load_scene = "Sermon".into();
    rules.insert(0, override_rule);
    let hit = match_rules(&rules, &slide("lyrics"), &lookup).target.unwrap();
    assert_eq!(hit.rule_id, "lyrics-sermon");

    rules[0].enabled = false;
    let hit = match_rules(&rules, &slide("lyrics"), &lookup).target.unwrap();
    assert_eq!(hit.rule_id, "lyrics-worship");
}

#[test]
fn missing_scene_is_skipped_and_reported() {
    let mut rules = default_rules();
    let mut ghost = rules[0].clone();
    ghost.id = "lyrics-ghost".into();
    ghost.then.load_scene = "Choir Only".into();
    rules.insert(0, ghost);
    let result = match_rules(&rules, &slide("lyrics"), &lookup);
    assert_eq!(result.target.unwrap().scene, "Worship");
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].scene, "Choir Only");

    let none = |_: &str| -> Option<String> { None };
    let result = match_rules(&default_rules(), &slide("lyrics"), &none);
    assert!(result.target.is_none());
    assert_eq!(result.skipped[0].rule_id, "lyrics-worship");
}

#[test]
fn matching_is_case_insensitive_and_accepts_short_event_names() {
    let mut rules = default_rules();
    rules[0].when.event = "slide.changed".into();
    rules[0].when.content_kind = Some("LYRICS".into());
    let hit = match_rules(&rules, &slide("lyrics"), &lookup).target.unwrap();
    assert_eq!(hit.scene, "Worship");
}

#[test]
fn item_type_condition_must_match() {
    let mut rules = default_rules();
    rules[0].when.item_type = Some("song".into());
    assert!(match_rules(&rules, &slide("lyrics"), &lookup).target.is_none());
    let input = RuleInput {
        event: EVENT_SLIDE_CHANGED,
        content_kind: Some("lyrics"),
        item_type: Some("Song"),
    };
    assert!(match_rules(&rules, &input, &lookup).target.is_some());
}

#[test]
fn rule_input_reads_parsed_event() {
    let body = r#"{"event":"lumina.slide.changed","payload":{"liveContent":{"contentKind":"lyrics","itemType":"song"}}}"#;
    let ev = super::protocol::parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
    let input = RuleInput::from_event(&ev);
    assert_eq!(input.content_kind, Some("lyrics"));
    assert_eq!(input.item_type, Some("song"));
}

#[test]
fn validate_rules_cleans_and_rejects() {
    let mut rules = default_rules();
    rules[0].id = "  spaced  ".into();
    rules[0].when.content_kind = Some("   ".into());
    let cleaned = validate_rules(&rules).unwrap();
    assert_eq!(cleaned[0].id, "spaced");
    assert_eq!(cleaned[0].when.content_kind, None);

    let mut dup = default_rules();
    dup[1].id = dup[0].id.clone();
    assert!(validate_rules(&dup).unwrap_err().contains("duplicate"));

    let mut bad_scene = default_rules();
    bad_scene[0].then.load_scene = "../evil".into();
    assert!(validate_rules(&bad_scene).is_err());

    let mut bad_event = default_rules();
    bad_event[0].when.event = "".into();
    assert!(validate_rules(&bad_event).is_err());

    let too_many: Vec<Rule> = (0..=MAX_RULES)
        .map(|i| {
            let mut r = default_rules()[0].clone();
            r.id = format!("r{i}");
            r
        })
        .collect();
    assert!(validate_rules(&too_many).is_err());
}

#[test]
fn rules_round_trip_as_camel_case_json() {
    let json = serde_json::to_value(default_rules()).unwrap();
    assert_eq!(json[0]["when"]["contentKind"], "lyrics");
    assert_eq!(json[0]["then"]["loadScene"], "Worship");
    let parsed: Vec<Rule> = serde_json::from_value(serde_json::json!([
        {"id": "x", "when": {"event": "lumina.countdown.started"}, "then": {"loadScene": "Walk-in"}}
    ]))
    .unwrap();
    assert!(parsed[0].enabled);
    assert_eq!(parsed[0].when.content_kind, None);
}

#[test]
fn debouncer_fires_only_after_stable_period() {
    let mut d = Debouncer::new(DEBOUNCE_MS);
    assert_eq!(d.on_event(target("Worship"), 0), None);
    assert_eq!(d.tick(1_000), None);
    // More lyric slides keep the same target; the timer is not reset.
    assert_eq!(d.on_event(target("Worship"), 1_200), None);
    let fired = d.tick(1_500).unwrap();
    assert_eq!(fired.scene, "Worship");
    assert_eq!(d.current(), Some("Worship"));
    assert_eq!(d.tick(5_000), None);
}

#[test]
fn debouncer_flapping_slides_never_fire() {
    let mut d = Debouncer::new(DEBOUNCE_MS);
    let mut now = 0;
    for i in 0..20 {
        let scene = if i % 2 == 0 { "Worship" } else { "Sermon" };
        assert_eq!(d.on_event(target(scene), now), None, "flap {i}");
        now += 400;
        assert_eq!(d.tick(now), None);
    }
    // Then it settles on Sermon (last event was at now - 400).
    assert_eq!(d.on_event(target("Sermon"), now), None);
    assert_eq!(d.tick(now + DEBOUNCE_MS).unwrap().scene, "Sermon");
}

#[test]
fn debouncer_does_not_reapply_current_scene() {
    let mut d = Debouncer::new(DEBOUNCE_MS);
    d.set_current(Some("Worship".into()));
    assert_eq!(d.on_event(target("Worship"), 0), None);
    assert_eq!(d.tick(10_000), None);
    assert!(d.pending().is_none());

    // Going away and straight back cancels the change.
    assert_eq!(d.on_event(target("Sermon"), 10_000), None);
    assert_eq!(d.on_event(target("worship"), 10_500), None);
    assert_eq!(d.tick(20_000), None);
}

#[test]
fn debouncer_unmatched_content_cancels_pending() {
    let mut d = Debouncer::new(DEBOUNCE_MS);
    d.on_event(target("Sermon"), 0);
    assert_eq!(d.on_event(None, 500), None);
    assert_eq!(d.tick(5_000), None);
}

#[test]
fn debouncer_fires_immediately_with_zero_delay_and_tracks_manual_loads() {
    let mut d = Debouncer::new(0);
    assert_eq!(d.on_event(target("Sermon"), 7).unwrap().scene, "Sermon");
    d.on_event(target("Worship"), 8);
    assert_eq!(d.current(), Some("Worship"));

    let mut d = Debouncer::new(DEBOUNCE_MS);
    d.on_event(target("Sermon"), 0);
    // The operator loads Sermon by hand before the timer: nothing to do.
    d.set_current(Some("Sermon".into()));
    assert_eq!(d.tick(DEBOUNCE_MS), None);
    d.on_event(target("Worship"), 2_000);
    d.cancel();
    assert_eq!(d.tick(9_000), None);
}

#[test]
fn only_slide_and_item_events_cancel() {
    assert!(changes_live_content(EVENT_SLIDE_CHANGED));
    assert!(changes_live_content("lumina.item.started"));
    assert!(!changes_live_content("lumina.bridge.ping"));
    assert!(!changes_live_content("lumina.state.sync"));
}
