mod auth;
pub mod desktop;
mod extensions;
mod settings;
mod tls;
mod updates;
mod web;

pub use auth::Auth;
pub use axum::http::StatusCode as Status;

use auth::PairError;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Monitor1,
    Monitor2,
    Monitor3,
    Desk,
}

impl Role {
    fn monitor_key(self) -> Option<&'static str> {
        match self {
            Self::Monitor1 => Some("send_monitor_db"),
            Self::Monitor2 => Some("send_monitor2_db"),
            Self::Monitor3 => Some("send_monitor3_db"),
            Self::Desk => None,
        }
    }

    fn monitor_bus(self) -> Option<&'static str> {
        match self {
            Self::Monitor1 => Some("monitor"),
            Self::Monitor2 => Some("monitor2"),
            Self::Monitor3 => Some("monitor3"),
            Self::Desk => None,
        }
    }

    fn permits(self, change: &Control) -> bool {
        match (&change.target, &change.change) {
            (Target::Personal, Change::Mute { .. }) => self.monitor_bus().is_some(),
            (Target::Personal, _) => false,
            (Target::Strip { .. }, Change::Level { .. }) => true,
            _ => self == Self::Desk,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Target {
    Strip { index: u32 },
    Bus { id: String },
    Return { id: String },
    Dca { index: u32 },
    Personal,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Change {
    Level { db: f32 },
    Mute { muted: bool },
}

#[derive(Deserialize)]
pub struct Control {
    pub layout: String,
    pub target: Target,
    #[serde(flatten)]
    pub change: Change,
}

#[derive(Serialize)]
pub struct ViewControl {
    pub target: Target,
    pub name: String,
    pub db: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

#[derive(Serialize)]
pub struct MixerView {
    pub role: Role,
    pub running: bool,
    pub layout: String,
    pub controls: Vec<ViewControl>,
}

pub trait Board: Send + Sync + 'static {
    fn view(&self, role: Role) -> Result<MixerView, Status>;
    fn apply(&self, role: Role, control: Control) -> Result<(), Status>;
    fn notify(&self) {}
}

struct Context {
    board: Arc<dyn Board>,
    auth: Arc<Mutex<Auth>>,
    hosts: [String; 1],
    origin: String,
    fingerprint: String,
    jobs: Arc<Semaphore>,
    dirty: AtomicBool,
    updates: updates::Channels,
}

#[cfg(test)]
pub fn router(board: Arc<dyn Board>, auth: Arc<Mutex<Auth>>, host: &str) -> Router {
    router_with_identity(board, auth, host, String::new())
}

fn router_with_identity(
    board: Arc<dyn Board>,
    auth: Arc<Mutex<Auth>>,
    host: &str,
    fingerprint: String,
) -> Router {
    let ctx = Arc::new(Context {
        board,
        auth,
        hosts: [host.into()],
        origin: format!("https://{host}"),
        fingerprint,
        jobs: Arc::new(Semaphore::new(8)),
        dirty: AtomicBool::new(false),
        updates: updates::Channels::default(),
    });
    tokio::spawn(updates::publish(Arc::downgrade(&ctx)));
    Router::new()
        .route("/", get(web::index))
        .route("/desk.css", get(web::css))
        .route("/desk.js", get(web::js))
        .route("/protocol.js", get(web::protocol))
        .route("/font.woff2", get(web::font))
        .route("/mono.woff2", get(web::mono))
        .route("/api/pair", post(pair))
        .route("/api/state", get(state))
        .route("/api/events", get(updates::events))
        .route("/api/renew", post(renew))
        .route("/api/identity", get(identity))
        .route("/api/control", post(control))
        .layer(DefaultBodyLimit::max(1024))
        .layer(middleware::from_fn_with_state(ctx.clone(), checked_request))
        .with_state(ctx)
}

fn one_header(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    let mut all = headers.get_all(name).iter();
    let value = all.next()?.to_str().ok()?;
    all.next().is_none().then_some(value)
}

async fn checked_request(
    State(ctx): State<Arc<Context>>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    if !crate::link::server::is_allowed_host(one_header(headers, header::HOST), &ctx.hosts) {
        return Status::FORBIDDEN.into_response();
    }
    if request.method() != Method::GET
        && request.method() != Method::HEAD
        && one_header(headers, header::ORIGIN) != Some(ctx.origin.as_str())
    {
        return Status::FORBIDDEN.into_response();
    }

    let mut response = match tokio::time::timeout(Duration::from_secs(2), next.run(request)).await {
        Ok(response) => response,
        Err(_) => Status::REQUEST_TIMEOUT.into_response(),
    };
    let headers = response.headers_mut();
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        ("content-security-policy", "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairRequest {
    code: String,
    name: String,
    #[serde(default)]
    renew: bool,
}

fn client_ip(peer: Option<Extension<IpAddr>>) -> IpAddr {
    peer.map_or(Ipv4Addr::LOCALHOST.into(), |Extension(ip)| ip)
}

fn cookie_response(cookie: String) -> Response {
    match HeaderValue::from_str(&cookie) {
        Ok(cookie) => (Status::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response(),
        Err(_) => Status::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn pair(
    State(ctx): State<Arc<Context>>,
    peer: Option<Extension<IpAddr>>,
    Json(body): Json<PairRequest>,
) -> Response {
    let result = ctx
        .auth
        .lock()
        .pair(&body.code, &body.name, client_ip(peer), body.renew);
    match result {
        Ok(cookie) => cookie_response(cookie),
        Err(error) => match error {
            PairError::Code => Status::UNAUTHORIZED,
            PairError::Name => Status::BAD_REQUEST,
            PairError::Full | PairError::Backoff => Status::TOO_MANY_REQUESTS,
            PairError::Random => Status::SERVICE_UNAVAILABLE,
        }
        .into_response(),
    }
}

async fn renew(
    State(ctx): State<Arc<Context>>,
    peer: Option<Extension<IpAddr>>,
    headers: HeaderMap,
) -> Response {
    let Some(cookie) = one_header(&headers, header::COOKIE) else {
        return Status::UNAUTHORIZED.into_response();
    };
    match ctx.auth.lock().renew(cookie, client_ip(peer)) {
        Some(cookie) => cookie_response(cookie),
        None => Status::FORBIDDEN.into_response(),
    }
}

async fn identity(State(ctx): State<Arc<Context>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "fingerprint": ctx.fingerprint }))
}

impl Context {
    async fn run_board<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&dyn Board) -> Result<T, Status> + Send + 'static,
    ) -> Result<T, Status> {
        let permit = self
            .jobs
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::SERVICE_UNAVAILABLE)?;
        let board = self.board.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation(board.as_ref())
        })
        .await
        .map_err(|_| Status::SERVICE_UNAVAILABLE)?
    }

    fn role(&self, cookie: &str, ip: IpAddr) -> Option<Role> {
        self.auth.lock().role(cookie, ip)
    }
}

