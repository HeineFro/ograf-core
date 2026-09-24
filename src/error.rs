use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Renderer not connected: {0}")]
    RendererNotConnected(String),
    #[error("Renderer did not respond in time: {0}")]
    Timeout(String),
    #[error("Renderer overloaded: {0}")]
    RendererOverloaded(String),
    /// The renderer answered, but with a statusCode indicating the action
    /// failed (eg 550 when a GraphicInstance's own action method threw).
    #[error("Graphic action failed ({status_code}): {message}")]
    GraphicAction { status_code: u16, message: String },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::RendererNotConnected(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
            }
            AppError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, self.to_string()),
            AppError::RendererOverloaded(_) => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            AppError::GraphicAction { status_code, .. } => {
                let status =
                    StatusCode::from_u16(*status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, self.to_string())
            }
            AppError::Internal(err) => {
                // Log full error internally for debugging
                tracing::error!("Internal server error: {err:?}");
                // Return generic message to user (don't leak file paths, etc.)
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        problem(status, message).into_response()
    }
}

/// The spec's `ErrorResponse`: RFC 7807 problem details (`title`, `status`,
/// `detail`), plus `error` — the only field before 0.6.0 — so existing
/// clients keep working.
fn problem(status: StatusCode, detail: String) -> (StatusCode, Json<serde_json::Value>) {
    // 550 (a GraphicInstance's own method threw) has no standard reason.
    let title = status.canonical_reason().unwrap_or(match status.as_u16() {
        550 => "Graphic Action Failed",
        _ => "Error",
    });
    let body = json!({
        "title": title,
        "status": status.as_u16(),
        "detail": detail,
        "error": detail,
    });
    (status, Json(body))
}

pub type Result<T> = std::result::Result<T, AppError>;
