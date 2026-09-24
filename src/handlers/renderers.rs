use std::collections::HashMap;

use axum::{
    extract::{Path, Query, RawQuery, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    error::{AppError, Result},
    handlers::{api_key_from, authorize_target},
    models::{is_valid_renderer_name, Graphic, RenderTarget, RendererId, RendererInfo},
    renderer_ws,
    store::graphics::GraphicStore,
    AppState,
};

pub async fn list_renderers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    let api_key = api_key_from(&headers);
    let all = state.renderers.list_info().await;
    let visible = state.access.filter_visible(&api_key, all).await;
    let list: Vec<Value> = visible
        .iter()
        .map(|r| json!({ "id": r.id, "name": r.name, "renderTarget": r.render_target }))
        .collect();
    Ok(Json(json!({ "renderers": list })))
}

pub async fn get_renderer(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, &id).await?;
    let graphics = load_graphics_by_id(&state).await?;

    let render_target_schema = info
        .render_target_schema
        .clone()
        .unwrap_or_else(generic_render_target_schema);

    Ok(Json(json!({
        "renderer": {
            "id": info.id,
            "name": info.name,
            "status": { "status": "OK" },
            "renderTargetSchema": render_target_schema,
            "renderTargets": [target_info(&info, &graphics)],
        }
    })))
}

#[derive(Deserialize)]
pub struct TargetQuery {
    #[serde(rename = "renderTarget")]
    pub render_target: String,
}

pub async fn get_target(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    Query(query): Query<TargetQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, &id).await?;

    let requested: RenderTarget = serde_json::from_str(&query.render_target)
        .map_err(|e| AppError::BadRequest(format!("invalid renderTarget: {e}")))?;

    if requested != info.render_target {
        return Err(AppError::NotFound(format!(
            "renderTarget {requested:?} on renderer '{renderer_id}' (bound to {:?})",
            info.render_target
        )));
    }

    let graphics = load_graphics_by_id(&state).await?;
    Ok(Json(target_info(&info, &graphics)))
}

/// `zone`/`token` (or whatever else an `AccessControl` impl wants) travel in
/// the raw query string, unparsed by Core — see `access::AccessControl`'s
/// doc comment. `AllowAllAccessControl` ignores it entirely.
pub async fn connect_renderer(
    ws: axum::extract::WebSocketUpgrade,
    RawQuery(query): RawQuery,
    State(state): State<AppState>,
) -> Response {
    let query = query.unwrap_or_default();

    if !state.access.authorize_connect(&query).await {
        return AppError::Forbidden("not authorized to connect".into()).into_response();
    }

    ws.on_upgrade(move |socket| renderer_ws::handle_session(socket, state, query))
}

/// Validates and returns the renderer name from a URL path parameter.
/// In 0.4.0+, renderer IDs are names, not UUIDs. Returns 404 for invalid names
/// (spec: "No Renderer found"), not 400, since an invalid name can't exist.
pub(crate) fn parse_renderer_id(renderer_id: &str) -> Result<RendererId> {
    if is_valid_renderer_name(renderer_id) {
        Ok(renderer_id.to_string())
    } else {
        // Invalid name format → can't exist → 404, not 400
        Err(AppError::NotFound(format!("renderer '{renderer_id}'")))
    }
}

pub(crate) async fn load_graphics_by_id(state: &AppState) -> Result<HashMap<String, Graphic>> {
    let graphics = GraphicStore::new(&state.config.graphics_storage)
        .list_cached(state.config.graphics_cache_ttl())
        .await?;
    Ok(graphics.into_iter().map(|g| (g.id.clone(), g)).collect())
}

/// Fallback for renderers that didn't declare their own `renderTargetSchema`
/// at `hello` — "any shallow object" is the most permissive schema that's
/// still spec-valid, since we genuinely don't know this renderer's shape.
fn generic_render_target_schema() -> Value {
    json!({ "type": "object" })
}

pub(crate) fn target_info(info: &RendererInfo, graphics: &HashMap<String, Graphic>) -> Value {
    json!({
        "renderTarget": info.render_target,
        "name": info.name,
        "graphicInstances": info.instances.iter().map(|inst| json!({
            "graphicInstanceId": inst.instance_id,
            "graphic": graphics
                .get(&inst.graphic_id)
                .map(Graphic::list_info)
                .unwrap_or_else(|| json!({ "id": inst.graphic_id, "name": inst.graphic_id })),
            // Renderer-confirmed data/state as of the last successful action —
            // not part of the OGraf spec's GraphicInstance shape, but useful for
            // verifying a live round-trip without eyeballing the actual output.
            "data": inst.data,
            "state": inst.state,
            "currentStep": inst.current_step,
        })).collect::<Vec<_>>(),
    })
}
