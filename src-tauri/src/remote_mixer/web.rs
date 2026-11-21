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

fn script(source: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], source)
}

fn woff2(bytes: &'static [u8]) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "font/woff2")], bytes)
}

pub(super) async fn js() -> impl IntoResponse {
    script(include_str!("web/desk.js"))
}

pub(super) async fn protocol() -> impl IntoResponse {
    script(include_str!("web/protocol.js"))
}

pub(super) async fn board() -> impl IntoResponse {
    script(include_str!("web/board.js"))
}

pub(super) async fn fader() -> impl IntoResponse {
    script(include_str!("web/fader.js"))
}

pub(super) async fn meters() -> impl IntoResponse {
    script(include_str!("web/meters.js"))
}

pub(super) async fn mixmath() -> impl IntoResponse {
    script(include_str!("web/mixmath.js"))
}

pub(super) async fn font() -> impl IntoResponse {
    woff2(include_bytes!("web/legend.woff2"))
}

pub(super) async fn font_semibold() -> impl IntoResponse {
    woff2(include_bytes!("web/legend-600.woff2"))
}

pub(super) async fn mono() -> impl IntoResponse {
    woff2(include_bytes!("web/mono.woff2"))
}

pub(super) async fn mono_medium() -> impl IntoResponse {
    woff2(include_bytes!("web/mono-500.woff2"))
}
