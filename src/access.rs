//! Dependency Inversion seam: Core defines what access control it needs,
//! never how it's decided. Consumers can provide their own implementation;
//! [`AllowAllAccessControl`] is a trivial one for anyone who wants no
//! restriction at all.

use async_trait::async_trait;

use crate::models::{Graphic, RendererInfo};

#[async_trait]
pub trait AccessControl: Send + Sync {
    /// Authorizes a renderer's WebSocket connection *before* it's upgraded.
    /// `query` is the connect URL's raw query string, unparsed and
    /// unvalidated by Core — an implementation decides what it means (a
    /// zone name + token for `ZoneAccessControl`, ignored entirely here).
    async fn authorize_connect(&self, query: &str) -> bool;

    /// Authorizes the renderer name from `hello`, before the session is registered.
    /// Called after `authorize_connect` and after name validation, but before
    /// attempting to register the session. Returning `false` closes the connection
    /// and never registers it.
    ///
    /// Default implementation allows all names (backward compatible). Implementations
    /// can enforce policies like "name must match query string" or "name must be
    /// authorized for this zone".
    ///
    /// Added in 0.4.0 to prevent name squatting (connecting with valid query but
    /// claiming another zone's renderer name in `hello`).
    async fn authorize_name(&self, _name: &str, _query: &str) -> bool {
        true
    }

    /// Called once a renderer's `hello` names it, after `authorize_connect`
    /// already approved the connection — a chance to record durable identity
    /// for tracking or failover purposes. Best-effort: Core doesn't drop the
    /// connection if this does nothing.
    async fn on_renderer_connected(&self, name: &str, query: &str);

    /// Filters `renderers` down to what `api_key` may see — `GET /renderers`.
    async fn filter_visible(
        &self,
        api_key: &str,
        renderers: Vec<RendererInfo>,
    ) -> Vec<RendererInfo>;

    /// Filters `graphics` down to what `api_key` may see — `GET /graphics`.
    /// Default implementation returns all graphics (no filtering), maintaining
    /// backward compatibility and the original "unscoped by design" behavior.
    async fn filter_graphics(&self, _api_key: &str, graphics: Vec<Graphic>) -> Vec<Graphic> {
        graphics
    }

    /// Whether `api_key` may target the renderer named `renderer_name` —
    /// checked before every renderer-scoped call (get/target/load/play/
    /// stop/update/customAction/clear). Keyed on the stable *name*, not the
    /// per-session `id`.
    async fn can_target(&self, api_key: &str, renderer_name: &str) -> bool;

    /// Whether `api_key` may load `graphic_id` onto `renderer_name` — checked
    /// before load() sends a LoadMessage. Default implementation allows all
    /// loads (maintaining backward compatibility), but implementations can
    /// enforce zone/renderer-specific graphic restrictions.
    async fn can_load_graphic(
        &self,
        _api_key: &str,
        _renderer_name: &str,
        _graphic_id: &str,
    ) -> bool {
        true
    }
}

/// No restriction at all — every renderer visible, every key can target
/// anything, every connection accepted. The default for a consumer that
/// doesn't need access control (or hasn't wired anything up yet).
pub struct AllowAllAccessControl;

#[async_trait]
impl AccessControl for AllowAllAccessControl {
    async fn authorize_connect(&self, _query: &str) -> bool {
        true
    }

    async fn on_renderer_connected(&self, _name: &str, _query: &str) {}

    async fn filter_visible(
        &self,
        _api_key: &str,
        renderers: Vec<RendererInfo>,
    ) -> Vec<RendererInfo> {
        renderers
    }

    async fn can_target(&self, _api_key: &str, _renderer_name: &str) -> bool {
        true
    }
}
