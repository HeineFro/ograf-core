use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::extract::ws::{Message, WebSocket};
use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    models::{RenderTarget, RendererId, RendererMessage, ServerMessage},
    store::renderers::{RendererRegistry, RendererSession},
    AppState,
};

const PING_TIMEOUT: Duration = Duration::from_secs(30);
const CHANNEL_SIZE: usize = 64;

/// Sanitize renderer-provided strings for logging to prevent log injection.
/// Filters control characters (except space) and limits length.
fn sanitize_for_logs(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .take(100)
        .collect()
}

struct Hello {
    id: RendererId,
    name: String,
    render_target: RenderTarget,
    render_target_schema: Option<Value>,
}

/// `query` is the connect URL's raw query string — already used once by
/// `authorize_connect` before the WS upgrade, threaded through here too so
/// `on_renderer_connected` can see it once the renderer's `hello` names it
/// (e.g. an AccessControl implementation persisting renderer identity).
pub async fn handle_session(mut socket: WebSocket, state: AppState, query: String) {
    let Some(hello) = wait_for_hello(&mut socket).await else {
        return;
    };
    let renderer_id = hello.id;

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(CHANNEL_SIZE);

    let session = RendererSession {
        id: renderer_id,
        name: hello.name.clone(),
        connected_at: Utc::now(),
        render_target: hello.render_target,
        render_target_schema: hello.render_target_schema,
        sender: tx,
        instances: HashMap::new(),
        pending: HashMap::new(),
        messages_sent: std::sync::atomic::AtomicU64::new(0),
        messages_received: std::sync::atomic::AtomicU64::new(0),
    };

    state.renderers.register(session).await;
    state.access.on_renderer_connected(&hello.name, &query).await;

    let welcome = serde_json::json!({ "type": "welcome", "rendererId": renderer_id });
    if socket
        .send(Message::Text(welcome.to_string()))
        .await
        .is_err()
    {
        state.renderers.unregister(renderer_id).await;
        return;
    }

    run_session(&mut socket, &mut rx, renderer_id, &state.renderers).await;
    state.renderers.unregister(renderer_id).await;
    tracing::info!("renderer {renderer_id} disconnected");
}

async fn wait_for_hello(socket: &mut WebSocket) -> Option<Hello> {
    match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<RendererMessage>(&text) {
            Ok(RendererMessage::Hello {
                name,
                render_target,
                capabilities,
            }) => {
                tracing::info!(
                    "renderer hello: {} (renderTarget: {})",
                    sanitize_for_logs(&name),
                    render_target
                );
                let render_target_schema = capabilities.get("renderTargetSchema").cloned();
                Some(Hello {
                    id: Uuid::new_v4(),
                    name,
                    render_target,
                    render_target_schema,
                })
            }
            Ok(_) => {
                tracing::warn!("renderer's first message wasn't hello: {}", sanitize_for_logs(&text));
                None
            }
            Err(err) => {
                tracing::warn!(
                    "renderer sent an invalid hello (missing/invalid renderTarget?): {err} — raw: {}",
                    sanitize_for_logs(&text)
                );
                None
            }
        },
        Ok(Some(Ok(_))) => {
            tracing::warn!("renderer's first WS frame wasn't a text message");
            None
        }
        Ok(Some(Err(err))) => {
            tracing::warn!("WS error while waiting for renderer hello: {err}");
            None
        }
        Ok(None) => {
            tracing::warn!("renderer closed the connection before sending hello");
            None
        }
        Err(_) => {
            tracing::warn!("renderer didn't send hello within 10s");
            None
        }
    }
}

async fn run_session(
    socket: &mut WebSocket,
    rx: &mut mpsc::Receiver<ServerMessage>,
    renderer_id: RendererId,
    registry: &Arc<RendererRegistry>,
) {
    let mut last_ping = tokio::time::Instant::now();

    loop {
        let timeout = tokio::time::sleep_until(last_ping + PING_TIMEOUT);

        tokio::select! {
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(text))) => match serde_json::from_str::<RendererMessage>(&text) {
                    Ok(RendererMessage::Ping) => {
                        last_ping = tokio::time::Instant::now();
                        let _ = socket.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                    }
                    Ok(RendererMessage::Hello { .. }) => {}
                    Ok(result_msg) => {
                        if let Some(request_id) = result_msg.request_id() {
                            registry.resolve(renderer_id, request_id, result_msg).await;
                        }
                    }
                    Err(err) => {
                        tracing::warn!("renderer {renderer_id} sent an unparseable message: {err}");
                    }
                },
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            cmd = rx.recv() => match cmd {
                Some(msg) => {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                None => break,
            },
            _ = timeout => {
                tracing::warn!("renderer {renderer_id} ping timeout, disconnecting");
                break;
            }
        }
    }
}
