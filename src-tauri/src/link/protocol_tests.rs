//! Unit tests for `link::protocol`, using fixtures shaped like the requests
//! Lumina's `services/aetherBridge.ts` sends.
use super::protocol::*;

fn headers(event: &str) -> BridgeHeaders {
    BridgeHeaders {
        event: Some(event.to_string()),
        workspace: Some("ws-header".to_string()),
        session: Some("sess-header".to_string()),
        room: None,
    }
}

const SLIDE_CHANGED: &str = r#"{
  "source": "lumina-presenter",
  "version": "1.0",
  "protocol": "lumina-aether",
  "protocolVersion": 1,
  "event": "lumina.slide.changed",
  "sentAt": "2026-09-28T10:15:00.000Z",
  "sequence": 42,
  "workspaceId": "ws-1",
  "sessionId": "sess-1",
  "payload": {
    "liveContent": {
      "mode": "overlay",
      "contentKind": "lyrics",
      "strapActive": true,
      "itemId": "item-7",
      "itemType": "song",
      "slideId": "slide-3",
      "slideIndex": 2,
      "lyricStep": 1,
      "changedAt": "2026-09-28T10:15:00.000Z"
    }
  }
}"#;

#[test]
fn parses_full_slide_changed_fixture() {
    let ev = parse_request(&headers("lumina.slide.changed"), SLIDE_CHANGED.as_bytes()).unwrap();
    assert_eq!(ev.event, "lumina.slide.changed");
    assert_eq!(ev.workspace_id, "ws-1");
    assert_eq!(ev.session_id, "sess-1");
    assert_eq!(ev.sequence, Some(42));
    assert_eq!(ev.sent_at.as_deref(), Some("2026-09-28T10:15:00.000Z"));
    assert_eq!(ev.content_kind(), Some("lyrics"));
    assert_eq!(ev.item_type.as_deref(), Some("song"));
    let content = ev.content.unwrap();
    assert_eq!(content.mode.as_deref(), Some("overlay"));
    assert_eq!(content.strap_active, Some(true));
    assert_eq!(content.item_id.as_deref(), Some("item-7"));
    assert_eq!(content.slide_id.as_deref(), Some("slide-3"));
    assert_eq!(content.slide_index, Some(2));
    assert_eq!(content.lyric_step, Some(1));
}

#[test]
fn accepts_flat_payload_without_live_content_wrapper() {
    let body = r#"{"protocol":"lumina-aether","protocolVersion":1,
        "event":"lumina.slide.changed","sequence":1,"workspaceId":"w","sessionId":"s",
        "payload":{"mode":"full-frame","contentKind":"scripture","slideIndex":0}}"#;
    let ev = parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
    assert_eq!(ev.content_kind(), Some("scripture"));
    assert_eq!(ev.content.unwrap().mode.as_deref(), Some("full-frame"));
}

#[test]
fn missing_payload_fields_are_tolerated() {
    let body = r#"{"protocol":"lumina-aether","event":"lumina.slide.changed","payload":{}}"#;
    let ev = parse_request(&headers("lumina.slide.changed"), body.as_bytes()).unwrap();
    assert!(ev.content.is_none());
    assert_eq!(ev.sequence, None);
    // Falls back to the header ids when the body lacks them.
    assert_eq!(ev.workspace_id, "ws-header");
    assert_eq!(ev.session_id, "sess-header");
}

#[test]
fn countdown_started_has_no_content() {
    let body = r#"{"protocol":"lumina-aether","protocolVersion":1,
        "event":"lumina.countdown.started","sequence":5,"workspaceId":"w","sessionId":"s",
        "payload":{"durationSec":300,"label":"Service starts"}}"#;
    let ev = parse_request(&headers("lumina.countdown.started"), body.as_bytes()).unwrap();
    assert_eq!(ev.event, EVENT_COUNTDOWN_STARTED);
    assert!(ev.content.is_none());
    assert!(is_known_event(&ev.event));
}

#[test]
fn item_started_reads_item_type_from_payload() {
    let body = r#"{"protocol":"lumina-aether","event":"lumina.item.started",
        "payload":{"item":{"id":"i1","type":"sermon"}}}"#;
    let ev = parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
    assert_eq!(ev.item_type.as_deref(), Some("sermon"));

    let flat = r#"{"event":"lumina.item.started","payload":{"itemType":"song"}}"#;
    let ev = parse_request(&BridgeHeaders::default(), flat.as_bytes()).unwrap();
    assert_eq!(ev.item_type.as_deref(), Some("song"));
}

#[test]
fn event_can_come_from_header_only() {
    let body = r#"{"protocol":"lumina-aether","payload":{}}"#;
    let ev = parse_request(&headers("lumina.bridge.ping"), body.as_bytes()).unwrap();
    assert_eq!(ev.event, EVENT_PING);
}

#[test]
fn unknown_event_parses_but_is_not_known() {
    let body = r#"{"protocol":"lumina-aether","event":"lumina.future.thing","payload":{}}"#;
    let ev = parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
    assert!(!is_known_event(&ev.event));
}

