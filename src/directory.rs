//! Which renderers exist beyond the ones connected right now — a second
//! Dependency Inversion seam next to [`AccessControl`](crate::access::AccessControl).
//! Core itself keeps no storage: it remembers disconnected renderers only for
//! the life of the process. A consumer with a database (or a config file, or
//! an NMOS registry) implements [`RendererDirectory`] so controllers can see
//! renderers that are offline, or haven't connected yet — reported with
//! `status: ERROR`.

use async_trait::async_trait;

use crate::models::RendererId;

/// A renderer a [`RendererDirectory`] knows about.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct KnownRenderer {
    pub id: RendererId,
    pub name: String,
    pub description: Option<String>,
}

impl KnownRenderer {
    /// Core's renderer ids are also their names, so `id` fills both.
    pub fn new(id: impl Into<RendererId>) -> Self {
        let id = id.into();
        Self { name: id.clone(), id, description: None }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

#[async_trait]
pub trait RendererDirectory: Send + Sync {
    /// Every renderer this directory knows, connected or not. Core merges
    /// them with its own sessions; a renderer that's connected is reported
    /// from its live session, whatever the directory says.
    async fn known_renderers(&self) -> Vec<KnownRenderer>;
}

/// No directory — only connected renderers and ones that disconnected since
/// the process started are listed. The default.
pub struct NoDirectory;

#[async_trait]
impl RendererDirectory for NoDirectory {
    async fn known_renderers(&self) -> Vec<KnownRenderer> {
        Vec::new()
    }
}
