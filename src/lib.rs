//! The pure OGraf-spec Core server: graphics, renderers, actions, and the
//! renderer WebSocket protocol. Knows nothing about zones, API keys, or
//! admin accounts — every access decision is delegated to whatever
//! [`access::AccessControl`] implementation the binary wires in (Dependency
//! Inversion principle). A consumer that wants no access control at all can
//! use [`access::AllowAllAccessControl`].

pub mod access;
pub mod config;
pub mod directory;
pub mod error;
pub mod handlers;
pub mod models;
pub mod protocol;
pub mod renderer_ws;
pub mod store;

use std::sync::Arc;

use access::AccessControl;
use config::Config;
use directory::{NoDirectory, RendererDirectory};
use store::renderers::RendererRegistry;

#[derive(Clone)]
#[non_exhaustive]
pub struct AppState {
    pub config: Arc<Config>,
    pub renderers: Arc<RendererRegistry>,
    pub access: Arc<dyn AccessControl>,
    pub directory: Arc<dyn RendererDirectory>,
}

impl AppState {
    /// With [`NoDirectory`] — see [`AppState::with_directory`] to list
    /// renderers from a consumer's own storage too.
    pub fn new(
        config: Arc<Config>,
        renderers: Arc<RendererRegistry>,
        access: Arc<dyn AccessControl>,
    ) -> Self {
        Self { config, renderers, access, directory: Arc::new(NoDirectory) }
    }

    pub fn with_directory(mut self, directory: Arc<dyn RendererDirectory>) -> Self {
        self.directory = directory;
        self
    }
}

pub use axum::Router;

/// Where renderers open their WebSocket (`GET`, upgraded). Outside
/// `/ograf/v1` on purpose, see `build_router`.
pub const RENDERER_CONNECT_PATH: &str = "/rendererApi/v1/connect";

/// Builds the `/ograf/v1/*` API router plus the internal graphic-asset route
/// used by the renderer HTML — everything a consumer needs to nest under its
/// own top-level router alongside its own admin routes. Static file serving
/// (`/renderer`, admin UI) and CORS/tracing layers are the binary's own
/// concern, not Core's.
pub fn build_router(state: AppState) -> Router {
    use axum::routing::{get, post, put};

    let api = Router::new()
        .route("/", get(handlers::server_info))
        .route("/health", get(handlers::health))
        .route("/graphics", get(handlers::graphics::list_graphics))
        .route("/graphics/:id", get(handlers::graphics::get_graphic))
        .route(
            "/graphics/:id/assets/*path",
            get(handlers::graphics::serve_graphic_asset),
        )
        .route(
            "/graphics/:id/thumbnail",
            get(handlers::graphics::get_thumbnail),
        )
        .route("/renderers", get(handlers::renderers::list_renderers))
        .route("/renderers/:id", get(handlers::renderers::get_renderer))
        .route(
            "/renderers/:id/target",
            get(handlers::renderers::get_target),
        )
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
        // The spec's server info is `/` under `/ograf/v1`, i.e. `/ograf/v1/`
        // — `nest` only matches its `/` route without the trailing slash.
        .route("/ograf/v1/", get(handlers::server_info))
        // The renderer WebSocket protocol isn't part of the OGraf Server API,
        // so it lives outside `/ograf/v1` — under `/ograf/v1/renderers/` it
        // shadowed `GET /renderers/{rendererId}` for a renderer named
        // `connect`. Versioned on its own, since the protocol changes
        // independently of the Server API.
        .route(
            RENDERER_CONNECT_PATH,
            get(handlers::renderers::connect_renderer),
        )
        .route(
            "/serverApi/internal/graphics/:graphic_id/*path",
            get(handlers::graphics::serve_graphic_asset),
        )
        .with_state(state)
}
