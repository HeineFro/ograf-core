//! The spec's `DELETE /graphics/{graphicId}?force=`.

mod common;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::Harness;
use ograf_core::{
    access::AccessControl,
    models::{Graphic, RendererInfo},
    store::graphics::{GraphicStore, DELETED_MARKER},
};

#[tokio::test]
async fn delete_without_force_unlists_but_keeps_serving_assets() {
    let h = Harness::new();
    h.add_graphic("lower-third");

    let (status, body) = h.request("DELETE", "/ograf/v1/graphics/lower-third", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, list) = h.get("/ograf/v1/graphics").await;
    assert_eq!(list["graphics"].as_array().unwrap().len(), 0, "{list}");
    assert_eq!(h.get("/ograf/v1/graphics/lower-third").await.0, StatusCode::NOT_FOUND);

    // An on-air instance still fetching its module/assets must keep working.
    let asset = h.get("/serverApi/internal/graphics/lower-third/graphic.mjs").await.0;
    assert_eq!(asset, StatusCode::OK);
    assert!(h.graphics.join("lower-third").join(DELETED_MARKER).exists());
}

#[tokio::test]
async fn deleting_an_unlisted_graphic_again_needs_force() {
    let h = Harness::new();
    h.add_graphic("g");
    assert_eq!(h.request("DELETE", "/ograf/v1/graphics/g", None).await.0, StatusCode::OK);

    assert_eq!(h.request("DELETE", "/ograf/v1/graphics/g", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(h.request("DELETE", "/ograf/v1/graphics/g?force=true", None).await.0, StatusCode::OK);
    assert!(!h.graphics.join("g").exists());
}

#[tokio::test]
async fn force_removes_a_listed_graphic_at_once() {
    let h = Harness::new();
    h.add_graphic("g");
    assert_eq!(h.request("DELETE", "/ograf/v1/graphics/g?force=true", None).await.0, StatusCode::OK);
    assert!(!h.graphics.join("g").exists());
    assert_eq!(h.get("/serverApi/internal/graphics/g/graphic.mjs").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_an_unknown_graphic_is_404() {
    let h = Harness::new();
    let (status, body) = h.request("DELETE", "/ograf/v1/graphics/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["status"], 404, "RFC 7807 body: {body}");
}

#[tokio::test]
async fn purge_skips_graphics_still_in_use() {
    let h = Harness::new();
    h.add_graphic("on-air");
    h.add_graphic("off-air");
    let store = GraphicStore::new(&h.graphics);
    store.delete("on-air", false).await.unwrap();
    store.delete("off-air", false).await.unwrap();

    let removed = store.purge_deleted(Duration::ZERO, |id| id == "on-air").await;
    assert_eq!(removed, 1);
    assert!(h.graphics.join("on-air").exists());
    assert!(!h.graphics.join("off-air").exists());
}

#[tokio::test]
async fn purge_waits_for_the_retention_period() {
    let h = Harness::new();
    h.add_graphic("g");
    let store = GraphicStore::new(&h.graphics);
    store.delete("g", false).await.unwrap();

    assert_eq!(store.purge_deleted(Duration::from_secs(3600), |_| false).await, 0);
    assert!(h.graphics.join("g").exists());
}

struct NoDeletes;

#[async_trait]
impl AccessControl for NoDeletes {
    async fn authorize_connect(&self, _query: &str) -> bool {
        true
    }
    async fn on_renderer_connected(&self, _name: &str, _query: &str) {}
    async fn filter_visible(&self, _api_key: &str, renderers: Vec<RendererInfo>) -> Vec<RendererInfo> {
        renderers
    }
    async fn filter_graphics(&self, _api_key: &str, graphics: Vec<Graphic>) -> Vec<Graphic> {
        graphics
    }
    async fn can_target(&self, _api_key: &str, _renderer_name: &str) -> bool {
        true
    }
    async fn can_delete_graphic(&self, _api_key: &str, _graphic_id: &str) -> bool {
        false
    }
}

#[tokio::test]
async fn can_delete_graphic_can_refuse() {
    let h = Harness::with_access(Arc::new(NoDeletes));
    h.add_graphic("g");
    assert_eq!(h.request("DELETE", "/ograf/v1/graphics/g", None).await.0, StatusCode::FORBIDDEN);
    assert!(!h.graphics.join("g").join(DELETED_MARKER).exists());
}
