use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{
    error::{AppError, Result},
    models::Graphic,
};

/// Reads graphics straight from disk — Core owns no database. Whatever sits
/// in front of Core (`ograf-zones`' admin routes, or a human with `scp`)
/// writes (and deletes) `{graphics_storage}/{graphic_id}/...` directly; Core
/// only ever reads. Re-scanned on every call rather than cached, since at
/// the scale this runs at (a handful of templates) that's cheaper than
/// building and invalidating a cache correctly.
pub struct GraphicStore {
    root: PathBuf,
}

/// A `graphic_id` reaches here straight from a URL path segment — it's only
/// ever safe to use as a filesystem directory name (never joined containing
/// `..`/`/`) once it passes this. Mirrors the charset `ograf-zones::storage`
/// already enforces at upload time, enforced again here since every other
/// caller (read/thumbnail/asset/delete) trusts whatever's in the URL.
pub fn is_valid_graphic_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Joins `rel` onto `base` component-by-component, rejecting `..` and
/// absolute paths instead of letting them either escape `base` or (per
/// `PathBuf::join`'s documented behavior) discard `base` entirely when `rel`
/// is itself absolute. `rel` here is untrusted (a query param / URL
/// wildcard tail) and used to serve files straight off disk, so this is the
/// only thing standing between a request and arbitrary file read.
pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut result = base.to_path_buf();
    for component in Path::new(rel).components() {
        match component {
            std::path::Component::Normal(part) => result.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
        }
    }
    Some(result)
}

impl GraphicStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub async fn get(&self, id: &str) -> Result<Graphic> {
        if !is_valid_graphic_id(id) {
            return Err(AppError::NotFound(format!("graphic '{id}'")));
        }
        load_one(&self.root, id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("graphic '{id}'")))
    }

    pub async fn list(&self) -> Result<Vec<Graphic>> {
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            // No graphics have ever been uploaded yet — an empty list, not
            // an error.
            Err(_) => return Ok(Vec::new()),
        };

        let mut graphics = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to read graphics dir: {e}")))?
        {
            if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            // A directory without a valid manifest isn't a graphic (could be
            // mid-upload, or leftover cruft) — skip it rather than fail the
            // whole listing.
            if let Some(graphic) = load_one(&self.root, &id).await {
                graphics.push(graphic);
            }
        }

        graphics.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
        Ok(graphics)
    }

    /// Storage path for a graphic that may or may not exist — used by asset
    /// and thumbnail serving, which do their own 404 handling on read.
    pub fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }
}

async fn load_one(root: &Path, id: &str) -> Option<Graphic> {
    let dir = root.join(id);
    let manifest_path = find_manifest(&dir).await?;

    let raw = tokio::fs::read_to_string(&manifest_path).await.ok()?;
    let manifest: Value = serde_json::from_str(&raw).ok()?;

    let uploaded_at = tokio::fs::metadata(&manifest_path)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
        .unwrap_or_default();

    Some(Graphic {
        id: id.to_string(),
        name: manifest["name"].as_str().unwrap_or(id).to_string(),
        version: manifest["version"].as_str().map(String::from),
        description: manifest["description"].as_str().map(String::from),
        manifest,
        storage_path: dir.to_string_lossy().into_owned(),
        uploaded_at,
    })
}

async fn find_manifest(dir: &Path) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    while let Some(entry) = entries.next_entry().await.ok()? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".ograf.json") {
            return Some(entry.path());
        }
    }
    None
}
