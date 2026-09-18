use std::{env, time::Duration};

/// No `renderer_token` here anymore — Ograf-v2.md §3 replaces the single
/// global token with per-zone tokens, which live in `ograf-zones`'
/// database, not in Core's env-derived config.
pub struct Config {
    pub host: String,
    pub port: u16,
    pub graphics_storage: String,
    pub log_level: String,
    pub action_timeout_ms: u64,
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
        }
    }

    /// How long the server waits for a renderer to confirm a load/action
    /// command before treating it as failed (504).
    pub fn action_timeout(&self) -> Duration {
        Duration::from_millis(self.action_timeout_ms)
    }
}
