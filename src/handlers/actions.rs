use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    handlers::{api_key_from, authorize_target, renderers::parse_renderer_id},
    models::{GraphicInstance, InstanceId, RenderTarget, RendererMessage, ServerMessage},
    store::graphics::GraphicStore,
    AppState,
};

fn ensure_target_matches(
    renderer_id: &str,
    expected: &RenderTarget,
    requested: &RenderTarget,
) -> Result<()> {
    if expected != requested {
        return Err(AppError::NotFound(format!(
            "renderTarget {requested:?} not found on renderer '{renderer_id}' (bound to {expected:?})"
        )));
    }
    Ok(())
}

fn ensure_instance_exists(instances: &[GraphicInstance], instance_id: InstanceId) -> Result<()> {
    if instances.iter().any(|i| i.instance_id == instance_id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!(
            "graphicInstance '{instance_id}'"
        )))
    }
}

fn action_result(
    instance_id: InstanceId,
    status_code: u16,
    status_message: Option<String>,
    current_step: Option<f64>,
) -> Result<Json<Value>> {
    if status_code >= 400 {
        return Err(AppError::GraphicAction {
            status_code,
            message: status_message.unwrap_or_else(|| "graphic action failed".into()),
        });
    }

    let mut body = json!({ "graphicInstanceId": instance_id, "statusCode": status_code });
    if let Some(msg) = status_message {
        body["statusMessage"] = json!(msg);
    }
    if let Some(step) = current_step {
        body["currentStep"] = json!(step);
    }
    Ok(Json(body))
}

fn unexpected_reply() -> AppError {
    AppError::Internal(anyhow::anyhow!("renderer sent an unexpected reply type"))
}

// ---------------- Load ----------------

#[derive(Deserialize)]
pub struct LoadParams {
    pub data: Option<Value>,
}

#[derive(Deserialize)]
pub struct LoadRequest {
    #[serde(rename = "renderTarget")]
    pub render_target: RenderTarget,
    #[serde(rename = "graphicId")]
    pub graphic_id: String,
    pub params: LoadParams,
}

pub async fn load(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<LoadRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, id).await?;
    ensure_target_matches(&renderer_id, &info.render_target, &body.render_target)?;

    // 404s if the graphic doesn't exist, per spec.
    GraphicStore::new(&state.config.graphics_storage)
        .get(&body.graphic_id)
        .await?;

    // Check per-graphic load authorization (Wish 2: can_load_graphic)
    let api_key = api_key_from(&headers);
    if !state.access.can_load_graphic(&api_key, &info.name, &body.graphic_id).await {
        return Err(AppError::Forbidden(format!(
            "not authorized to load graphic '{}' on renderer '{}'",
            body.graphic_id, info.name
        )));
    }

    let instance_id = Uuid::new_v4();
    let graphic_id = body.graphic_id;
    let data = body.params.data;

    let reply = state
        .renderers
        .send_and_await(
            id,
            move |request_id| ServerMessage::Load {
                request_id,
                instance_id,
                graphic_id,
                data,
            },
            state.config.action_timeout(),
        )
        .await?;

    match reply {
        RendererMessage::LoadResult {
            instance_id,
            status_code,
            status_message,
            ..
        } => action_result(instance_id, status_code, status_message, None),
        _ => Err(unexpected_reply()),
    }
}

// ---------------- PlayAction ----------------

#[derive(Deserialize)]
pub struct PlayActionParams {
    pub delta: Option<f64>,
    pub goto: Option<f64>,
    #[serde(rename = "skipAnimation")]
    pub skip_animation: Option<bool>,
}

#[derive(Deserialize)]
pub struct PlayActionRequest {
    #[serde(rename = "renderTarget")]
    pub render_target: RenderTarget,
    #[serde(rename = "graphicInstanceId")]
    pub graphic_instance_id: InstanceId,
    pub params: PlayActionParams,
}

