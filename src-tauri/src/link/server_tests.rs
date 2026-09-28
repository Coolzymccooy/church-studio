//! Integration tests: the real Link server on an ephemeral loopback port,
//! with a test `LinkBackend` (no Tauri `AppHandle`), driven by raw HTTP/1.1.
use super::protocol::LuminaEvent;
use super::server::{bind_loopback, router, serve, LinkBackend, StatusInfo, BRIDGE_PATH};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct TestBackend {
    events: Mutex<Vec<LuminaEvent>>,
    enabled: AtomicBool,
}

impl LinkBackend for TestBackend {
    fn token(&self) -> String {
        TOKEN.to_string()
    }

    fn accepting(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    fn handle_event(&self, event: LuminaEvent) -> String {
        self.events.lock().unwrap().push(event);
        "Event accepted.".to_string()
    }

    fn status(&self) -> StatusInfo {
        StatusInfo {
            app: "tiwaton-ai-studio".to_string(),
            version: "test".to_string(),
            engine_running: false,
            current_scene: Some("Worship".to_string()),
            bus_loudness: None,
        }
    }
}

struct Harness {
    port: u16,
    backend: Arc<TestBackend>,
    _shutdown: oneshot::Sender<()>,
}

fn start() -> Harness {
    let backend = Arc::new(TestBackend {
        events: Mutex::new(Vec::new()),
        enabled: AtomicBool::new(true),
    });
    let listener = bind_loopback(0, 0).expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = oneshot::channel();
    let dyn_backend: Arc<dyn LinkBackend> = backend.clone();
    tokio::spawn(serve(listener, router(dyn_backend), rx));
    Harness {
        port,
        backend,
        _shutdown: tx,
    }
}

/// Send one raw request (`Connection: close`) and return (status, full text).
async fn send(port: u16, raw: String) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.expect("connect");
    stream.write_all(raw.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    (status, text)
}

fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    raw
}

fn slide_body(sequence: u64) -> String {
    format!(
        r#"{{"source":"lumina-presenter","version":"1.0","protocol":"lumina-aether","protocolVersion":1,
"event":"lumina.slide.changed","sentAt":"2026-09-28T10:00:00.000Z","sequence":{sequence},
"workspaceId":"ws-1","sessionId":"sess-1",
"payload":{{"liveContent":{{"mode":"overlay","contentKind":"lyrics","strapActive":false,
"itemId":"i1","itemType":"song","slideId":"s1","slideIndex":0,"lyricStep":0,
"changedAt":"2026-09-28T10:00:00.000Z"}}}}}}"#
    )
}

fn lumina_headers(token: Option<&str>) -> Vec<(&str, &str)> {
    let mut headers = vec![
        ("Content-Type", "application/json"),
        ("x-lumina-event", "lumina.slide.changed"),
        ("x-lumina-workspace", "ws-1"),
        ("x-lumina-session", "sess-1"),
    ];
    if let Some(token) = token {
        headers.push(("x-lumina-token", token));
    }
    headers
}

#[tokio::test]
async fn valid_bridge_request_is_accepted_and_parsed() {
    let h = start();
    let raw = request("POST", BRIDGE_PATH, &lumina_headers(Some(TOKEN)), &slide_body(1));
    let (status, text) = send(h.port, raw).await;
    assert_eq!(status, 202, "{text}");
    assert!(text.contains("\"ok\":true"), "{text}");
    assert!(text.to_ascii_lowercase().contains("access-control-allow-origin: *"));
    let events = h.backend.events.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "lumina.slide.changed");
    assert_eq!(events[0].session_id, "sess-1");
    assert_eq!(events[0].sequence, Some(1));
    assert_eq!(events[0].content_kind(), Some("lyrics"));
    assert_eq!(events[0].item_type.as_deref(), Some("song"));
}

#[tokio::test]
async fn missing_or_wrong_token_is_401() {
    let h = start();
    let (status, _) = send(
        h.port,
        request("POST", BRIDGE_PATH, &lumina_headers(None), &slide_body(1)),
    )
    .await;
    assert_eq!(status, 401);
    let wrong = "f".repeat(64);
    let (status, text) = send(
        h.port,
        request("POST", BRIDGE_PATH, &lumina_headers(Some(wrong.as_str())), &slide_body(2)),
    )
    .await;
    assert_eq!(status, 401);
    assert!(!text.contains(TOKEN), "the token must never be echoed");
    assert!(h.backend.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bearer_token_is_accepted() {
    let h = start();
    let bearer = format!("Bearer {TOKEN}");
    let mut headers = lumina_headers(None);
    headers.push(("Authorization", bearer.as_str()));
    let (status, _) = send(h.port, request("POST", BRIDGE_PATH, &headers, &slide_body(1))).await;
    assert_eq!(status, 202);
}

#[tokio::test]
async fn bad_json_is_400() {
    let h = start();
    let (status, text) = send(
        h.port,
        request("POST", BRIDGE_PATH, &lumina_headers(Some(TOKEN)), "{not json"),
    )
    .await;
    assert_eq!(status, 400, "{text}");
    assert!(h.backend.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn health_needs_no_token() {
    let h = start();
    let (status, text) = send(h.port, request("GET", "/api/health", &[], "")).await;
    assert_eq!(status, 200);
    assert!(text.contains("tiwaton-ai-studio"));
}

#[tokio::test]
async fn status_requires_token_and_has_no_secrets() {
    let h = start();
    let (status, _) = send(h.port, request("GET", "/api/status", &[], "")).await;
    assert_eq!(status, 401);
    let (status, text) = send(
        h.port,
        request("GET", "/api/status", &[("x-lumina-token", TOKEN)], ""),
    )
    .await;
    assert_eq!(status, 200);
    assert!(text.contains("\"currentScene\":\"Worship\""), "{text}");
    assert!(text.contains("\"engineRunning\":false"));
    assert!(!text.contains(TOKEN));
}

#[tokio::test]
async fn stale_sequence_is_ignored_with_202() {
    let h = start();
    let headers = lumina_headers(Some(TOKEN));
    let (first, _) = send(h.port, request("POST", BRIDGE_PATH, &headers, &slide_body(5))).await;
    let (stale, text) = send(h.port, request("POST", BRIDGE_PATH, &headers, &slide_body(4))).await;
    assert_eq!(first, 202);
    assert_eq!(stale, 202);
    assert!(text.contains("out-of-order"), "{text}");
    assert_eq!(h.backend.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn disabled_backend_is_503() {
    let h = start();
    h.backend.enabled.store(false, Ordering::SeqCst);
    let raw = request("POST", BRIDGE_PATH, &lumina_headers(Some(TOKEN)), &slide_body(1));
    let (status, _) = send(h.port, raw).await;
    assert_eq!(status, 503);
}

#[tokio::test]
async fn cors_preflight_is_answered_without_token() {
    let h = start();
    let headers = [
        ("Origin", "http://localhost:5173"),
        ("Access-Control-Request-Method", "POST"),
        ("Access-Control-Request-Headers", "content-type,x-lumina-token"),
    ];
    let (status, text) = send(h.port, request("OPTIONS", BRIDGE_PATH, &headers, "")).await;
    assert_eq!(status, 204, "{text}");
    let lower = text.to_ascii_lowercase();
    assert!(lower.contains("access-control-allow-headers:"));
    assert!(lower.contains("x-lumina-token"));
    assert!(lower.contains("access-control-allow-private-network: true"));
}

#[tokio::test]
async fn binds_only_loopback() {
    let listener = bind_loopback(0, 0).unwrap();
    assert!(listener.local_addr().unwrap().ip().is_loopback());
}
