use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

pub type RendererId = Uuid;

// Re-export protocol types for backward compatibility
pub use crate::protocol::{
    InstanceId, InstanceSnapshot, RenderTarget, RendererMessage, ServerMessage, WsMessage,
};

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
