//! Shared test harness: a Core router over a temporary graphics directory.

#![allow(dead_code)]

use std::{path::PathBuf, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use ograf_core::{
    access::{AccessControl, AllowAllAccessControl},
    build_router,
    config::Config,
    directory::RendererDirectory,
    store::renderers::RendererRegistry,
    AppState, Router,
};
use serde_json::Value;
use tower::ServiceExt;

pub struct Harness {
    pub state: AppState,
    pub graphics: PathBuf,
}

impl Harness {
    pub fn new() -> Self {
        Self::with_access(Arc::new(AllowAllAccessControl))
    }

    pub fn with_access(access: Arc<dyn AccessControl>) -> Self {
        let graphics = std::env::temp_dir().join(format!("ograf-core-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&graphics).unwrap();
        let mut config = Config::from_env();
        config.graphics_storage = graphics.to_string_lossy().into_owned();
        config.graphics_cache_ttl_secs = 0;
        config.action_timeout_ms = 300;
        let state = AppState::new(Arc::new(config), Arc::new(RendererRegistry::new()), access);
        Self { state, graphics }
    }

    pub fn with_directory(mut self, directory: Arc<dyn RendererDirectory>) -> Self {
        self.state = self.state.clone().with_directory(directory);
        self
    }

    pub fn router(&self) -> Router {
        build_router(self.state.clone())
    }

    /// A graphic directory with a manifest and one asset.
    pub fn add_graphic(&self, id: &str) {
        let dir = self.graphics.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{id}.ograf.json")),
            format!(r#"{{"id":"{id}","name":"{id}","main":"graphic.mjs"}}"#),
        )
        .unwrap();
        std::fs::write(dir.join("graphic.mjs"), "export default class {}").unwrap();
    }

    pub async fn request(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let builder = Request::builder().method(method).uri(path);
        let request = match body {
            Some(body) => builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => builder.body(Body::empty()),
        }
        .unwrap();
        let response = self.router().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.request("GET", path, None).await
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.graphics);
    }
}
