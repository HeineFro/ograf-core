use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use std::sync::atomic::{AtomicI64, Ordering};

use chrono::Utc;

use crate::{
    error::{AppError, Result},
    handlers::api_key_from,
    models::Graphic,
    store::graphics::{is_valid_graphic_id, safe_join, GraphicStore},
    AppState,
};

// Optional access control via `filter_graphics` — the default
// `AllowAllAccessControl` returns all graphics (maintaining the original
// "unscoped by design" behavior), but implementations can restrict visibility
// based on zones, roles, or other policies.

pub async fn list_graphics(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    purge_deleted_graphics_throttled(&state).await;
    let api_key = api_key_from(&headers);
    let all_graphics = GraphicStore::new(&state.config.graphics_storage)
        .list()
        .await?;
    let visible = state.access.filter_graphics(&api_key, all_graphics).await;
    let list: Vec<Value> = visible.iter().map(Graphic::list_info).collect();
    Ok(Json(json!({ "graphics": list })))
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    pub force: bool,
}

/// The spec's `DELETE /graphics/{graphicId}?force=` — see
/// [`GraphicStore::delete`] for what `force` changes.
pub async fn delete_graphic(
    State(state): State<AppState>,
    Path(graphic_id): Path<String>,
    Query(query): Query<DeleteQuery>,
    headers: HeaderMap,
) -> Result<Json<Value>> {
    let api_key = api_key_from(&headers);
    if !state.access.can_delete_graphic(&api_key, &graphic_id).await {
        return Err(AppError::Forbidden(format!(
            "not authorized to delete graphic '{graphic_id}'"
        )));
    }
    GraphicStore::new(&state.config.graphics_storage)
        .delete(&graphic_id, query.force)
        .await?;
    purge_deleted_graphics(&state).await;
    Ok(Json(json!({})))
}

/// Removes graphics whose retention has passed — run from `DELETE` and, at
/// most once a minute, from `GET /graphics`, so Core needs no background task.
async fn purge_deleted_graphics(state: &AppState) {
    let renderers = state.renderers.clone();
    GraphicStore::new(&state.config.graphics_storage)
        .purge_deleted(state.config.deleted_graphic_retention(), |id| {
            renderers.graphic_in_use(id)
        })
        .await;
}

async fn purge_deleted_graphics_throttled(state: &AppState) {
    static LAST_PURGE: AtomicI64 = AtomicI64::new(0);
    let now = Utc::now().timestamp();
    let last = LAST_PURGE.load(Ordering::Relaxed);
    if now - last >= 60
        && LAST_PURGE
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        purge_deleted_graphics(state).await;
    }
}

pub async fn get_graphic(
    State(state): State<AppState>,
    Path(graphic_id): Path<String>,
) -> Result<Json<Value>> {
    let graphic = GraphicStore::new(&state.config.graphics_storage)
        .get(&graphic_id)
        .await?;
    Ok(Json(json!({
        "graphic": graphic.manifest,
        "metadata": {
            "createdAt": graphic.uploaded_at,
        }
    })))
}

#[derive(Deserialize)]
pub struct ThumbnailQuery {
    pub file: String,
}

pub async fn get_thumbnail(
    State(state): State<AppState>,
    Path(graphic_id): Path<String>,
    Query(query): Query<ThumbnailQuery>,
) -> Response {
    let not_found = || AppError::NotFound(format!("thumbnail '{}' of graphic '{graphic_id}'", query.file)).into_response();
    if !is_valid_graphic_id(&graphic_id) {
        return not_found();
    }

    let mime = mime_guess::from_path(&query.file).first_or_octet_stream();
    let is_supported = matches!(
        mime.essence_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    );
    if !is_supported {
        return AppError::BadRequest("unsupported thumbnail file type".into()).into_response();
    }

    let storage_path = GraphicStore::new(&state.config.graphics_storage).path_for(&graphic_id);
    let Some(file_path) = safe_join(&storage_path, &query.file) else {
        return AppError::BadRequest("invalid thumbnail file reference".into()).into_response();
    };

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, mime.to_string())],
            bytes,
        )
            .into_response(),
        Err(_) => not_found(),
    }
}

pub async fn serve_graphic_asset(
    State(state): State<AppState>,
    Path((graphic_id, asset_path)): Path<(String, String)>,
) -> Response {
    let storage_path = match GraphicStore::new(&state.config.graphics_storage)
        .get_for_assets(&graphic_id)
        .await
    {
        Ok(graphic) => graphic.storage_path,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let Some(file_path) = safe_join(std::path::Path::new(&storage_path), &asset_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(&asset_path)
                .first_or_octet_stream()
                .to_string();
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime)],
                bytes,
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
