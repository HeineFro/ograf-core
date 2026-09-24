use chrono::Utc;
use dashmap::DashMap;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    models::{
        GraphicInstance, InstanceId, InstanceState, RenderTarget, RendererId, RendererInfo,
        RendererMessage, RendererMetrics, RendererStatus, ServerMessage,
    },
};

pub struct RendererSession {
    pub id: RendererId,
    pub name: String,
    pub connected_at: chrono::DateTime<Utc>,
    pub render_target: RenderTarget,
    pub render_target_schema: Option<serde_json::Value>,
    /// `hello.capabilities.description` / `.customActions` /
    /// `.renderCharacteristics` — passed through to the spec's RendererInfo.
    pub description: Option<String>,
    pub custom_actions: Option<serde_json::Value>,
    pub render_characteristics: Option<serde_json::Value>,
    pub sender: mpsc::Sender<ServerMessage>,
    pub instances: HashMap<InstanceId, GraphicInstance>,
    /// Requests awaiting a correlated reply from this renderer, keyed by the
    /// `requestId` sent out on the ServerMessage.
    pub pending: HashMap<Uuid, oneshot::Sender<RendererMessage>>,
    /// Metrics tracking (for observability)
    pub messages_sent: AtomicU64,
    pub messages_received: AtomicU64,
    /// Set when a request to this renderer times out, cleared by its next
    /// reply — reported as `status: WARNING`.
    pub(crate) timed_out: bool,
    /// Internal connection identifier for cleanup safety. Not exposed in API.
    /// Ensures a refused or late-cleaning-up connection doesn't unregister
    /// a different session that took over the name.
    pub(crate) connection_id: Uuid,
}

/// What's left of a session once it ends — enough to keep listing the
/// renderer (with `status: ERROR`) until it reconnects.
struct DepartedRenderer {
    info: RendererInfo,
}

pub struct RendererRegistry {
    sessions: DashMap<RendererId, RendererSession>,
    departed: DashMap<RendererId, DepartedRenderer>,
    max_pending: usize,
}

impl Default for RendererRegistry {
    fn default() -> Self {
        Self::with_max_pending(100)
    }
}

impl RendererRegistry {
    /// Creates a new RendererRegistry with default settings (max_pending=100).
    /// Use `with_max_pending()` to customize the limit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new RendererRegistry with a custom max pending requests limit.
    pub fn with_max_pending(max_pending: usize) -> Self {
        Self {
            sessions: DashMap::new(),
            departed: DashMap::new(),
            max_pending,
        }
    }

