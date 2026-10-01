//! Lumina `lumina-aether` v1 webhook validation (pure: no I/O, no Tauri).
//!
//! Lumina's `services/aetherBridge.ts` POSTs one JSON envelope per event:
//! `{source, protocol, protocolVersion, event, sentAt, sequence, workspaceId,
//! sessionId, payload}` with `x-lumina-*` headers. Parsing is lenient about
//! missing payload fields (older Lumina builds send less), and strict only
//! about what would make the request meaningless: bad JSON, a different
//! protocol or version, or no usable event name.
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

pub const PROTOCOL: &str = "lumina-aether";
pub const PROTOCOL_VERSION: u64 = 1;
/// Longest string kept from any request field (longer ones are clipped).
pub const MAX_FIELD_CHARS: usize = 128;
/// A session that has been silent this long may restart its sequence.
pub const SEQUENCE_GAP_MS: u64 = 60_000;
/// Sessions remembered by `SequenceGuard` (oldest evicted first).
const MAX_SESSIONS: usize = 64;

pub const EVENT_PING: &str = "lumina.bridge.ping";
pub const EVENT_STATE_SYNC: &str = "lumina.state.sync";
pub const EVENT_SCENE_SWITCH: &str = "lumina.scene.switch";
pub const EVENT_SLIDE_CHANGED: &str = "lumina.slide.changed";
pub const EVENT_ITEM_STARTED: &str = "lumina.item.started";
pub const EVENT_COUNTDOWN_STARTED: &str = "lumina.countdown.started";
pub const EVENT_COUNTDOWN_ENDED: &str = "lumina.countdown.ended";
pub const EVENT_SERVICE_MODE_CHANGED: &str = "lumina.service.mode.changed";
pub const EVENT_STREAM_REQUEST: &str = "lumina.stream.request";
pub const EVENT_RECORDING_REQUEST: &str = "lumina.recording.request";

pub const KNOWN_EVENTS: [&str; 10] = [
    EVENT_PING,
    EVENT_STATE_SYNC,
    EVENT_SCENE_SWITCH,
    EVENT_SLIDE_CHANGED,
    EVENT_ITEM_STARTED,
    EVENT_COUNTDOWN_STARTED,
    EVENT_COUNTDOWN_ENDED,
    EVENT_SERVICE_MODE_CHANGED,
    EVENT_STREAM_REQUEST,
    EVENT_RECORDING_REQUEST,
];

pub fn is_known_event(event: &str) -> bool {
    KNOWN_EVENTS.contains(&event)
}

/// Lumina's `liveContent` snapshot. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveContent {
    pub mode: Option<String>,
    pub content_kind: Option<String>,
    pub strap_active: Option<bool>,
    pub item_id: Option<String>,
    pub item_type: Option<String>,
    pub slide_id: Option<String>,
    pub slide_index: Option<i64>,
    pub lyric_step: Option<i64>,
    pub changed_at: Option<String>,
}

impl LiveContent {
    fn is_empty(&self) -> bool {
        *self == LiveContent::default()
    }
}

/// A validated bridge request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuminaEvent {
    pub event: String,
    pub workspace_id: String,
    pub session_id: String,
    pub sequence: Option<u64>,
    pub sent_at: Option<String>,
    pub content: Option<LiveContent>,
    pub item_type: Option<String>,
}

impl LuminaEvent {
    pub fn content_kind(&self) -> Option<&str> {
        self.content.as_ref().and_then(|c| c.content_kind.as_deref())
    }
}

/// The `x-lumina-*` header values (the token is checked by the server).
#[derive(Debug, Clone, Default)]
pub struct BridgeHeaders {
    pub event: Option<String>,
    pub workspace: Option<String>,
    pub session: Option<String>,
    pub room: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    InvalidJson,
    NotAnObject,
    WrongProtocol,
    UnsupportedVersion,
    MissingEvent,
    EventMismatch,
    BadEventName,
}

impl ParseError {
    /// Client-facing message (never echoes request content).
    pub fn message(&self) -> &'static str {
        match self {
            ParseError::InvalidJson => "Body is not valid JSON.",
            ParseError::NotAnObject => "Body must be a JSON object.",
            ParseError::WrongProtocol => "protocol must be 'lumina-aether'.",
            ParseError::UnsupportedVersion => "Unsupported protocolVersion (expected 1).",
            ParseError::MissingEvent => "Missing event name.",
            ParseError::EventMismatch => "x-lumina-event does not match the body event.",
            ParseError::BadEventName => "Event name is not valid.",
        }
    }
}

