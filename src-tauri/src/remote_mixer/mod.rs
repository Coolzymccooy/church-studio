mod auth;
pub mod desktop;
mod extensions;
mod tls;
mod web;

pub use auth::Auth;
pub use axum::http::StatusCode as Status;

use auth::PairError;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

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

    fn permits(self, change: &Control) -> bool {
        self == Self::Desk
            || matches!(
                (&change.target, &change.change),
                (Target::Strip { .. }, Change::Level { .. })
            )
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Target {
    Strip { index: u32 },
    Bus { id: String },
    Return { id: String },
    Dca { index: u32 },
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
}

struct Context {
    board: Arc<dyn Board>,
    auth: Arc<Mutex<Auth>>,
    hosts: [String; 1],
    origin: String,
}

pub fn router(board: Arc<dyn Board>, auth: Arc<Mutex<Auth>>, host: &str) -> Router {
    let ctx = Arc::new(Context {
        board,
        auth,
        hosts: [host.into()],
        origin: format!("https://{host}"),
    });
    Router::new()
        .route("/", get(web::index))
        .route("/desk.css", get(web::css))
        .route("/desk.js", get(web::js))
        .route("/protocol.js", get(web::protocol))
        .route("/font.woff2", get(web::font))
        .route("/mono.woff2", get(web::mono))
        .route("/api/pair", post(pair))
        .route("/api/state", get(state))
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
}

async fn pair(State(ctx): State<Arc<Context>>, Json(body): Json<PairRequest>) -> Response {
    match ctx.auth.lock().pair(&body.code, &body.name) {
        Ok(cookie) => match HeaderValue::from_str(&cookie) {
            Ok(cookie) => (Status::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response(),
            Err(_) => Status::INTERNAL_SERVER_ERROR.into_response(),
        },
        Err(error) => match error {
            PairError::Code => Status::UNAUTHORIZED,
            PairError::Name => Status::BAD_REQUEST,
            PairError::Full => Status::TOO_MANY_REQUESTS,
            PairError::Random => Status::SERVICE_UNAVAILABLE,
        }
        .into_response(),
    }
}

fn authorized(auth: &Auth, headers: &HeaderMap) -> Option<Role> {
    auth.role(one_header(headers, header::COOKIE)?)
}

async fn state(State(ctx): State<Arc<Context>>, headers: HeaderMap) -> Response {
    let role = { authorized(&ctx.auth.lock(), &headers) };
    let Some(role) = role else {
        return Status::UNAUTHORIZED.into_response();
    };
    match ctx.board.view(role) {
        Ok(view) => Json(view).into_response(),
        Err(code) => code.into_response(),
    }
}

async fn control(
    State(ctx): State<Arc<Context>>,
    headers: HeaderMap,
    Json(change): Json<Control>,
) -> Status {
    let auth = ctx.auth.lock();
    let Some(role) = authorized(&auth, &headers) else {
        return Status::UNAUTHORIZED;
    };
    if !role.permits(&change) {
        return Status::FORBIDDEN;
    }
    if matches!(change.change, Change::Level { db } if !db.is_finite()) {
        return Status::BAD_REQUEST;
    }
    match ctx.board.apply(role, change) {
        Ok(()) => Status::NO_CONTENT,
        Err(code) => code,
    }
}