pub async fn play_action(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PlayActionRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, id).await?;
    ensure_target_matches(&renderer_id, &info.render_target, &body.render_target)?;
    ensure_instance_exists(&info.instances, body.graphic_instance_id)?;

    let instance_id = body.graphic_instance_id;
    let goto = body.params.goto;
    let delta = body.params.delta;
    let skip_animation = body.params.skip_animation;

    let reply = state
        .renderers
        .send_and_await(
            id,
            move |request_id| ServerMessage::PlayAction {
                request_id,
                instance_id,
                goto,
                delta,
                skip_animation,
            },
            state.config.action_timeout(),
        )
        .await?;

    match reply {
        RendererMessage::PlayActionResult {
            instance_id,
            status_code,
            status_message,
            current_step,
            ..
        } => action_result(instance_id, status_code, status_message, Some(current_step)),
        _ => Err(unexpected_reply()),
    }
}

// ---------------- StopAction ----------------

#[derive(Deserialize)]
pub struct StopActionParams {
    #[serde(rename = "skipAnimation")]
    pub skip_animation: Option<bool>,
}

#[derive(Deserialize)]
pub struct StopActionRequest {
    #[serde(rename = "renderTarget")]
    pub render_target: RenderTarget,
    #[serde(rename = "graphicInstanceId")]
    pub graphic_instance_id: InstanceId,
    pub params: StopActionParams,
}

pub async fn stop_action(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<StopActionRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, id).await?;
    ensure_target_matches(&renderer_id, &info.render_target, &body.render_target)?;
    ensure_instance_exists(&info.instances, body.graphic_instance_id)?;

    let instance_id = body.graphic_instance_id;
    let skip_animation = body.params.skip_animation;

    let reply = state
        .renderers
        .send_and_await(
            id,
            move |request_id| ServerMessage::StopAction {
                request_id,
                instance_id,
                skip_animation,
            },
            state.config.action_timeout(),
        )
        .await?;

    match reply {
        RendererMessage::StopActionResult {
            instance_id,
            status_code,
            status_message,
            ..
        } => action_result(instance_id, status_code, status_message, None),
        _ => Err(unexpected_reply()),
    }
}

// ---------------- UpdateAction ----------------

#[derive(Deserialize)]
pub struct UpdateActionParams {
    pub data: Value,
    #[serde(rename = "skipAnimation")]
    pub skip_animation: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateActionRequest {
    #[serde(rename = "renderTarget")]
    pub render_target: RenderTarget,
    #[serde(rename = "graphicInstanceId")]
    pub graphic_instance_id: InstanceId,
    pub params: UpdateActionParams,
}

pub async fn update_action(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateActionRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, id).await?;
    ensure_target_matches(&renderer_id, &info.render_target, &body.render_target)?;
    ensure_instance_exists(&info.instances, body.graphic_instance_id)?;

    let instance_id = body.graphic_instance_id;
    let data = body.params.data;
    let skip_animation = body.params.skip_animation;

    let reply = state
        .renderers
        .send_and_await(
            id,
            move |request_id| ServerMessage::UpdateAction {
                request_id,
                instance_id,
                data,
                skip_animation,
            },
            state.config.action_timeout(),
        )
        .await?;

    match reply {
        RendererMessage::UpdateActionResult {
            instance_id,
            status_code,
            status_message,
            ..
        } => action_result(instance_id, status_code, status_message, None),
        _ => Err(unexpected_reply()),
    }
}

// ---------------- CustomAction (instance-scoped) ----------------

#[derive(Deserialize)]
pub struct CustomActionParams {
    pub payload: Value,
    #[serde(rename = "skipAnimation")]
    pub skip_animation: Option<bool>,
}

#[derive(Deserialize)]
pub struct CustomActionRequest {
    #[serde(rename = "renderTarget")]
    pub render_target: RenderTarget,
    #[serde(rename = "graphicInstanceId")]
    pub graphic_instance_id: InstanceId,
    pub params: CustomActionParams,
}