/// Parse and validate one bridge request.
pub fn parse_request(headers: &BridgeHeaders, body: &[u8]) -> Result<LuminaEvent, ParseError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ParseError::InvalidJson)?;
    let obj = value.as_object().ok_or(ParseError::NotAnObject)?;

    if let Some(protocol) = obj.get("protocol") {
        if protocol.as_str() != Some(PROTOCOL) {
            return Err(ParseError::WrongProtocol);
        }
    }
    if let Some(version) = obj.get("protocolVersion") {
        if version.as_u64() != Some(PROTOCOL_VERSION) {
            return Err(ParseError::UnsupportedVersion);
        }
    }

    let event = resolve_event(obj.get("event"), headers.event.as_deref())?;
    let workspace_id = text(obj.get("workspaceId"))
        .or_else(|| clip_opt(headers.workspace.as_deref()))
        .unwrap_or_default();
    let session_id = text(obj.get("sessionId"))
        .or_else(|| clip_opt(headers.session.as_deref()))
        .unwrap_or_default();

    let payload = obj.get("payload").and_then(Value::as_object);
    let content = payload.and_then(parse_content);
    let item_type = content
        .as_ref()
        .and_then(|c| c.item_type.clone())
        .or_else(|| payload.and_then(payload_item_type));

    Ok(LuminaEvent {
        event,
        workspace_id,
        session_id,
        sequence: obj.get("sequence").and_then(as_sequence),
        sent_at: text(obj.get("sentAt")),
        content,
        item_type,
    })
}

fn resolve_event(body: Option<&Value>, header: Option<&str>) -> Result<String, ParseError> {
    let from_body = match body {
        Some(Value::String(s)) => Some(s.trim().to_string()),
        Some(Value::Null) | None => None,
        Some(_) => return Err(ParseError::BadEventName),
    };
    let from_header = header.map(|h| h.trim().to_string()).filter(|h| !h.is_empty());
    let event = match (from_body, from_header) {
        (Some(b), Some(h)) if b != h => return Err(ParseError::EventMismatch),
        (Some(b), _) => b,
        (None, Some(h)) => h,
        (None, None) => return Err(ParseError::MissingEvent),
    };
    if event.is_empty() {
        return Err(ParseError::MissingEvent);
    }
    if !is_valid_event_name(&event) {
        return Err(ParseError::BadEventName);
    }
    Ok(event)
}

/// Event names are short dotted identifiers: `[a-zA-Z0-9._-]`, ≤ 128 chars.
pub fn is_valid_event_name(event: &str) -> bool {
    !event.is_empty()
        && event.len() <= MAX_FIELD_CHARS
        && event
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// `payload.liveContent` when it is an object, else the flat payload when it
/// carries any live-content field. `None` when nothing useful is present.
fn parse_content(payload: &Map<String, Value>) -> Option<LiveContent> {
    let source = match payload.get("liveContent") {
        Some(Value::Object(inner)) => inner,
        _ => payload,
    };
    let content = LiveContent {
        mode: text(source.get("mode")),
        content_kind: text(source.get("contentKind")),
        strap_active: source.get("strapActive").and_then(Value::as_bool),
        item_id: text(source.get("itemId")),
        item_type: text(source.get("itemType")),
        slide_id: text(source.get("slideId")),
        slide_index: source.get("slideIndex").and_then(as_int),
        lyric_step: source.get("lyricStep").and_then(as_int),
        changed_at: text(source.get("changedAt")),
    };
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

/// `payload.itemType`, else `payload.item.type` / `payload.item.itemType`.
fn payload_item_type(payload: &Map<String, Value>) -> Option<String> {
    text(payload.get("itemType")).or_else(|| {
        let item = payload.get("item")?.as_object()?;
        text(item.get("type")).or_else(|| text(item.get("itemType")))
    })
}

/// A string (trimmed, clipped) or a number rendered as text.
fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => clip_opt(Some(s.as_str())),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn clip_opt(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_FIELD_CHARS).collect())
}

fn as_int(value: &Value) -> Option<i64> {
    if let Some(i) = value.as_i64() {
        return Some(i);
    }
    let f = value.as_f64()?;
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
        Some(f as i64)
    } else {
        None
    }
}

fn as_sequence(value: &Value) -> Option<u64> {
    as_int(value).and_then(|i| u64::try_from(i).ok())
}

/// Result of `SequenceGuard::check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceVerdict {
    Accept,
    Stale { last: u64 },
}

/// Drops out-of-order announcements per Lumina session. Lumina's sequence is
/// monotonic per session: anything ≤ the last accepted value is stale, unless
/// the session has been silent for `SEQUENCE_GAP_MS` (Lumina restarted with
/// the same session id). A request without a sequence is always accepted.
#[derive(Debug, Default)]
pub struct SequenceGuard {
    /// session id → (last accepted sequence, accepted at, ms)
    sessions: HashMap<String, (u64, u64)>,
}

impl SequenceGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&mut self, session: &str, sequence: Option<u64>, now_ms: u64) -> SequenceVerdict {
        let seq = match sequence {
            Some(seq) => seq,
            None => return SequenceVerdict::Accept,
        };
        if let Some(&(last, at)) = self.sessions.get(session) {
            if seq <= last && now_ms.saturating_sub(at) < SEQUENCE_GAP_MS {
                return SequenceVerdict::Stale { last };
            }
        } else if self.sessions.len() >= MAX_SESSIONS {
            self.evict_oldest();
        }
        self.sessions.insert(session.to_string(), (seq, now_ms));
        SequenceVerdict::Accept
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .sessions
            .iter()
            .min_by_key(|entry| (entry.1).1)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            self.sessions.remove(&key);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.sessions.len()
    }
}
