use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Renderer identifier - the renderer's stable name.
/// Changed from Uuid in 0.4.0 for spec compliance and stability across reconnects.
pub type RendererId = String;

/// Validates a renderer name for use as RendererId.
///
/// Valid names: 1-64 characters, A-Z a-z 0-9 - _ .
/// Case-sensitive, needs no URL escaping in path segments. `.` and `..` are
/// refused: they're dot-segments a client or proxy normalizes away, so
/// `/renderers/{rendererId}` could never reach them.
///
/// # Examples
/// ```
/// use ograf_core::models::is_valid_renderer_name;
///
/// assert!(is_valid_renderer_name("main-output"));
/// assert!(is_valid_renderer_name("Renderer_1.backup"));
/// assert!(!is_valid_renderer_name(""));  // too short
/// assert!(!is_valid_renderer_name("renderer with spaces"));  // invalid chars
/// assert!(!is_valid_renderer_name(".."));  // dot-segment
/// ```
pub fn is_valid_renderer_name(name: &str) -> bool {
    let len = name.len();
    len >= 1
        && len <= 64
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

// Re-export protocol types for backward compatibility
pub use crate::protocol::{
    InstanceId, InstanceSnapshot, RenderTarget, RendererMessage, ServerMessage,
};

// Re-export validation helpers for public use
pub use crate::store::graphics::is_valid_graphic_id;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Graphic {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub manifest: Value,
    pub storage_path: String,
    pub uploaded_at: String,
}

impl Graphic {
    /// The public, spec-shaped `GraphicListInfo` view of this graphic —
    /// deliberately omits internal fields like `storage_path`.
    pub fn list_info(&self) -> Value {
        let mut info = json!({
            "id": self.id,
            "name": self.name,
        });
        if let Some(description) = &self.description {
            info["description"] = json!(description);
        }
        if let Some(thumbnails) = self.manifest.get("thumbnails") {
            info["thumbnails"] = thumbnails.clone();
        }
        info
    }
}

/// Metrics for a renderer connection (extension, not part of OGraf spec).
/// Added to RendererInfo response to provide observability without requiring
/// a separate metrics endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RendererMetrics {
    pub pending_requests: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub uptime_seconds: u64,
}

/// Not part of the OGraf spec itself (the spec has no instance state
/// machine) — this is Core's own bookkeeping, inferred from which
/// action last succeeded, purely so dashboards/UIs can show more than
/// "a graphic is loaded here" (see `store::renderers::apply_result`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstanceState {
    Loaded,
    Playing,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GraphicInstance {
    pub instance_id: InstanceId,
    pub graphic_id: String,
    pub data: Option<Value>,
    pub loaded_at: DateTime<Utc>,
    pub state: InstanceState,
    /// The last `currentStep` a playAction() reply reported — `None` until
    /// the first PlayAction (freshly loaded instances have no step yet;
    /// StopAction doesn't report one either, per the spec, so it just keeps
    /// whatever it was last).
    pub current_step: Option<f64>,
}

/// The spec's `RendererInfo.status.status`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum StatusLevel {
    Ok,
    Warning,
    Error,
}

/// The spec's `RendererInfo.status` — `{ "status": "OK", "message": "…" }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RendererStatus {
    pub status: StatusLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl RendererStatus {
    pub fn ok() -> Self {
        Self { status: StatusLevel::Ok, message: None }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self { status: StatusLevel::Warning, message: Some(message.into()) }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { status: StatusLevel::Error, message: Some(message.into()) }
    }
}

/// A renderer as Core knows it: connected, disconnected since this process
/// started, or only known through a [`RendererDirectory`](crate::directory::RendererDirectory).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RendererInfo {
    pub id: RendererId,
    pub name: String,
    /// From `hello.capabilities.description`, or the directory's.
    pub description: Option<String>,
    pub status: RendererStatus,
    /// `None` while not connected.
    pub connected_at: Option<DateTime<Utc>>,
    /// Set once a session ends; `None` while connected or never seen.
    pub disconnected_at: Option<DateTime<Utc>>,
    /// The renderer's fixed RenderTarget from `hello` — `None` for a renderer
    /// that hasn't connected since this process started.
    pub render_target: Option<RenderTarget>,
    /// The renderer's own declared schema for `render_target`, from
    /// `hello.capabilities.renderTargetSchema` — `None` if it didn't send
    /// one, in which case callers fall back to a generic placeholder.
    pub render_target_schema: Option<Value>,
    /// The spec's `RendererInfo.customActions`, from `hello.capabilities`.
    pub custom_actions: Option<Value>,
    /// The spec's `RendererInfo.renderCharacteristics`, from `hello.capabilities`.
    pub render_characteristics: Option<Value>,
    /// Empty while not connected.
    pub instances: Vec<GraphicInstance>,
    /// Optional metrics for observability (extension, not part of OGraf spec).
    /// Only populated while connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<RendererMetrics>,
}

impl RendererInfo {
    pub fn is_connected(&self) -> bool {
        self.connected_at.is_some()
    }

    /// A renderer only known through a directory — never connected since
    /// this process started.
    pub fn known(renderer: &crate::directory::KnownRenderer) -> Self {
        Self {
            id: renderer.id.clone(),
            name: renderer.name.clone(),
            description: renderer.description.clone(),
            status: RendererStatus::error("not connected"),
            connected_at: None,
            disconnected_at: None,
            render_target: None,
            render_target_schema: None,
            custom_actions: None,
            render_characteristics: None,
            instances: Vec::new(),
            metrics: None,
        }
    }
}
