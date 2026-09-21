use chrono::Utc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    models::{
        GraphicInstance, InstanceId, InstanceState, RenderTarget, RendererId, RendererInfo,
        RendererMessage, ServerMessage,
    },
};

pub struct RendererSession {
    pub id: RendererId,
    pub name: String,
    pub connected_at: chrono::DateTime<Utc>,
    pub render_target: RenderTarget,
    pub render_target_schema: Option<serde_json::Value>,
    pub sender: mpsc::Sender<ServerMessage>,
    pub instances: HashMap<InstanceId, GraphicInstance>,
    /// Requests awaiting a correlated reply from this renderer, keyed by the
    /// `requestId` sent out on the ServerMessage.
    pub pending: HashMap<Uuid, oneshot::Sender<RendererMessage>>,
}

#[derive(Default)]
pub struct RendererRegistry {
    sessions: RwLock<HashMap<RendererId, RendererSession>>,
}

impl RendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, session: RendererSession) -> RendererId {
        let id = session.id;
        self.sessions.write().await.insert(id, session);
        id
    }

    pub async fn unregister(&self, id: RendererId) {
        self.sessions.write().await.remove(&id);
    }

    pub async fn get_info(&self, id: RendererId) -> Result<RendererInfo> {
        self.sessions
            .read()
            .await
            .get(&id)
            .map(session_to_info)
            .ok_or_else(|| AppError::NotFound(format!("renderer '{id}'")))
    }

    pub async fn list_info(&self) -> Vec<RendererInfo> {
        self.sessions
            .read()
            .await
            .values()
            .map(session_to_info)
            .collect()
    }

    /// Sends a command to a renderer and waits for its correlated result.
    /// The OGraf spec requires load()/playAction()/etc HTTP responses to
    /// reflect what actually happened inside the GraphicInstance, so this
    /// is the only way commands reach a renderer — no fire-and-forget.
    pub async fn send_and_await(
        &self,
        renderer_id: RendererId,
        build: impl FnOnce(Uuid) -> ServerMessage,
        timeout: Duration,
    ) -> Result<RendererMessage> {
        let request_id = Uuid::new_v4();
        let msg = build(request_id);
        let (tx, rx) = oneshot::channel();

        // Only hold the write lock long enough to register the pending reply
        // and grab a cloned sender handle — the actual WS send (and the
        // await below) happen without it, so one busy renderer's channel
        // can't stall commands to every other renderer.
        let sender = {
            let mut sessions = self.sessions.write().await;
            let session = sessions
                .get_mut(&renderer_id)
                .ok_or_else(|| AppError::RendererNotConnected(renderer_id.to_string()))?;
            session.pending.insert(request_id, tx);
            session.sender.clone()
        };

        sender
            .send(msg)
            .await
            .map_err(|_| AppError::RendererNotConnected(renderer_id.to_string()))?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err(AppError::RendererNotConnected(renderer_id.to_string())),
            Err(_) => {
                if let Some(session) = self.sessions.write().await.get_mut(&renderer_id) {
                    session.pending.remove(&request_id);
                }
                Err(AppError::Timeout(renderer_id.to_string()))
            }
        }
    }

    /// Applies a renderer-confirmed result to session state (so GET renderer
    /// endpoints reflect renderer-confirmed truth, not what was merely sent)
    /// and wakes up the HTTP handler awaiting it via `send_and_await`, if any.
    pub async fn resolve(&self, renderer_id: RendererId, request_id: Uuid, message: RendererMessage) {
        let mut sessions = self.sessions.write().await;
        let Some(session) = sessions.get_mut(&renderer_id) else {
            return;
        };

        apply_result(session, &message);

        if let Some(tx) = session.pending.remove(&request_id) {
            let _ = tx.send(message);
        }
    }
}

fn apply_result(session: &mut RendererSession, message: &RendererMessage) {
    // Only update state on success — Composed Method pattern makes this
    // rule explicit rather than repeating `if status_code < 400` in each arm.
    if !message.is_success() {
        return;
    }

    match message {
        RendererMessage::LoadResult {
            instance_id,
            graphic_id,
            data,
            ..
        } => {
            session.instances.insert(
                *instance_id,
                GraphicInstance {
                    instance_id: *instance_id,
                    graphic_id: graphic_id.clone(),
                    data: data.clone(),
                    loaded_at: Utc::now(),
                    state: InstanceState::Loaded,
                    current_step: None,
                },
            );
        }
        RendererMessage::PlayActionResult {
            instance_id,
            current_step,
            ..
        } => {
            if let Some(instance) = session.instances.get_mut(instance_id) {
                instance.state = InstanceState::Playing;
                instance.current_step = Some(*current_step);
            }
        }
        RendererMessage::StopActionResult { instance_id, .. } => {
            if let Some(instance) = session.instances.get_mut(instance_id) {
                instance.state = InstanceState::Stopped;
            }
        }
        RendererMessage::UpdateActionResult {
            instance_id,
            data: Some(data),
            ..
        } => {
            if let Some(instance) = session.instances.get_mut(instance_id) {
                instance.data = Some(data.clone());
            }
        }
        RendererMessage::ClearResult { instance_id, .. } => {
            session.instances.remove(instance_id);
        }
        _ => {}
    }
}

fn session_to_info(s: &RendererSession) -> RendererInfo {
    // `instances` is a HashMap (keyed by id for O(1) lookup on actions),
    // which has no defined iteration order — sorting by `loaded_at` here
    // reconstructs actual load order, which is also visual stacking order
    // in typical renderers (newer instances paint on top). Newest first,
    // so callers can treat this list as "top of stack first".
    let mut instances: Vec<_> = s.instances.values().cloned().collect();
    instances.sort_by(|a, b| b.loaded_at.cmp(&a.loaded_at));

    RendererInfo {
        id: s.id,
        name: s.name.clone(),
        connected_at: s.connected_at,
        render_target: s.render_target.clone(),
        render_target_schema: s.render_target_schema.clone(),
        instances,
    }
}