    /// Registers a renderer session. Returns Ok(id) on success, or Err if
    /// the name is already in use (first-wins policy). Atomic check-and-insert
    /// ensures no race conditions with concurrent registration attempts.
    pub async fn register(&self, session: RendererSession) -> Result<RendererId> {
        let id = session.id.clone();

        // Atomic check-and-insert: insert only if key doesn't exist (first-wins)
        use dashmap::mapref::entry::Entry;
        match self.sessions.entry(id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(session);
                self.departed.remove(&id);
                Ok(id)
            }
            Entry::Occupied(_) => Err(AppError::Conflict(format!(
                "renderer name '{}' already connected",
                id
            ))),
        }
    }

    /// Unregisters a renderer session, but only if the connection_id matches.
    /// This prevents a refused or late-cleaning-up connection from unregistering
    /// a different session that took over the name.
    /// The renderer stays listed afterwards, as disconnected.
    pub async fn unregister(&self, id: &str, connection_id: Uuid) {
        let removed = self
            .sessions
            .remove_if(id, |_, session| session.connection_id == connection_id);
        if let Some((id, session)) = removed {
            let info = departed_info(&session);
            self.departed.insert(id, DepartedRenderer { info });
        }
    }

    /// Connected, or disconnected since this process started.
    pub async fn get_info(&self, id: &str) -> Result<RendererInfo> {
        if let Some(entry) = self.sessions.get(id) {
            return Ok(self.session_to_info(&entry));
        }
        self.departed
            .get(id)
            .map(|entry| entry.info.clone())
            .ok_or_else(|| AppError::NotFound(format!("renderer '{id}'")))
    }

    /// Connected renderers plus ones that disconnected since this process
    /// started — check [`RendererInfo::is_connected`] to tell them apart.
    pub async fn list_info(&self) -> Vec<RendererInfo> {
        let connected = self
            .sessions
            .iter()
            .map(|entry| self.session_to_info(entry.value()));
        let departed = self
            .departed
            .iter()
            .filter(|entry| !self.sessions.contains_key(entry.key()))
            .map(|entry| entry.info.clone());
        connected.chain(departed).collect()
    }

    /// Whether any connected renderer has an instance of `graphic_id` —
    /// a deleted graphic's files stay until this is false.
    pub fn graphic_in_use(&self, graphic_id: &str) -> bool {
        self.sessions.iter().any(|session| {
            session
                .instances
                .values()
                .any(|instance| instance.graphic_id == graphic_id)
        })
    }

    /// Sends a command to a renderer and waits for its correlated result.
    /// The OGraf spec requires load()/playAction()/etc HTTP responses to
    /// reflect what actually happened inside the GraphicInstance, so this
    /// is the only way commands reach a renderer — no fire-and-forget.
    pub async fn send_and_await(
        &self,
        renderer_id: &str,
        build: impl FnOnce(Uuid) -> ServerMessage,
        timeout: Duration,
    ) -> Result<RendererMessage> {
        let request_id = Uuid::new_v4();
        let msg = build(request_id);
        let (tx, rx) = oneshot::channel();

        // DashMap's get_mut holds a shard-level lock, not a global lock,
        // so one busy renderer doesn't stall commands to other renderers
        // (assuming they hash to different shards).
        let sender = {
            let mut session = self
                .sessions
                .get_mut(renderer_id)
                .ok_or_else(|| AppError::RendererNotConnected(renderer_id.to_string()))?;

            // Check if renderer has too many pending requests (DoS protection)
            if session.pending.len() >= self.max_pending {
                return Err(AppError::RendererOverloaded(format!(
                    "Renderer has {} pending requests (max: {})",
                    session.pending.len(),
                    self.max_pending
                )));
            }

            session.pending.insert(request_id, tx);
            session.messages_sent.fetch_add(1, Ordering::Relaxed);
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
                if let Some(mut session) = self.sessions.get_mut(renderer_id) {
                    session.pending.remove(&request_id);
                    session.timed_out = true;
                }
                Err(AppError::Timeout(renderer_id.to_string()))
            }
        }
    }

    /// Applies a renderer-confirmed result to session state (so GET renderer
    /// endpoints reflect renderer-confirmed truth, not what was merely sent)
    /// and wakes up the HTTP handler awaiting it via `send_and_await`, if any.
    pub async fn resolve(
        &self,
        renderer_id: &str,
        request_id: Uuid,
        message: RendererMessage,
    ) {
        if let Some(mut session) = self.sessions.get_mut(renderer_id) {
            session.messages_received.fetch_add(1, Ordering::Relaxed);
            session.timed_out = false;
            apply_result(&mut session, &message);

            if let Some(tx) = session.pending.remove(&request_id) {
                let _ = tx.send(message);
            }
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

impl RendererRegistry {
    fn session_to_info(&self, s: &RendererSession) -> RendererInfo {
        session_to_info(s, self.status_of(s))
    }

    /// `WARNING` once requests pile up (half of `max_pending`) or the last
    /// one timed out — the early sign of a renderer that stopped answering
    /// while its WebSocket stays open.
    fn status_of(&self, s: &RendererSession) -> RendererStatus {
        let pending = s.pending.len();
        if pending > 0 && pending >= (self.max_pending / 2).max(1) {
            RendererStatus::warning(format!("{pending} pending requests"))
        } else if s.timed_out {
            RendererStatus::warning("last request timed out")
        } else {
            RendererStatus::ok()
        }
    }
}

fn departed_info(s: &RendererSession) -> RendererInfo {
    let now = Utc::now();
    let mut info = session_to_info(s, RendererStatus::error(format!("disconnected since {}", now.to_rfc3339())));
    info.connected_at = None;
    info.disconnected_at = Some(now);
    info.instances = Vec::new();
    info.metrics = None;
    info
}

fn session_to_info(s: &RendererSession, status: RendererStatus) -> RendererInfo {
    // `instances` is a HashMap (keyed by id for O(1) lookup on actions),
    // which has no defined iteration order — sorting by `loaded_at` here
    // reconstructs actual load order, which is also visual stacking order
    // in typical renderers (newer instances paint on top). Newest first,
    // so callers can treat this list as "top of stack first".
    let mut instances: Vec<_> = s.instances.values().cloned().collect();
    instances.sort_by(|a, b| b.loaded_at.cmp(&a.loaded_at));

    let uptime_seconds = s
        .connected_at
        .signed_duration_since(Utc::now())
        .num_seconds()
        .unsigned_abs();

    let metrics = RendererMetrics {
        pending_requests: s.pending.len(),
        messages_sent: s.messages_sent.load(Ordering::Relaxed),
        messages_received: s.messages_received.load(Ordering::Relaxed),
        uptime_seconds,
    };

    RendererInfo {
        id: s.id.clone(),
        name: s.name.clone(),
        description: s.description.clone(),
        status,
        connected_at: Some(s.connected_at),
        disconnected_at: None,
        render_target: Some(s.render_target.clone()),
        render_target_schema: s.render_target_schema.clone(),
        custom_actions: s.custom_actions.clone(),
        render_characteristics: s.render_characteristics.clone(),
        instances,
        metrics: Some(metrics),
    }
}
