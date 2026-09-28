//! The Tiwaton Link HTTP server (integrations design, decision 7).
//!
//! Loopback only (`127.0.0.1`, port 4460 or the next free one up to 4469).
//! Routes:
//! - `POST /api/lumina/bridge`: Lumina's `lumina-aether` v1 webhook (token).
//! - `GET /api/status`: app, version, engine state, current scene (token).
//! - `GET /api/health`: liveness, no token, no CORS headers.
//!
//! Every request must carry `Host: 127.0.0.1:<port>` or `localhost:<port>`
//! (403 otherwise), which blocks DNS-rebinding pages.
//!
//! Lumina sends from a browser context, so the bridge and status routes also
//! answer CORS preflights. That is safe because every request still needs
//! the token, and no cookies or ambient credentials are involved.
//!
//! The router depends only on the `LinkBackend` trait, so tests build it
//! without a Tauri `AppHandle`.
use crate::link::config::constant_time_eq;
use crate::link::protocol::{self, BridgeHeaders, LuminaEvent, SequenceGuard, SequenceVerdict};
use crate::link::lock;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::middleware::Next;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use std::io;
use std::net::{Ipv4Addr, TcpListener as StdTcpListener};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::oneshot;

pub const FIRST_PORT: u16 = 4460;
pub const LAST_PORT: u16 = 4469;
pub const BODY_LIMIT_BYTES: usize = 64 * 1024;
pub const APP_ID: &str = "tiwaton-ai-studio";
pub const BRIDGE_PATH: &str = "/api/lumina/bridge";

const ALLOW_HEADERS: &str = "content-type, authorization, x-lumina-event, x-lumina-workspace, \
x-lumina-session, x-lumina-token, x-lumina-room";

/// `GET /api/status` body. Contains no secrets.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub app: String,
    pub version: String,
    pub engine_running: bool,
    pub current_scene: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_loudness: Option<serde_json::Value>,
}

/// What the server needs from the app.
pub trait LinkBackend: Send + Sync + 'static {
    /// The expected token (empty means "reject everything").
    fn token(&self) -> String;
    /// False while Link is disabled: requests get 503.
    fn accepting(&self) -> bool;
    /// Handle one validated, in-order event. Returns a short message for
    /// Lumina's bridge log (no secrets).
    fn handle_event(&self, event: LuminaEvent) -> String;
    fn status(&self) -> StatusInfo;
}

pub struct ServerCtx {
    backend: Arc<dyn LinkBackend>,
    sequences: Mutex<SequenceGuard>,
    started: Instant,
}

impl ServerCtx {
    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// `port` is the bound port: only `127.0.0.1:<port>` and `localhost:<port>`
/// are accepted as `Host` (DNS-rebinding defence in depth). Only the bridge
/// and status routes carry CORS headers; `/api/health` does not, so other
/// web pages cannot read it.
pub fn router(backend: Arc<dyn LinkBackend>, port: u16) -> Router {
    let ctx = Arc::new(ServerCtx {
        backend,
        sequences: Mutex::new(SequenceGuard::new()),
        started: Instant::now(),
    });
    let hosts = Arc::new(allowed_hosts(port));
    let api = Router::new()
        .route("/api/status", get(status).options(preflight))
        .route(BRIDGE_PATH, post(bridge).options(preflight))
        .layer(axum::middleware::map_response(add_cors));
    Router::new()
        .route("/api/health", get(health))
        .merge(api)
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(axum::middleware::from_fn_with_state(hosts, check_host))
        .with_state(ctx)
}

/// The `Host` values a loopback client on `port` sends.
pub fn allowed_hosts(port: u16) -> Vec<String> {
    vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")]
}

/// True when `host` (a raw `Host` header value) is one of `allowed`.
pub fn is_allowed_host(host: Option<&str>, allowed: &[String]) -> bool {
    match host {
        Some(host) => {
            let host = host.trim();
            allowed.iter().any(|a| a.eq_ignore_ascii_case(host))
        }
        None => false,
    }
}

/// Runs before routing (so before preflight, token or body handling).
async fn check_host(
    State(allowed): State<Arc<Vec<String>>>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok());
    if !is_allowed_host(host, &allowed) {
        return message(StatusCode::FORBIDDEN, false, "Host not allowed.");
    }
    next.run(request).await
}

fn message(status: StatusCode, ok: bool, text: &str) -> Response {
    let body = serde_json::json!({ "ok": ok, "message": text });
    (status, Json(body)).into_response()
}

async fn add_cors(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
}

