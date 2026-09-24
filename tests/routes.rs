//! Router-level checks: which handler a path actually reaches.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use ograf_core::{
    access::AllowAllAccessControl, build_router, config::Config,
    store::renderers::RendererRegistry, AppState, Router,
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

#[tokio::test]
async fn server_info_answers_with_and_without_trailing_slash() {
    assert_eq!(get("/ograf/v1/").await.0, StatusCode::OK);
    assert_eq!(get("/ograf/v1").await.0, StatusCode::OK);
}
