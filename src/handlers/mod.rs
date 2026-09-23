pub mod actions;
pub mod graphics;
pub mod renderers;

use axum::{http::HeaderMap, Json};
use serde_json::{json, Value};

use crate::{
    error::{AppError, Result},
    models::{RendererId, RendererInfo},
    AppState,
};

pub async fn server_info() -> Json<Value> {
    Json(json!({
        "name": "OGraf Server",
        "description": "OGraf-compatible graphics server",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Health check endpoint for liveness/readiness probes — always returns 200
/// OK with `{"status": "ok"}`. No authentication required (by design: a load
/// balancer or orchestrator needs to check this without credentials).
pub async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok"
    }))
}

/// The vendor/controller API key from `X-OGraf-Key`, or empty if absent —
/// an `AccessControl` implementation decides what an empty key means
/// (`AllowAllAccessControl` doesn't care; other implementations may reject
/// empty or unrecognized keys).
pub(crate) fn api_key_from(headers: &HeaderMap) -> String {
    headers
        .get("X-OGraf-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// Shared by every renderer-scoped handler in `renderers.rs`/`actions.rs`:
/// fetches the renderer's current info and checks `can_target` before
/// letting the caller touch it — Tell Don't Ask, callers get a yes/no
/// answer instead of reaching into the registry themselves to decide.
///
/// In 0.4.0+, renderer_id is the renderer's name (RendererId = String).
pub(crate) async fn authorize_target(
    state: &AppState,
    headers: &HeaderMap,
    renderer_id: &RendererId,
) -> Result<RendererInfo> {
    let info = state.renderers.get_info(renderer_id).await?;
    let api_key = api_key_from(headers);
    if state.access.can_target(&api_key, &info.name).await {
        Ok(info)
    } else {
        Err(AppError::Forbidden(format!(
            "no access to renderer '{}'",
            info.name
        )))
    }
}
