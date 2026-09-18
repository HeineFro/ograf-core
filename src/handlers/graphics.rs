use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    error::Result,
    models::Graphic,
    store::graphics::{is_valid_graphic_id, safe_join, GraphicStore},
    AppState,
};

// Unscoped by design (Ograf-v2.md §2/§4): automation loads a known
// `graphicId` from its own config, never browses a list, and an operator's
// controller is tightly coupled to specific templates — hiding templates
// from a listing wouldn't be a real access boundary. The real one is
// `can_target` on the renderer a graphic gets loaded onto.

pub async fn list_graphics(State(state): State<AppState>) -> Result<Json<Value>> {
    let graphics = GraphicStore::new(&state.config.graphics_storage).list().await?;
    let list: Vec<Value> = graphics.iter().map(Graphic::list_info).collect();
    Ok(Json(json!({ "graphics": list })))
}

pub async fn get_graphic(
    State(state): State<AppState>,
    Path(graphic_id): Path<String>,
) -> Result<Json<Value>> {
    let graphic = GraphicStore::new(&state.config.graphics_storage).get(&graphic_id).await?;
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
    if !is_valid_graphic_id(&graphic_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mime = mime_guess::from_path(&query.file).first_or_octet_stream();
    let is_supported = matches!(
        mime.essence_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    );
    if !is_supported {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported thumbnail file type" })),
        )
            .into_response();
    }

    let storage_path = GraphicStore::new(&state.config.graphics_storage).path_for(&graphic_id);
    let Some(file_path) = safe_join(&storage_path, &query.file) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid thumbnail file reference" })),
        )
            .into_response();
    };

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, mime.to_string())],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn serve_graphic_asset(
    State(state): State<AppState>,
    Path((graphic_id, asset_path)): Path<(String, String)>,
) -> Response {
    let storage_path = match GraphicStore::new(&state.config.graphics_storage).get(&graphic_id).await {
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
