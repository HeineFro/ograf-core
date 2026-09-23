use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Renderer identifier - the renderer's stable name.
/// Changed from Uuid in 0.4.0 for spec compliance and stability across reconnects.
pub type RendererId = String;

/// Validates a renderer name for use as RendererId.
///
/// Valid names: 1-64 characters, A-Z a-z 0-9 - _ .
/// Case-sensitive, needs no URL escaping in path segments.
///
/// # Examples
/// ```
/// use ograf_core::models::is_valid_renderer_name;
///
/// assert!(is_valid_renderer_name("main-output"));
/// assert!(is_valid_renderer_name("Renderer_1.backup"));
/// assert!(!is_valid_renderer_name(""));  // too short
/// assert!(!is_valid_renderer_name("renderer with spaces"));  // invalid chars
/// ```
pub fn is_valid_renderer_name(name: &str) -> bool {
    let len = name.len();
    len >= 1
        && len <= 64
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererInfo {
    pub id: RendererId,
    pub name: String,
    pub connected_at: DateTime<Utc>,
    pub render_target: RenderTarget,
    /// The renderer's own declared schema for `render_target`, from
    /// `hello.capabilities.renderTargetSchema` — `None` if it didn't send
    /// one, in which case callers fall back to a generic placeholder.
    pub render_target_schema: Option<Value>,
    pub instances: Vec<GraphicInstance>,
    /// Optional metrics for observability (extension, not part of OGraf spec).
    /// Only populated if metrics tracking is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<RendererMetrics>,
}
