use super::{client_ip, one_header, Context, Role, Status};
use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Response,
};
use axum::Extension;
use std::{
    convert::Infallible,
    net::IpAddr,
    sync::{atomic::Ordering, Arc, Weak},
    time::Duration,
};
use tokio::{sync::watch, time::Instant};

const UPDATE_INTERVAL: Duration = Duration::from_millis(60);
const AUTH_INTERVAL: Duration = Duration::from_millis(500);
const ROLES: [Role; 4] = [Role::Monitor1, Role::Monitor2, Role::Monitor3, Role::Desk];

#[derive(Default)]
pub(super) struct Channels([watch::Sender<Option<String>>; ROLES.len()]);

pub(super) async fn publish(context: Weak<Context>) {
    let mut tick = tokio::time::interval(UPDATE_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_notify = None::<Instant>;
    loop {
        tick.tick().await;
        let Some(ctx) = context.upgrade() else { break };
        let notify_due = last_notify.is_none_or(|last| last.elapsed() >= Duration::from_millis(50));
        if notify_due && ctx.dirty.swap(false, Ordering::AcqRel) {
            let now = Instant::now();
            let notified = ctx
                .run_board(|board| {
                    board.notify();
                    Ok(())
                })
                .await;
            if notified.is_ok() {
                last_notify = Some(now);
            } else {
                ctx.dirty.store(true, Ordering::Release);
            }
        }
        let reads = ROLES.into_iter().zip(&ctx.updates.0).map(|(role, tx)| {
            let ctx = &ctx;
            async move {
                if tx.receiver_count() == 0 {
                    tx.send_if_modified(|last| last.take().is_some());
                    return;
                }
                if let Ok(view) = ctx.run_board(move |board| board.view(role)).await {
                    if let Ok(json) = serde_json::to_string(&view) {
                        tx.send_if_modified(|last| {
                            if last.as_ref() == Some(&json) {
                                return false;
                            }
                            *last = Some(json);
                            true
                        });
                    }
                }
            }
        });
        futures_util::future::join_all(reads).await;
    }
}

struct Subscription {
    ctx: Weak<Context>,
    receiver: watch::Receiver<Option<String>>,
    cookie: String,
    ip: IpAddr,
    role: Role,
    closed: bool,
}

pub(super) async fn events(
    State(ctx): State<Arc<Context>>,
    peer: Option<Extension<IpAddr>>,
    headers: HeaderMap,
) -> Response {
    let ip = client_ip(peer);
    let Some(cookie) = one_header(&headers, header::COOKIE) else {
        return Status::UNAUTHORIZED.into_response();
    };
    let Some(role) = ctx.role(cookie, ip) else {
        return Status::UNAUTHORIZED.into_response();
    };
    let mut receiver = ctx.updates.0[role as usize].subscribe();
    receiver.mark_changed();
    let subscription = Subscription {
        ctx: Arc::downgrade(&ctx),
        receiver,
        cookie: cookie.into(),
        ip,
        role,
        closed: false,
    };
    let stream = futures_util::stream::unfold(subscription, |mut sub| async move {
        while !sub.closed {
            let ctx = sub.ctx.upgrade()?;
            if ctx.role(&sub.cookie, sub.ip) != Some(sub.role) {
                sub.closed = true;
                return Some((
                    Ok::<_, Infallible>(Event::default().event("revoked").data("Pair again")),
                    sub,
                ));
            }
            drop(ctx);
            let Ok(changed) = tokio::time::timeout(AUTH_INTERVAL, sub.receiver.changed()).await
            else {
                continue;
            };
            changed.ok()?;
            if sub.ctx.upgrade()?.role(&sub.cookie, sub.ip) != Some(sub.role) {
                continue;
            }
            let current = sub.receiver.borrow_and_update().clone();
            if let Some(json) = current {
                return Some((Ok(Event::default().event("state").data(json)), sub));
            }
        }
        None
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(5)))
        .into_response()
}
