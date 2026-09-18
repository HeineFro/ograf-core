//! The pure OGraf-spec Core server: graphics, renderers, actions, and the
//! renderer WebSocket protocol. Knows nothing about zones, API keys, or
//! admin accounts — every access decision is delegated to whatever
//! [`access::AccessControl`] implementation the binary wires in (Dependency
//! Inversion, see Ograf-v2.md §1). A consumer that wants no access control
//! at all can use [`access::AllowAllAccessControl`].

pub mod access;
pub mod config;
pub mod error;
pub mod handlers;
pub mod models;
pub mod renderer_ws;
pub mod store;

use std::sync::Arc;

use access::AccessControl;
use config::Config;
use store::renderers::RendererRegistry;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub renderers: Arc<RendererRegistry>,
    pub access: Arc<dyn AccessControl>,
}

pub use axum::Router;

/// Builds the `/ograf/v1/*` API router plus the internal graphic-asset route
/// used by the renderer HTML — everything a consumer needs to nest under its
/// own top-level router alongside its own admin routes. Static file serving
/// (`/renderer`, admin UI) and CORS/tracing layers are the binary's own
/// concern, not Core's.
pub fn build_router(state: AppState) -> Router {
    use axum::routing::{get, post, put};

    let api = Router::new()
        .route("/", get(handlers::server_info))
        .route("/graphics", get(handlers::graphics::list_graphics))
        .route("/graphics/:id", get(handlers::graphics::get_graphic))
        .route("/graphics/:id/assets/*path", get(handlers::graphics::serve_graphic_asset))
        .route("/graphics/:id/thumbnail", get(handlers::graphics::get_thumbnail))
        .route("/renderers/connect", get(handlers::renderers::connect_renderer))
        .route("/renderers", get(handlers::renderers::list_renderers))
        .route("/renderers/:id", get(handlers::renderers::get_renderer))
        .route("/renderers/:id/target", get(handlers::renderers::get_target))
        .route(
            "/renderers/:id/customActions/:action_id",
            post(handlers::actions::renderer_custom_action),
        )
        .route(
            "/renderers/:id/target/graphicInstance/clear",
            put(handlers::actions::clear),
        )
        .route(
            "/renderers/:id/target/graphicInstance/load",
            post(handlers::actions::load),
        )
        .route(
            "/renderers/:id/target/graphicInstance/playAction",
            post(handlers::actions::play_action),
        )
        .route(
            "/renderers/:id/target/graphicInstance/stopAction",
            post(handlers::actions::stop_action),
        )
        .route(
            "/renderers/:id/target/graphicInstance/updateAction",
            post(handlers::actions::update_action),
        )
        .route(
            "/renderers/:id/target/graphicInstance/customActions/:action_id",
            post(handlers::actions::custom_action),
        );

    Router::new()
        .nest("/ograf/v1", api)
        .route(
            "/serverApi/internal/graphics/:graphic_id/*path",
            get(handlers::graphics::serve_graphic_asset),
        )
        .with_state(state)
}
