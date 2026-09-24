//! Renderer status, offline renderers and the RendererDirectory — over a real
//! WebSocket, the way a renderer connects.

mod common;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::Harness;
use futures_util::{SinkExt, StreamExt};
use ograf_core::{
    directory::{KnownRenderer, RendererDirectory},
    RENDERER_CONNECT_PATH,
};
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn serve(h: &Harness) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = h.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("ws://{addr}{RENDERER_CONNECT_PATH}")
}

/// Connects and says hello; returns once `welcome` arrived.
async fn connect_renderer(url: &str, id: &str, capabilities: Value) -> Socket {
    let (mut ws, _) = connect_async(url).await.unwrap();
    let hello = json!({ "type": "hello", "rendererId": id, "renderTarget": { "layer": 1 }, "capabilities": capabilities });
    ws.send(Message::Text(hello.to_string())).await.unwrap();
    loop {
        if let Some(Ok(Message::Text(text))) = ws.next().await {
            if serde_json::from_str::<Value>(&text).unwrap()["type"] == "welcome" {
                return ws;
            }
        }
    }
}

/// Polls `GET /renderers/{id}` until its status is `level`.
async fn wait_for_status(h: &Harness, id: &str, level: &str) -> Value {
    for _ in 0..50 {
        let (_, body) = h.get(&format!("/ograf/v1/renderers/{id}")).await;
        if body["renderer"]["status"]["status"] == level {
            return body["renderer"].clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("renderer '{id}' never reached status {level}");
}

#[tokio::test]
async fn connected_renderer_is_ok_and_reports_its_capabilities() {
    let h = Harness::new();
    let url = serve(&h).await;
    let capabilities = json!({
        "description": "Studio 1, layer 10",
        "customActions": [{ "id": "reload", "name": "Reload" }],
        "renderCharacteristics": { "resolution": { "width": 1920, "height": 1080 } },
    });
    let _ws = connect_renderer(&url, "studio1", capabilities).await;

    let renderer = wait_for_status(&h, "studio1", "OK").await;
    assert_eq!(renderer["description"], "Studio 1, layer 10");
    assert_eq!(renderer["customActions"][0]["id"], "reload");
    assert_eq!(renderer["renderCharacteristics"]["resolution"]["width"], 1920);
    assert_eq!(renderer["renderTargets"].as_array().unwrap().len(), 1);

    let (_, list) = h.get("/ograf/v1/renderers").await;
    assert_eq!(list["renderers"][0]["status"]["status"], "OK", "{list}");
    assert_eq!(list["renderers"][0]["description"], "Studio 1, layer 10");
}

#[tokio::test]
async fn disconnected_renderer_stays_listed_as_error_and_refuses_actions() {
    let h = Harness::new();
    h.add_graphic("g");
    let url = serve(&h).await;
    let mut ws = connect_renderer(&url, "studio1", json!({})).await;
    wait_for_status(&h, "studio1", "OK").await;

    ws.close(None).await.unwrap();
    let renderer = wait_for_status(&h, "studio1", "ERROR").await;
    assert!(renderer["status"]["message"].as_str().unwrap().starts_with("disconnected since"));
    assert_eq!(renderer["renderTargets"], json!([]));

    let load = json!({ "renderTarget": { "layer": 1 }, "graphicId": "g", "params": { "data": {} } });
    let (status, body) = h
        .request("POST", "/ograf/v1/renderers/studio1/target/graphicInstance/load", Some(load))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let clear = json!({ "filters": [] });
    let (status, _) = h
        .request("PUT", "/ograf/v1/renderers/studio1/target/graphicInstance/clear", Some(clear))
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn reconnecting_renderer_is_ok_again() {
    let h = Harness::new();
    let url = serve(&h).await;
    let mut ws = connect_renderer(&url, "studio1", json!({})).await;
    ws.close(None).await.unwrap();
    wait_for_status(&h, "studio1", "ERROR").await;

    let _ws = connect_renderer(&url, "studio1", json!({})).await;
    wait_for_status(&h, "studio1", "OK").await;
    let (_, list) = h.get("/ograf/v1/renderers").await;
    assert_eq!(list["renderers"].as_array().unwrap().len(), 1, "no duplicate: {list}");
}

#[tokio::test]
async fn timed_out_request_is_a_warning() {
    let h = Harness::new();
    h.add_graphic("g");
    let url = serve(&h).await;
    let _ws = connect_renderer(&url, "studio1", json!({})).await; // never answers

    let load = json!({ "renderTarget": { "layer": 1 }, "graphicId": "g", "params": { "data": {} } });
    let (status, _) = h
        .request("POST", "/ograf/v1/renderers/studio1/target/graphicInstance/load", Some(load))
        .await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);

    let renderer = wait_for_status(&h, "studio1", "WARNING").await;
    assert_eq!(renderer["status"]["message"], "last request timed out");
}

#[tokio::test]
async fn graphic_instance_id_that_is_not_a_uuid_is_404() {
    let h = Harness::new();
    let url = serve(&h).await;
    let _ws = connect_renderer(&url, "studio1", json!({})).await;
    wait_for_status(&h, "studio1", "OK").await;

    let update = json!({ "renderTarget": { "layer": 1 }, "graphicInstanceId": "graphic-instance-0", "params": { "data": {} } });
    let (status, body) = h
        .request("POST", "/ograf/v1/renderers/studio1/target/graphicInstance/updateAction", Some(update))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let clear = json!({ "filters": [{ "graphicInstanceId": "graphic-instance-0" }] });
    let (status, body) = h
        .request("PUT", "/ograf/v1/renderers/studio1/target/graphicInstance/clear", Some(clear))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["graphicInstances"], json!([]));
}

struct Directory(Vec<KnownRenderer>);

#[async_trait]
impl RendererDirectory for Directory {
    async fn known_renderers(&self) -> Vec<KnownRenderer> {
        self.0.clone()
    }
}

#[tokio::test]
async fn directory_renderers_are_listed_until_they_connect() {
    let directory = Directory(vec![
        KnownRenderer::new("studio1"),
        KnownRenderer::new("backup").with_description("Backup machine"),
    ]);
    let h = Harness::new().with_directory(Arc::new(directory));
    let url = serve(&h).await;
    let _ws = connect_renderer(&url, "studio1", json!({})).await;
    wait_for_status(&h, "studio1", "OK").await;

    let (_, list) = h.get("/ograf/v1/renderers").await;
    let renderers = list["renderers"].as_array().unwrap();
    assert_eq!(renderers.len(), 2, "connected one isn't duplicated: {list}");
    let backup = renderers.iter().find(|r| r["id"] == "backup").unwrap();
    assert_eq!(backup["status"], json!({ "status": "ERROR", "message": "not connected" }));
    assert_eq!(backup["description"], "Backup machine");

    let (status, body) = h.get("/ograf/v1/renderers/backup").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["renderer"]["renderTargets"], json!([]));
}

#[tokio::test]
async fn directory_description_fills_in_for_a_connected_renderer() {
    let directory = Directory(vec![KnownRenderer::new("studio1").with_description("Studio 1, layer 10")]);
    let h = Harness::new().with_directory(Arc::new(directory));
    let url = serve(&h).await;
    let _ws = connect_renderer(&url, "studio1", json!({})).await;

    let renderer = wait_for_status(&h, "studio1", "OK").await;
    assert_eq!(renderer["description"], "Studio 1, layer 10");
    let (_, list) = h.get("/ograf/v1/renderers").await;
    assert_eq!(list["renderers"][0]["description"], "Studio 1, layer 10");
}

#[tokio::test]
async fn unknown_renderer_is_a_problem_details_404() {
    let h = Harness::new();
    let (status, body) = h.get("/ograf/v1/renderers/nobody").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["title"], "Not Found");
    assert_eq!(body["status"], 404);
    assert!(body["detail"].as_str().unwrap().contains("nobody"));
    assert_eq!(body["error"], body["detail"], "old clients keep their field");
}
