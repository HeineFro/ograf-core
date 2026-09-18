//! Dependency Inversion seam (Ograf-v2.md §1): Core defines what access
//! control it needs, never how it's decided. `ograf-zones` provides the real
//! implementation; [`AllowAllAccessControl`] is a trivial one for anyone who
//! wants no restriction at all.

use async_trait::async_trait;

use crate::models::RendererInfo;

#[async_trait]
pub trait AccessControl: Send + Sync {
    /// Authorizes a renderer's WebSocket connection *before* it's upgraded.
    /// `query` is the connect URL's raw query string, unparsed and
    /// unvalidated by Core — an implementation decides what it means (a
    /// zone name + token for `ZoneAccessControl`, ignored entirely here).
    async fn authorize_connect(&self, query: &str) -> bool;

    /// Called once a renderer's `hello` names it, after `authorize_connect`
    /// already approved the connection — a chance to record durable
    /// identity, e.g. persisting name → zone for failover (Ograf-v2.md §3).
    /// Best-effort: Core doesn't drop the connection if this does nothing.
    async fn on_renderer_connected(&self, name: &str, query: &str);

    /// Filters `renderers` down to what `api_key` may see — `GET /renderers`.
    async fn filter_visible(&self, api_key: &str, renderers: Vec<RendererInfo>) -> Vec<RendererInfo>;

    /// Whether `api_key` may target the renderer named `renderer_name` —
    /// checked before every renderer-scoped call (get/target/load/play/
    /// stop/update/customAction/clear). Keyed on the stable *name*, not the
    /// per-session `id` (Ograf-v2.md §3).
    async fn can_target(&self, api_key: &str, renderer_name: &str) -> bool;
}

/// No restriction at all — every renderer visible, every key can target
/// anything, every connection accepted. The default for a consumer that
/// doesn't want `ograf-zones` (or hasn't wired anything up yet).
pub struct AllowAllAccessControl;

#[async_trait]
impl AccessControl for AllowAllAccessControl {
    async fn authorize_connect(&self, _query: &str) -> bool {
        true
    }

    async fn on_renderer_connected(&self, _name: &str, _query: &str) {}

    async fn filter_visible(&self, _api_key: &str, renderers: Vec<RendererInfo>) -> Vec<RendererInfo> {
        renderers
    }

    async fn can_target(&self, _api_key: &str, _renderer_name: &str) -> bool {
        true
    }
}
