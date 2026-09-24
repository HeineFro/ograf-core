use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::extract::ws::{Message, WebSocket};
use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    models::{
        is_valid_renderer_name, GraphicInstance, InstanceSnapshot, InstanceState, RenderTarget,
        RendererId, RendererMessage, ServerMessage,
    },
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

/// Convert an InstanceSnapshot (from Hello message) to a GraphicInstance.
/// Sets loaded_at to now and infers state from current_step.
fn snapshot_to_instance(snapshot: &InstanceSnapshot) -> GraphicInstance {
    let state = if snapshot.current_step.is_some() {
        InstanceState::Playing
    } else {
        InstanceState::Loaded
    };

    GraphicInstance {
        instance_id: snapshot.instance_id,
        graphic_id: snapshot.graphic_id.clone(),
        data: snapshot.data.clone(),
        loaded_at: Utc::now(),
        state,
        current_step: snapshot.current_step,
    }
}

struct Hello {
    id: RendererId,
    name: String,
    render_target: RenderTarget,
    render_target_schema: Option<Value>,
    instances: Option<Vec<InstanceSnapshot>>,
}

/// `query` is the connect URL's raw query string — already used once by
/// `authorize_connect` before the WS upgrade, threaded through here too so
/// `on_renderer_connected` can see it once the renderer's `hello` names it
/// (e.g. an AccessControl implementation persisting renderer identity).
pub async fn handle_session(mut socket: WebSocket, state: AppState, query: String) {
    let Some(hello) = wait_for_hello(&mut socket).await else {
        return;
    };

    // Validate renderer name (0.4.0)
    if !is_valid_renderer_name(&hello.name) {
        tracing::warn!(
            "renderer hello with invalid name rejected: '{}'",
            sanitize_for_logs(&hello.name)
        );
        let close_msg = format!("invalid renderer name: must be 1-64 characters, A-Z a-z 0-9 - _ .");
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1008, // Policy violation
                reason: close_msg.into(),
            })))
            .await;
        return;
    }

    // Authorize renderer name (0.4.0: prevent name squatting)
    if !state.access.authorize_name(&hello.name, &query).await {
        tracing::warn!(
            "renderer name authorization failed: '{}'",
            sanitize_for_logs(&hello.name)
        );
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1008, // Policy violation
                reason: "renderer name not authorized".into(),
            })))
            .await;
        return;
    }

    let renderer_id = hello.id.clone();
    let connection_id = Uuid::new_v4(); // Internal ID for cleanup safety

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(CHANNEL_SIZE);

    // Populate instances from Hello snapshots (0.3.0: reconnect state resync)
    let instances = hello
        .instances
        .as_ref()
        .map(|snapshots| {
            snapshots
                .iter()
                .map(|s| (s.instance_id, snapshot_to_instance(s)))
                .collect()
        })
        .unwrap_or_default();

    let session = RendererSession {
        id: renderer_id.clone(),
        name: hello.name.clone(),
        connected_at: Utc::now(),
        render_target: hello.render_target,
        render_target_schema: hello.render_target_schema,
        sender: tx,
        instances,
        pending: HashMap::new(),
        messages_sent: std::sync::atomic::AtomicU64::new(0),
        messages_received: std::sync::atomic::AtomicU64::new(0),
        connection_id,
    };

    // Register session (first-wins policy - will fail if name already taken)
    if let Err(err) = state.renderers.register(session).await {
        tracing::warn!("renderer registration failed: {err}");
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1008, // Policy violation
                reason: "renderer name already connected".into(),
            })))
            .await;
        return;
    }

    state
        .access
        .on_renderer_connected(&hello.name, &query)
        .await;

    let welcome = serde_json::json!({ "type": "welcome", "rendererId": renderer_id });
    if socket
        .send(Message::Text(welcome.to_string()))
        .await
        .is_err()
    {
        state
            .renderers
            .unregister(&renderer_id, connection_id)
            .await;
        return;
    }

    run_session(&mut socket, &mut rx, &renderer_id, &state.renderers).await;
    state.renderers.unregister(&renderer_id, connection_id).await;
    tracing::info!("renderer {renderer_id} disconnected");
}

async fn wait_for_hello(socket: &mut WebSocket) -> Option<Hello> {
    match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<RendererMessage>(&text) {
            Ok(RendererMessage::Hello {
                renderer_id,
                render_target,
                capabilities,
                instances,
            }) => {
                let instance_count = instances.as_ref().map_or(0, |v| v.len());
                tracing::info!(
                    "renderer hello: {} (renderTarget: {}, instances: {})",
                    sanitize_for_logs(&renderer_id),
                    render_target,
                    instance_count
                );
                let render_target_schema = capabilities.get("renderTargetSchema").cloned();
                Some(Hello {
                    id: renderer_id.clone(), // the id doubles as the spec's `name`
                    name: renderer_id,
                    render_target,
                    render_target_schema,
                    instances,
                })
            }
            Ok(_) => {
                tracing::warn!(
                    "renderer's first message wasn't hello: {}",
                    sanitize_for_logs(&text)
                );
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
    renderer_id: &str,
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