async fn state(
    State(ctx): State<Arc<Context>>,
    peer: Option<Extension<IpAddr>>,
    headers: HeaderMap,
) -> Response {
    let role =
        one_header(&headers, header::COOKIE).and_then(|cookie| ctx.role(cookie, client_ip(peer)));
    let Some(role) = role else {
        return Status::UNAUTHORIZED.into_response();
    };
    match ctx.run_board(move |board| board.view(role)).await {
        Ok(view) => Json(view).into_response(),
        Err(code) => code.into_response(),
    }
}

async fn control(
    State(ctx): State<Arc<Context>>,
    peer: Option<Extension<IpAddr>>,
    headers: HeaderMap,
    Json(change): Json<Control>,
) -> Status {
    let Some(cookie) = one_header(&headers, header::COOKIE) else {
        return Status::UNAUTHORIZED;
    };
    let result = ctx.auth.lock().write_role(cookie, client_ip(peer));
    let role = match result {
        Ok(role) => role,
        Err(code) => return code,
    };
    if !role.permits(&change) {
        return Status::FORBIDDEN;
    }
    if matches!(change.change, Change::Level { db } if !db.is_finite()) {
        return Status::BAD_REQUEST;
    }
    let written = ctx.clone();
    match ctx
        .run_board(move |board| {
            board.apply(role, change)?;
            written.dirty.store(true, Ordering::Release);
            Ok(())
        })
        .await
    {
        Ok(()) => Status::NO_CONTENT,
        Err(code) => code,
    }
}
