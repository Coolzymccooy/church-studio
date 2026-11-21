//! Live meters on the remote SSE stream: read-only, scoped to the paired role.
use crate::remote_mixer::{
    router, Auth, Board, Control, MeterView, MixerView, Role, Status, Target, ViewControl,
};
use axum::body::{Body, BodyDataStream};
use axum::http::{header, Request};
use axum::Router;
use futures_util::StreamExt;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const HOST: &str = "desk.example.test:4475";
const ORIGIN: &str = "https://desk.example.test:4475";

/// Tags every reading with the role it was built for, so a leak is visible.
fn role_tag(role: Role) -> f32 {
    match role {
        Role::Monitor1 => -1.0,
        Role::Monitor2 => -2.0,
        Role::Monitor3 => -3.0,
        Role::Desk => -4.0,
    }
}

#[derive(Default)]
struct MeterBoard {
    reads: AtomicU32,
}

impl Board for MeterBoard {
    fn view(&self, role: Role) -> Result<MixerView, Status> {
        Ok(MixerView {
            role,
            running: true,
            layout: "v1".into(),
            controls: vec![ViewControl {
                target: Target::Strip { index: 0 },
                name: "Pastor".into(),
                db: 0.0,
                muted: None,
            }],
        })
    }

    fn apply(&self, _: Role, _: Control) -> Result<(), Status> {
        Ok(())
    }

    fn live(&self, role: Role) -> Result<(MixerView, Option<MeterView>), Status> {
        let tick = self.reads.fetch_add(1, Ordering::Relaxed) as f32;
        let tag = role_tag(role);
        let meters = MeterView {
            strips: vec![-20.0 - (tick % 10.0)],
            mix: (role != Role::Desk).then_some([tag; 4]),
            buses: if role == Role::Desk {
                BTreeMap::from([("main", [tag, tag])])
            } else {
                BTreeMap::new()
            },
        };
        Ok((self.view(role)?, Some(meters)))
    }
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: String,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json");
    if method != "GET" {
        builder = builder.header(header::ORIGIN, ORIGIN);
    }
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn pair(app: &Router, auth: &Mutex<Auth>, role: Role) -> String {
    let code = auth.lock().issue(role).unwrap();
    let body = serde_json::json!({ "name": "Meter fixture", "code": code }).to_string();
    let paired = request(app, "POST", "/api/pair", None, body).await;
    assert_eq!(paired.status(), Status::NO_CONTENT);
    paired.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

async fn subscribe(app: &Router, cookie: &str) -> BodyDataStream {
    let events = request(app, "GET", "/api/events", Some(cookie), String::new()).await;
    assert_eq!(events.status(), Status::OK);
    events.into_body().into_data_stream()
}

/// The next SSE event as (name, data).
async fn next_event(stream: &mut BodyDataStream) -> (String, String) {
    let chunk = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("an event within a second")
        .expect("stream open")
        .unwrap();
    let text = String::from_utf8(chunk.to_vec()).unwrap();
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .map(|value| value.trim().to_owned())
            .unwrap_or_default()
    };
    (field("event:"), field("data:"))
}

async fn next_meters(stream: &mut BodyDataStream) -> serde_json::Value {
    loop {
        let (name, data) = next_event(stream).await;
        if name == "meters" {
            return serde_json::from_str(&data).unwrap();
        }
    }
}

#[tokio::test]
async fn each_role_receives_only_its_own_meters_after_the_first_state() {
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(Arc::new(MeterBoard::default()), auth.clone(), HOST);
    let mon2 = pair(&app, &auth, Role::Monitor2).await;
    let desk = pair(&app, &auth, Role::Desk).await;
    let mut mon2_events = subscribe(&app, &mon2).await;
    let mut desk_events = subscribe(&app, &desk).await;

    assert_eq!(
        next_event(&mut mon2_events).await.0,
        "state",
        "state comes first"
    );
    let mon2_meters = next_meters(&mut mon2_events).await;
    assert_eq!(
        mon2_meters["mix"],
        serde_json::json!([-2.0, -2.0, -2.0, -2.0])
    );
    assert!(mon2_meters.get("buses").is_none());
    assert_eq!(mon2_meters["strips"].as_array().unwrap().len(), 1);

    assert_eq!(next_event(&mut desk_events).await.0, "state");
    let desk_meters = next_meters(&mut desk_events).await;
    assert!(desk_meters.get("mix").is_none());
    assert_eq!(
        desk_meters["buses"]["main"],
        serde_json::json!([-4.0, -4.0])
    );

    for _ in 0..5 {
        assert_eq!(next_meters(&mut mon2_events).await["mix"][0], -2.0);
    }
}

#[tokio::test]
async fn meters_arrive_at_fifteen_to_twenty_hertz_and_stop_on_revocation() {
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(Arc::new(MeterBoard::default()), auth.clone(), HOST);
    let cookie = pair(&app, &auth, Role::Monitor1).await;
    assert_eq!(
        request(&app, "GET", "/api/events", None, String::new())
            .await
            .status(),
        Status::UNAUTHORIZED
    );
    let mut events = subscribe(&app, &cookie).await;
    next_meters(&mut events).await;

    let started = tokio::time::Instant::now();
    let mut count = 0;
    while started.elapsed() < Duration::from_secs(1) {
        next_meters(&mut events).await;
        count += 1;
    }
    assert!(
        (12..=22).contains(&count),
        "{count} meter events in one second"
    );

    let id = auth.lock().devices()[0].id;
    auth.lock().revoke(id);
    let revoked = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let (name, _) = next_event(&mut events).await;
            if name == "revoked" {
                break;
            }
            assert_eq!(name, "meters", "only queued meters before the revocation");
        }
    })
    .await;
    assert!(revoked.is_ok(), "the stream reports the revocation");
    assert!(events.next().await.is_none(), "no meters after revocation");
}

#[tokio::test]
async fn meters_are_read_only_for_every_role() {
    let auth = Arc::new(Mutex::new(Auth::default()));
    let app = router(Arc::new(MeterBoard::default()), auth.clone(), HOST);
    let cookie = pair(&app, &auth, Role::Monitor2).await;
    for path in ["/api/meters", "/api/events"] {
        let status = request(&app, "POST", path, Some(&cookie), "{}".into())
            .await
            .status();
        assert!(
            status == Status::METHOD_NOT_ALLOWED || status == Status::NOT_FOUND,
            "{path} accepted a write: {status}"
        );
    }
}