#[test]
fn rejects_bad_json_and_non_objects() {
    let h = BridgeHeaders::default();
    assert_eq!(parse_request(&h, b"{not json").unwrap_err(), ParseError::InvalidJson);
    assert_eq!(parse_request(&h, b"").unwrap_err(), ParseError::InvalidJson);
    assert_eq!(parse_request(&h, b"[1,2]").unwrap_err(), ParseError::NotAnObject);
}

#[test]
fn rejects_wrong_protocol_and_version() {
    let h = BridgeHeaders::default();
    let wrong = r#"{"protocol":"obs","event":"lumina.bridge.ping"}"#;
    assert_eq!(parse_request(&h, wrong.as_bytes()).unwrap_err(), ParseError::WrongProtocol);
    let v2 = r#"{"protocol":"lumina-aether","protocolVersion":2,"event":"lumina.bridge.ping"}"#;
    assert_eq!(parse_request(&h, v2.as_bytes()).unwrap_err(), ParseError::UnsupportedVersion);
}

#[test]
fn rejects_missing_mismatched_and_bad_event_names() {
    let none = r#"{"protocol":"lumina-aether","payload":{}}"#;
    assert_eq!(
        parse_request(&BridgeHeaders::default(), none.as_bytes()).unwrap_err(),
        ParseError::MissingEvent
    );
    let body = r#"{"event":"lumina.slide.changed"}"#;
    assert_eq!(
        parse_request(&headers("lumina.bridge.ping"), body.as_bytes()).unwrap_err(),
        ParseError::EventMismatch
    );
    let bad = r#"{"event":"lumina/../etc"}"#;
    assert_eq!(
        parse_request(&BridgeHeaders::default(), bad.as_bytes()).unwrap_err(),
        ParseError::BadEventName
    );
    let numeric = r#"{"event":12}"#;
    assert_eq!(
        parse_request(&BridgeHeaders::default(), numeric.as_bytes()).unwrap_err(),
        ParseError::BadEventName
    );
}

#[test]
fn long_fields_are_clipped() {
    let long = "x".repeat(500);
    let body = format!(r#"{{"event":"lumina.bridge.ping","sessionId":"{long}"}}"#);
    let ev = parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
    assert_eq!(ev.session_id.chars().count(), MAX_FIELD_CHARS);
}

#[test]
fn odd_sequence_values_are_ignored_not_fatal() {
    for seq in ["-3", "\"7\"", "1.5", "null"] {
        let body = format!(r#"{{"event":"lumina.bridge.ping","sequence":{seq}}}"#);
        let ev = parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
        assert_eq!(ev.sequence, None, "sequence {seq}");
    }
    let body = r#"{"event":"lumina.bridge.ping","sequence":7.0}"#;
    let ev = parse_request(&BridgeHeaders::default(), body.as_bytes()).unwrap();
    assert_eq!(ev.sequence, Some(7));
}

#[test]
fn sequence_guard_drops_stale_and_duplicate() {
    let mut g = SequenceGuard::new();
    assert_eq!(g.check("s", Some(5), 0), SequenceVerdict::Accept);
    assert_eq!(g.check("s", Some(5), 10), SequenceVerdict::Stale { last: 5 });
    assert_eq!(g.check("s", Some(3), 20), SequenceVerdict::Stale { last: 5 });
    assert_eq!(g.check("s", Some(6), 30), SequenceVerdict::Accept);
}

#[test]
fn sequence_guard_is_per_session() {
    let mut g = SequenceGuard::new();
    assert_eq!(g.check("a", Some(10), 0), SequenceVerdict::Accept);
    // A new session (Lumina restarted) starts again from 1.
    assert_eq!(g.check("b", Some(1), 5), SequenceVerdict::Accept);
    assert_eq!(g.check("a", Some(9), 6), SequenceVerdict::Stale { last: 10 });
}

#[test]
fn sequence_guard_accepts_reset_after_gap() {
    let mut g = SequenceGuard::new();
    assert_eq!(g.check("s", Some(100), 1_000), SequenceVerdict::Accept);
    assert_eq!(
        g.check("s", Some(1), 1_000 + SEQUENCE_GAP_MS - 1),
        SequenceVerdict::Stale { last: 100 }
    );
    assert_eq!(g.check("s", Some(1), 1_000 + SEQUENCE_GAP_MS), SequenceVerdict::Accept);
    assert_eq!(g.check("s", Some(2), 1_000 + SEQUENCE_GAP_MS + 1), SequenceVerdict::Accept);
}

#[test]
fn sequence_guard_accepts_missing_sequence() {
    let mut g = SequenceGuard::new();
    assert_eq!(g.check("s", Some(5), 0), SequenceVerdict::Accept);
    assert_eq!(g.check("s", None, 1), SequenceVerdict::Accept);
}

#[test]
fn sequence_guard_bounds_memory() {
    let mut g = SequenceGuard::new();
    for i in 0..200u64 {
        g.check(&format!("session-{i}"), Some(1), i);
    }
    assert!(g.len() <= 64);
    // The newest session is still tracked.
    assert_eq!(g.check("session-199", Some(1), 300), SequenceVerdict::Stale { last: 1 });
}