pub async fn custom_action(
    State(state): State<AppState>,
    Path((renderer_id, action_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<CustomActionRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, id).await?;
    ensure_target_matches(&renderer_id, &info.render_target, &body.render_target)?;
    ensure_instance_exists(&info.instances, body.graphic_instance_id)?;

    let instance_id = body.graphic_instance_id;
    let payload = body.params.payload;
    let skip_animation = body.params.skip_animation;

    let reply = state
        .renderers
        .send_and_await(
            id,
            move |request_id| ServerMessage::CustomAction {
                request_id,
                instance_id,
                action_id,
                payload,
                skip_animation,
            },
            state.config.action_timeout(),
        )
        .await?;

    match reply {
        RendererMessage::CustomActionResult {
            instance_id,
            status_code,
            status_message,
            ..
        } => action_result(instance_id, status_code, status_message, None),
        _ => Err(unexpected_reply()),
    }
}

// ---------------- CustomAction (renderer-scoped) ----------------

#[derive(Deserialize)]
pub struct RendererCustomActionRequest {
    pub payload: Value,
    #[serde(rename = "skipAnimation")]
    pub skip_animation: Option<bool>,
}

pub async fn renderer_custom_action(
    State(state): State<AppState>,
    Path((renderer_id, action_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<RendererCustomActionRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    // Ensures a clear 404/403 if the renderer id is unknown or off-limits.
    authorize_target(&state, &headers, id).await?;

    let payload = body.payload;
    let skip_animation = body.skip_animation;

    let reply = state
        .renderers
        .send_and_await(
            id,
            move |request_id| ServerMessage::RendererCustomAction {
                request_id,
                action_id,
                payload,
                skip_animation,
            },
            state.config.action_timeout(),
        )
        .await?;

    match reply {
        RendererMessage::RendererCustomActionResult {
            status_code,
            result,
            ..
        } if status_code < 400 => Ok(Json(json!({ "result": result }))),
        RendererMessage::RendererCustomActionResult {
            status_code,
            status_message,
            ..
        } => Err(AppError::GraphicAction {
            status_code,
            message: status_message.unwrap_or_else(|| "custom action failed".into()),
        }),
        _ => Err(unexpected_reply()),
    }
}

// ---------------- Clear ----------------

#[derive(Deserialize)]
pub struct GraphicFilter {
    #[serde(rename = "renderTarget")]
    pub render_target: Option<RenderTarget>,
    #[serde(rename = "graphicId")]
    pub graphic_id: Option<String>,
    #[serde(rename = "graphicInstanceId")]
    pub graphic_instance_id: Option<InstanceId>,
}

impl GraphicFilter {
    fn matches(&self, target: &RenderTarget, instance: &GraphicInstance) -> bool {
        self.render_target.as_ref().map_or(true, |rt| rt == target)
            && self
                .graphic_id
                .as_deref()
                .map_or(true, |gid| gid == instance.graphic_id)
            && self
                .graphic_instance_id
                .map_or(true, |iid| iid == instance.instance_id)
    }
}

#[derive(Deserialize)]
pub struct ClearRequest {
    pub filters: Vec<GraphicFilter>,
}

pub async fn clear(
    State(state): State<AppState>,
    Path(renderer_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ClearRequest>,
) -> Result<Json<Value>> {
    let id = parse_renderer_id(&renderer_id)?;
    let info = authorize_target(&state, &headers, id).await?;
    let target = info.render_target;

    let to_clear: Vec<InstanceId> = info
        .instances
        .iter()
        .filter(|inst| body.filters.is_empty() || body.filters.iter().any(|f| f.matches(&target, inst)))
        .map(|inst| inst.instance_id)
        .collect();

    let mut cleared = Vec::new();
    for instance_id in to_clear {
        let reply = state
            .renderers
            .send_and_await(
                id,
                move |request_id| ServerMessage::Clear {
                    request_id,
                    instance_id,
                },
                state.config.action_timeout(),
            )
            .await;

        if let Ok(RendererMessage::ClearResult { status_code, .. }) = reply {
            if status_code < 400 {
                cleared.push(json!({ "renderTarget": target, "graphicInstanceId": instance_id }));
            }
        }
    }

    Ok(Json(json!({ "graphicInstances": cleared })))
}
