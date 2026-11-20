use crate::remote_mixer::{
    router, Auth, Board, Change, Control, MixerView, Role, Status, Target, ViewControl,
};
use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;

const HOST: &str = "desk.example.test:4475";
const ORIGIN: &str = "https://desk.example.test:4475";

#[derive(Default)]
struct TestBoard(Mutex<f32>);

impl Board for TestBoard {
    fn view(&self, role: Role) -> Result<MixerView, Status> {
        Ok(MixerView {
            role,
            running: true,
            layout: "v1".into(),
            controls: vec![ViewControl {
                target: Target::Strip { index: 0 },
                name: "Pastor".into(),
                db: *self.0.lock(),
                muted: (role == Role::Desk).then_some(false),
            }],
        })
    }

    fn apply(&self, _role: Role, control: Control) -> Result<(), Status> {
        if let Change::Level { db } = control.change {
            *self.0.lock() = db;
        }
        Ok(())
    }
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    host: &str,
    origin: Option<&str>,
    cookie: Option<&str>,
    data: String,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, host);
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(
            request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(data))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn protected_state_enforces_host_origin_and_pairing() {
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(Arc::new(TestBoard::default()), auth.clone(), HOST);
    assert_eq!(
        send(&app, "GET", "/api/state", HOST, None, None, "".into())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            "GET",
            "/",
            "other.example.test",
            None,
            None,
            "".into()
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let code = auth.lock().issue(Role::Monitor2).unwrap();
    let body = serde_json::json!({ "name": "Rehearsal phone", "code": code }).to_string();
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/pair",
            HOST,
            Some("https://other.example.test"),
            None,
            body.clone()
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let paired = send(
        &app,
        "POST",
        "/api/pair",
        HOST,
        Some(ORIGIN),
        None,
        body.clone(),
    )
    .await;
    assert_eq!(paired.status(), StatusCode::NO_CONTENT);
    let cookie = paired.headers()[header::SET_COOKIE].to_str().unwrap();
    for attr in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(cookie.contains(attr));
    }
    let cookie = cookie.split(';').next().unwrap();
    assert_eq!(
        send(&app, "POST", "/api/pair", HOST, Some(ORIGIN), None, body)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let state = send(
        &app,
        "GET",
        "/api/state",
        HOST,
        None,
        Some(cookie),
        "".into(),
    )
    .await;
    let snapshot: serde_json::Value =
        serde_json::from_slice(&to_bytes(state.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(snapshot["role"], "monitor2");
    assert!(snapshot["controls"][0].get("muted").is_none());

    let control = |kind: &str, field: &str, value: serde_json::Value| {
        let mut data = serde_json::json!({
            "layout": "v1", "target": { "type": "strip", "index": 0 }, "kind": kind
        });
        data[field] = value;
        data.to_string()
    };
    let mute = control("mute", "muted", true.into());
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/control",
            HOST,
            Some(ORIGIN),
            Some(cookie),
            mute
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let now = Instant::now();
    let level = control("level", "db", (-15.0).into());
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/control",
            HOST,
            Some(ORIGIN),
            Some(cookie),
            level
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert!(now.elapsed() < Duration::from_millis(100));
    let updated = send(
        &app,
        "GET",
        "/api/state",
        HOST,
        None,
        Some(cookie),
        "".into(),
    )
    .await;
    let snapshot: serde_json::Value =
        serde_json::from_slice(&to_bytes(updated.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(snapshot["controls"][0]["db"], -15.0);

    let id = auth.lock().devices()[0].id;
    auth.lock().revoke(id);
    assert_eq!(
        send(
            &app,
            "GET",
            "/api/state",
            HOST,
            None,
            Some(cookie),
            "".into()
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}
