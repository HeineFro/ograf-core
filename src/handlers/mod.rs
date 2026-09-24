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

/// The spec's server info, taken from Core's own `Cargo.toml` so there's one
/// place to maintain it and `version` always follows the crate.
pub async fn server_info() -> Json<Value> {
    Json(json!({
        "name": env!("CARGO_PKG_NAME"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "author": author(env!("CARGO_PKG_AUTHORS"), env!("CARGO_PKG_REPOSITORY")),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// The spec's `Author` (`name` required, `email`/`url` optional) from Cargo's
/// `authors` — `"Name <email>"` entries joined by `:` — using the first one.
fn author(cargo_authors: &str, repository: &str) -> Value {
    let first = cargo_authors.split(':').next().unwrap_or_default().trim();
    let (name, email) = match first.split_once('<') {
        Some((name, rest)) => (name.trim(), rest.trim_end_matches('>').trim()),
        None => (first, ""),
    };

    let mut author = json!({ "name": name });
    if !email.is_empty() {
        author["email"] = json!(email);
    }
    if !repository.is_empty() {
        author["url"] = json!(repository);
    }
    author
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
    let info = find_renderer(state, renderer_id).await?;
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

/// Connected or recently disconnected (the registry), else only known
/// through the consumer's [`RendererDirectory`](crate::directory::RendererDirectory).
pub(crate) async fn find_renderer(state: &AppState, renderer_id: &str) -> Result<RendererInfo> {
    if let Ok(info) = state.renderers.get_info(renderer_id).await {
        return Ok(info);
    }
    state
        .directory
        .known_renderers()
        .await
        .iter()
        .find(|known| known.id == renderer_id)
        .map(RendererInfo::known)
        .ok_or_else(|| AppError::NotFound(format!("renderer '{renderer_id}'")))
}

/// Actions need a live session — a known but offline renderer is the spec's
/// "error comes from the Renderer", made specific as 503.
pub(crate) fn ensure_connected(info: &RendererInfo) -> Result<()> {
    if info.is_connected() {
        Ok(())
    } else {
        Err(AppError::RendererNotConnected(info.id.clone()))
    }
}
