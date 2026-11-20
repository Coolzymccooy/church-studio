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

#[tokio::test]
async fn slow_board_writes_leave_authentication_and_requests_responsive() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct SlowBoard(AtomicBool);
    impl Board for SlowBoard {
        fn view(&self, role: Role) -> Result<MixerView, Status> {
            TestBoard::default().view(role)
        }
        fn apply(&self, _: Role, _: Control) -> Result<(), Status> {
            self.0.store(true, Ordering::Release);
            std::thread::sleep(Duration::from_millis(180));
            Ok(())
        }
    }
    let board = Arc::new(SlowBoard(AtomicBool::new(false)));
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(board.clone(), auth.clone(), HOST);
    let code = auth.lock().issue(Role::Desk).unwrap();
    let body = serde_json::json!({ "name": "Rehearsal desk", "code": code }).to_string();
    let paired = send(&app, "POST", "/api/pair", HOST, Some(ORIGIN), None, body).await;
    let cookie = paired.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let pending = {
        let app = app.clone();
        tokio::spawn(async move {
            send(
                &app,
                "POST",
                "/api/control",
                HOST,
                Some(ORIGIN),
                Some(&cookie),
                r#"{"layout":"v1","target":{"type":"strip","index":0},"kind":"level","db":-12}"#
                    .into(),
            )
            .await
            .status()
        })
    };
    let started = Instant::now();
    while !board.0.load(Ordering::Acquire) {
        tokio::task::yield_now().await;
    }
    assert!(auth.try_lock().is_some());
    let response = send(&app, "GET", "/api/state", HOST, None, None, String::new()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "slow board starved unrelated requests"
    );
    assert_eq!(pending.await.unwrap(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn updates_push_changes_to_two_paired_clients_and_enforce_revocation() {
    use futures_util::StreamExt;
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(Arc::new(TestBoard::default()), auth.clone(), HOST);
    let mut cookies = Vec::new();
    for name in ["Fixture A", "Fixture B"] {
        let code = auth.lock().issue(Role::Desk).unwrap();
        let pair = send(
            &app,
            "POST",
            "/api/pair",
            HOST,
            Some(ORIGIN),
            None,
            serde_json::json!({ "name": name, "code": code }).to_string(),
        )
        .await;
        assert_eq!(pair.status(), Status::NO_CONTENT);
        cookies.push(
            pair.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(
        send(&app, "GET", "/api/events", HOST, None, None, String::new())
            .await
            .status(),
        Status::UNAUTHORIZED
    );
    let first = send(
        &app,
        "GET",
        "/api/events",
        HOST,
        None,
        Some(&cookies[0]),
        String::new(),
    )
    .await;
    let second = send(
        &app,
        "GET",
        "/api/events",
        HOST,
        None,
        Some(&cookies[1]),
        String::new(),
    )
    .await;
    let mut a = first.into_body().into_data_stream();
    let mut b = second.into_body().into_data_stream();
    let next = |chunk: axum::body::Bytes| String::from_utf8(chunk.to_vec()).unwrap();
    assert!(next(a.next().await.unwrap().unwrap()).contains("event: state"));
    assert!(next(b.next().await.unwrap().unwrap()).contains("event: state"));
    let response = send(
        &app,
        "POST",
        "/api/control",
        HOST,
        Some(ORIGIN),
        Some(&cookies[0]),
        r#"{"layout":"v1","target":{"type":"strip","index":0},"kind":"level","db":-17}"#.into(),
    )
    .await;
    assert_eq!(response.status(), Status::NO_CONTENT);
    let updates = tokio::time::timeout(Duration::from_millis(150), async {
        tokio::join!(a.next(), b.next())
    })
    .await
    .unwrap();
    for entry in [updates.0, updates.1] {
        assert!(next(entry.unwrap().unwrap()).contains(r#""db":-17.0"#));
    }
    let id = auth.lock().devices()[0].id;
    auth.lock().revoke(id);
    let closed = tokio::time::timeout(Duration::from_secs(1), a.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(next(closed).contains("event: revoked"));
    assert!(a.next().await.is_none());
}

#[tokio::test]
async fn writes_share_desktop_notifications_and_device_limits() {
    #[derive(Default)]
    struct PulseBoard {
        board: TestBoard,
        notifications: Mutex<Vec<Instant>>,
    }

    impl Board for PulseBoard {
        fn view(&self, role: Role) -> Result<MixerView, Status> {
            self.board.view(role)
        }
        fn apply(&self, role: Role, control: Control) -> Result<(), Status> {
            self.board.apply(role, control)
        }
        fn notify(&self) {
            self.notifications.lock().push(Instant::now());
        }
    }

    let board = Arc::new(PulseBoard::default());
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(board.clone(), auth.clone(), HOST);
    let code = auth.lock().issue(Role::Desk).unwrap();
    let pair = send(
        &app,
        "POST",
        "/api/pair",
        HOST,
        Some(ORIGIN),
        None,
        serde_json::json!({ "name": "Desk fixture", "code": code }).to_string(),
    )
    .await;
    let cookie = pair.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let write = || {
        send(
            &app,
            "POST",
            "/api/control",
            HOST,
            Some(ORIGIN),
            Some(cookie),
            r#"{"layout":"v1","target":{"type":"strip","index":0},"kind":"level","db":-3}"#.into(),
        )
    };
    // Generate a burst while the test runtime is on the same event loop.
    let burst = futures_util::future::join_all((0..24).map(|_| write())).await;
    let accepted = burst
        .iter()
        .filter(|response| response.status() == Status::NO_CONTENT)
        .count();
    assert!(accepted > 0 && accepted <= 8);
    assert!(burst
        .iter()
        .any(|response| response.status() == Status::TOO_MANY_REQUESTS));
    tokio::time::sleep(Duration::from_millis(125)).await;
    assert_eq!(write().await.status(), Status::NO_CONTENT);
    tokio::time::sleep(Duration::from_millis(125)).await;
    let pulses = board.notifications.lock();
    assert!(pulses.len() >= 2 && pulses.len() <= 3);
    assert!(pulses
        .windows(2)
        .all(|pair| pair[1] - pair[0] >= Duration::from_millis(50)));
}
