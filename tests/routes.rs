//! Router-level checks: which handler a path actually reaches.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use ograf_core::{
    access::AllowAllAccessControl, build_router, config::Config,
    store::renderers::RendererRegistry, AppState, Router, RENDERER_CONNECT_PATH,
};
use serde_json::Value;
use tower::ServiceExt;

fn app() -> Router {
    build_router(AppState {
        config: Arc::new(Config::from_env()),
        renderers: Arc::new(RendererRegistry::new()),
        access: Arc::new(AllowAllAccessControl),
    })
}

async fn get(path: &str) -> (StatusCode, Value) {
    let response = app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// `connect` is an ordinary renderer id now: the lookup answers the spec's
/// "No Renderer found", not the WebSocket upgrade handler.
#[tokio::test]
async fn renderers_connect_is_a_renderer_lookup() {
    let (status, body) = get("/ograf/v1/renderers/connect").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("connect"),
        "{body}"
    );
}

/// A plain GET (no upgrade headers) reaching the WebSocket handler is
/// refused by axum's `WebSocketUpgrade` extractor, never a 404.
#[tokio::test]
async fn websocket_route_lives_outside_the_server_api() {
    let (status, _) = get(RENDERER_CONNECT_PATH).await;
    assert_ne!(status, StatusCode::NOT_FOUND);
    assert!(!RENDERER_CONNECT_PATH.starts_with("/ograf/v1"));
}

#[tokio::test]
async fn server_info_answers_with_and_without_trailing_slash() {
    assert_eq!(get("/ograf/v1/").await.0, StatusCode::OK);
    assert_eq!(get("/ograf/v1").await.0, StatusCode::OK);
}

#[tokio::test]
async fn server_info_comes_from_cargo_metadata() {
    let (status, body) = get("/ograf/v1/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], env!("CARGO_PKG_NAME"));
    assert_eq!(body["description"], env!("CARGO_PKG_DESCRIPTION"));
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert!(
        body["author"]["name"]
            .as_str()
            .is_some_and(|n| !n.is_empty()),
        "{body}"
    );
    assert!(
        !body["author"]["name"].as_str().unwrap().contains('<'),
        "{body}"
    );
    assert_eq!(body["author"]["url"], env!("CARGO_PKG_REPOSITORY"));
}
