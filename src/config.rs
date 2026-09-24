use std::{env, time::Duration};

/// No `renderer_token` here — access control is delegated to the
/// `AccessControl` implementation, not handled by Core's env-derived config.
#[non_exhaustive]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub graphics_storage: String,
    pub log_level: String,
    pub action_timeout_ms: u64,
    pub graphics_cache_ttl_secs: u64,
    pub renderer_max_pending: usize,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            host: env::var("OGRAF_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("OGRAF_PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .expect("OGRAF_PORT must be a valid port number"),
            graphics_storage: env::var("OGRAF_STORAGE").unwrap_or_else(|_| "./graphics".into()),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
            action_timeout_ms: env::var("OGRAF_ACTION_TIMEOUT_MS")
                .unwrap_or_else(|_| "5000".into())
                .parse()
                .expect("OGRAF_ACTION_TIMEOUT_MS must be a valid number"),
            graphics_cache_ttl_secs: env::var("OGRAF_GRAPHICS_CACHE_TTL_SECS")
                .unwrap_or_else(|_| "30".into())
                .parse()
                .expect("OGRAF_GRAPHICS_CACHE_TTL_SECS must be a valid number"),
            renderer_max_pending: env::var("OGRAF_RENDERER_MAX_PENDING")
                .unwrap_or_else(|_| "100".into())
                .parse()
                .expect("OGRAF_RENDERER_MAX_PENDING must be a valid number"),
        }
    }

    /// How long the server waits for a renderer to confirm a load/action
    /// command before treating it as failed (504).
    pub fn action_timeout(&self) -> Duration {
        Duration::from_millis(self.action_timeout_ms)
    }

    /// How long to cache the graphics list before re-scanning disk.
    /// Set to 0 to disable caching (always fetch fresh from disk).
    pub fn graphics_cache_ttl(&self) -> Duration {
        Duration::from_secs(self.graphics_cache_ttl_secs)
    }
}
