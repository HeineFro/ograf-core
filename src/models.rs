use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

pub type RendererId = Uuid;
pub type InstanceId = Uuid;

/// Accepts whatever JSON value shows up and coerces it to a step number,
/// defaulting to 0.0 rather than failing — see `RendererMessage::PlayActionResult`.
fn lenient_f64<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Value::deserialize(deserializer)?.as_f64().unwrap_or(0.0))
}

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

/// Identifies a RenderTarget on a Renderer. Per the OGraf spec, its shape is
/// defined by each Renderer's own `renderTargetSchema` — Core treats it as
/// an opaque, shallow JSON object and only ever compares it for equality.
/// For example, a CasparCG renderer might use `{channel, layer}`, fixed for
/// the lifetime of one WS connection; other renderer types (eg a file-render
/// worker) are free to use a different shape (eg `{profile: "16:9-1080p"}`).
pub type RenderTarget = Value;

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
}

/// Messages sent from the server to a renderer over its WebSocket. Every
/// variant carries a `request_id` so the HTTP handler that triggered it can
/// correlate the renderer's eventual `*Result` reply (see RendererMessage)
/// and return the real statusCode/statusMessage, per the OGraf spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsMessage {
    Load {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "graphicId")]
        graphic_id: String,
        data: Option<Value>,
    },
    PlayAction {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(skip_serializing_if = "Option::is_none")]
        goto: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<f64>,
        #[serde(rename = "skipAnimation", skip_serializing_if = "Option::is_none")]
        skip_animation: Option<bool>,
    },
    StopAction {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "skipAnimation", skip_serializing_if = "Option::is_none")]
        skip_animation: Option<bool>,
    },
    UpdateAction {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        data: Value,
        #[serde(rename = "skipAnimation", skip_serializing_if = "Option::is_none")]
        skip_animation: Option<bool>,
    },
    CustomAction {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "actionId")]
        action_id: String,
        payload: Value,
        #[serde(rename = "skipAnimation", skip_serializing_if = "Option::is_none")]
        skip_animation: Option<bool>,
    },
    /// A custom action invoked on the Renderer itself, not on a GraphicInstance.
    RendererCustomAction {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "actionId")]
        action_id: String,
        payload: Value,
        #[serde(rename = "skipAnimation", skip_serializing_if = "Option::is_none")]
        skip_animation: Option<bool>,
    },
    Clear {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
    },
}

/// Messages received from a renderer over its WebSocket.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RendererMessage {
    Hello {
        name: String,
        #[serde(rename = "renderTarget")]
        render_target: Value,
        #[serde(default)]
        capabilities: Value,
    },
    Ping,
    LoadResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "graphicId")]
        graphic_id: String,
        #[serde(default)]
        data: Option<Value>,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
    },
    PlayActionResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
        // Lenient on purpose: `currentStep` is whatever a template's own
        // playAction() happened to return — a template that returns something
        // non-numeric shouldn't sink the *entire* message (and hang the HTTP
        // caller for the full timeout) just because of this one field.
        #[serde(rename = "currentStep", default, deserialize_with = "lenient_f64")]
        current_step: f64,
    },
    StopActionResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
    },
    UpdateActionResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(default)]
        data: Option<Value>,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
    },
    CustomActionResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
    },
    RendererCustomActionResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
        #[serde(default)]
        result: Option<Value>,
    },
    ClearResult {
        #[serde(rename = "requestId")]
        request_id: Uuid,
        #[serde(rename = "instanceId")]
        instance_id: InstanceId,
        #[serde(rename = "statusCode")]
        status_code: u16,
        #[serde(rename = "statusMessage", default)]
        status_message: Option<String>,
    },
}

impl RendererMessage {
    /// The correlation id this message is a reply to, if any (`Hello`/`Ping`
    /// carry none since nothing on the server side is awaiting them).
    pub fn request_id(&self) -> Option<Uuid> {
        match self {
            RendererMessage::LoadResult { request_id, .. }
            | RendererMessage::PlayActionResult { request_id, .. }
            | RendererMessage::StopActionResult { request_id, .. }
            | RendererMessage::UpdateActionResult { request_id, .. }
            | RendererMessage::CustomActionResult { request_id, .. }
            | RendererMessage::RendererCustomActionResult { request_id, .. }
            | RendererMessage::ClearResult { request_id, .. } => Some(*request_id),
            RendererMessage::Hello { .. } | RendererMessage::Ping => None,
        }
    }

    /// Whether this result message indicates success (status_code < 400).
    /// Returns `false` for Hello/Ping which carry no status code.
    pub fn is_success(&self) -> bool {
        match self {
            RendererMessage::LoadResult { status_code, .. }
            | RendererMessage::PlayActionResult { status_code, .. }
            | RendererMessage::StopActionResult { status_code, .. }
            | RendererMessage::UpdateActionResult { status_code, .. }
            | RendererMessage::CustomActionResult { status_code, .. }
            | RendererMessage::RendererCustomActionResult { status_code, .. }
            | RendererMessage::ClearResult { status_code, .. } => *status_code < 400,
            RendererMessage::Hello { .. } | RendererMessage::Ping => false,
        }
    }
}