async fn preflight() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(ALLOW_HEADERS),
    );
    headers.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("600"));
    // Chrome's Private Network Access preflight for requests to loopback.
    headers.insert(
        "access-control-allow-private-network",
        HeaderValue::from_static("true"),
    );
    response
}

async fn health() -> Response {
    let body = serde_json::json!({ "ok": true, "app": APP_ID });
    (StatusCode::OK, Json(body)).into_response()
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// `x-lumina-token`, else `Authorization: Bearer <token>`.
pub fn presented_token(headers: &HeaderMap) -> Option<String> {
    if let Some(token) = header_text(headers, "x-lumina-token") {
        return Some(token);
    }
    let auth = header_text(headers, header::AUTHORIZATION.as_str())?;
    let (scheme, rest) = auth.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !rest.trim().is_empty() {
        Some(rest.trim().to_string())
    } else {
        None
    }
}

pub fn is_authorized(headers: &HeaderMap, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    match presented_token(headers) {
        Some(token) => constant_time_eq(token.as_bytes(), expected.as_bytes()),
        None => false,
    }
}

/// Shared gate for the token-protected routes.
fn gate(ctx: &ServerCtx, headers: &HeaderMap) -> Option<Response> {
    if !ctx.backend.accepting() {
        return Some(message(
            StatusCode::SERVICE_UNAVAILABLE,
            false,
            "Tiwaton Link is disabled in the Studio.",
        ));
    }
    if !is_authorized(headers, &ctx.backend.token()) {
        return Some(message(
            StatusCode::UNAUTHORIZED,
            false,
            "Missing or invalid Tiwaton Link token.",
        ));
    }
    None
}

async fn status(State(ctx): State<Arc<ServerCtx>>, headers: HeaderMap) -> Response {
    if let Some(denied) = gate(&ctx, &headers) {
        return denied;
    }
    (StatusCode::OK, Json(ctx.backend.status())).into_response()
}

async fn bridge(State(ctx): State<Arc<ServerCtx>>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(denied) = gate(&ctx, &headers) {
        return denied;
    }
    let bridge_headers = BridgeHeaders {
        event: header_text(&headers, "x-lumina-event"),
        workspace: header_text(&headers, "x-lumina-workspace"),
        session: header_text(&headers, "x-lumina-session"),
        room: header_text(&headers, "x-lumina-room"),
    };
    let event = match protocol::parse_request(&bridge_headers, &body) {
        Ok(event) => event,
        Err(err) => return message(StatusCode::BAD_REQUEST, false, err.message()),
    };
    let verdict = lock(&ctx.sequences).check(&event.session_id, event.sequence, ctx.elapsed_ms());
    if let SequenceVerdict::Stale { .. } = verdict {
        return message(StatusCode::ACCEPTED, true, "Ignored: out-of-order sequence.");
    }
    let text = ctx.backend.handle_event(event);
    message(StatusCode::ACCEPTED, true, &text)
}

/// Bind the first free loopback port in `first..=last` (pass 0..=0 for an
/// OS-assigned port). The listener is non-blocking, ready for tokio.
pub fn bind_loopback(first: u16, last: u16) -> io::Result<StdTcpListener> {
    let mut last_err = None;
    for port in first..=last {
        match StdTcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => {
                listener.set_nonblocking(true)?;
                return Ok(listener);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::AddrInUse, "no free port")))
}

/// Serve `app` on `listener` until `shutdown` fires (or its sender drops).
/// Must be polled inside a tokio runtime.
pub async fn serve(listener: StdTcpListener, app: Router, shutdown: oneshot::Receiver<()>) {
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(e) => {
            log::error!("link: cannot start the server: {e}");
            return;
        }
    };
    let signal = async move {
        let _ = shutdown.await;
    };
    if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(signal).await {
        log::error!("link: server stopped with an error: {e}");
    }
}

/// A server running on Tauri's async runtime. Dropping it stops the server.
pub struct RunningServer {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

impl RunningServer {
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Bind 127.0.0.1:4460..=4469 and serve `backend` on Tauri's runtime.
pub fn start(backend: Arc<dyn LinkBackend>) -> io::Result<RunningServer> {
    let listener = bind_loopback(FIRST_PORT, LAST_PORT)?;
    let port = listener.local_addr()?.port();
    let (tx, rx) = oneshot::channel();
    let app = router(backend, port);
    tauri::async_runtime::spawn(serve(listener, app, rx));
    log::info!("link: listening on 127.0.0.1:{port}");
    Ok(RunningServer {
        port,
        shutdown: Some(tx),
    })
}
