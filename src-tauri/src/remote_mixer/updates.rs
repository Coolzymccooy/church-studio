use super::{client_ip, one_header, Context, Role, Status};
use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Response,
};
use axum::Extension;
use futures_util::future::{select, Either};
use serde::Serialize;
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

type Latest = watch::Sender<Option<String>>;

/// Per role: the latest snapshot JSON and the latest live meter JSON.
#[derive(Default)]
pub(super) struct Channels {
    states: [Latest; ROLES.len()],
    meters: [Latest; ROLES.len()],
}

fn clear(tx: &Latest) {
    tx.send_if_modified(|last| last.take().is_some());
}

fn offer(tx: &Latest, value: &impl Serialize) {
    let Ok(json) = serde_json::to_string(value) else {
        return;
    };
    tx.send_if_modified(|last| {
        if last.as_ref() == Some(&json) {
            return false;
        }
        *last = Some(json);
        true
    });
}

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
        let reads = ROLES.into_iter().enumerate().map(|(slot, role)| {
            let ctx = &ctx;
            let (states, meters) = (&ctx.updates.states[slot], &ctx.updates.meters[slot]);
            async move {
                if states.receiver_count() == 0 {
                    clear(states);
                    clear(meters);
                    return;
                }
                if let Ok((view, live)) = ctx.run_board(move |board| board.live(role)).await {
                    offer(states, &view);
                    match live {
                        Some(live) => offer(meters, &live),
                        None => clear(meters),
                    }
                }
            }
        });
        futures_util::future::join_all(reads).await;
    }
}

struct Subscription {
    ctx: Weak<Context>,
    states: watch::Receiver<Option<String>>,
    meters: watch::Receiver<Option<String>>,
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
    let mut states = ctx.updates.states[role as usize].subscribe();
    states.mark_changed();
    let subscription = Subscription {
        ctx: Arc::downgrade(&ctx),
        states,
        meters: ctx.updates.meters[role as usize].subscribe(),
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
            let changes = next_change(&mut sub.states, &mut sub.meters);
            let Ok(changed) = tokio::time::timeout(AUTH_INTERVAL, changes).await else {
                continue;
            };
            let name = changed.ok()?;
            if sub.ctx.upgrade()?.role(&sub.cookie, sub.ip) != Some(sub.role) {
                continue;
            }
            let latest = if name == "state" {
                &mut sub.states
            } else {
                &mut sub.meters
            };
            let current = latest.borrow_and_update().clone();
            if let Some(json) = current {
                return Some((Ok(Event::default().event(name).data(json)), sub));
            }
        }
        None
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(5)))
        .into_response()
}

/// Wait for the next snapshot or meter change. A snapshot wins a tie, so a
/// new subscriber always gets its state before any meters.
async fn next_change(
    states: &mut watch::Receiver<Option<String>>,
    meters: &mut watch::Receiver<Option<String>>,
) -> Result<&'static str, watch::error::RecvError> {
    let state = std::pin::pin!(states.changed());
    let meter = std::pin::pin!(meters.changed());
    match select(state, meter).await {
        Either::Left((changed, _)) => changed.map(|()| "state"),
        Either::Right((changed, _)) => changed.map(|()| "meters"),
    }
}
