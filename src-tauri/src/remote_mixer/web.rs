use axum::http::header;
use axum::response::{Html, IntoResponse};

pub(super) async fn index() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

pub(super) async fn css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        concat!(
            include_str!("../../../src/styles/faceplate-tokens.css"),
            include_str!("../../../src/styles/faceplate-parts.css"),
            include_str!("web/desk.css")
        ),
    )
}

pub(super) async fn js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("web/desk.js"),
    )
}

pub(super) async fn protocol() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("web/protocol.js"),
    )
}

pub(super) async fn font() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "font/woff2")],
        include_bytes!("web/legend.woff2").as_slice(),
    )
}

pub(super) async fn mono() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "font/woff2")],
        include_bytes!("web/mono.woff2").as_slice(),
    )
}
