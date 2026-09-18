# ograf-core

A pure, unauthenticated Rust implementation of the [OGraf](https://ograf.ebu.io/)
graphics-control HTTP + WebSocket API. No database, no users, no API keys,
no opinion on multi-tenancy — just graphics, renderers, and actions, per the
spec.

## Design

Every access decision — who may connect a renderer, who may see it, who may
target it — is delegated to an [`AccessControl`](src/access.rs) trait that a
consuming binary implements and wires in. `ograf-core` ships one trivial
implementation, [`AllowAllAccessControl`](src/access.rs), that imposes no
restriction at all.

This is a deliberate Dependency Inversion seam: `ograf-core` depends on
nothing outside this crate and knows nothing about zones, tenants, or
credentials. A consumer that wants real access control (zones, per-vendor
API keys, encrypted renderer tokens, ...) implements `AccessControl` in its
own crate and links `ograf-core` as a library — it never needs to fork or
patch this code to do it.

## Quick start

```rust
use std::{net::SocketAddr, sync::Arc};

use ograf_core::{access::AllowAllAccessControl, build_router, config::Config, store::renderers::RendererRegistry, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env();

    let state = AppState {
        config: Arc::new(config),
        renderers: Arc::new(RendererRegistry::new()),
        access: Arc::new(AllowAllAccessControl),
    };

    let addr: SocketAddr = format!("{}:{}", state.config.host, state.config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, build_router(state)).await?;

    Ok(())
}
```

`build_router` returns a plain `axum::Router` — nest it under your own
top-level router alongside whatever admin/auth routes your binary adds.

## Configuration

`Config::from_env()` reads:

| Variable | Default | Meaning |
|---|---|---|
| `OGRAF_HOST` | `0.0.0.0` | Bind address |
| `OGRAF_PORT` | `8080` | Bind port |
| `OGRAF_STORAGE` | `./graphics` | Where graphics live on disk (relative to the process's working directory) |
| `RUST_LOG` | `info` | Log level |
| `OGRAF_ACTION_TIMEOUT_MS` | `5000` | How long an HTTP action call waits for the renderer's confirmation before failing |

## Where graphics come from

`GraphicStore` (`src/store/graphics.rs`) scans `OGRAF_STORAGE` fresh on
every request — no database, no cache. Any immediate subdirectory
containing a `*.ograf.json` manifest is treated as one graphic; the
subdirectory name becomes its `graphicId`. Writing (or deleting) that
directory is entirely the consumer's job — `ograf-core` only ever reads.

## API surface

- `GET /ograf/v1/` — server info
- `GET /ograf/v1/graphics`, `GET /ograf/v1/graphics/:id` — list/inspect graphics
- `GET /ograf/v1/graphics/:id/assets/*path`, `GET /ograf/v1/graphics/:id/thumbnail` — serve graphic assets
- `GET /ograf/v1/renderers/connect` — renderer WebSocket upgrade
- `GET /ograf/v1/renderers`, `GET /ograf/v1/renderers/:id`, `GET /ograf/v1/renderers/:id/target` — list/inspect renderers
- `PUT /ograf/v1/renderers/:id/target/graphicInstance/{load,clear}`
- `POST /ograf/v1/renderers/:id/target/graphicInstance/{playAction,stopAction,updateAction}`
- `POST /ograf/v1/renderers/:id/target/graphicInstance/customActions/:actionId`
- `POST /ograf/v1/renderers/:id/customActions/:actionId` — renderer-scoped custom action

## OGraf v1 Spec Compliance

`ograf-core` is **100% compatible** with the [OGraf v1 Server API specification](https://ograf.ebu.io/) and can be used as a drop-in replacement for any OGraf-compliant server.

### Non-breaking Extensions

These additions enhance observability without breaking compatibility with spec-compliant clients or renderers:

#### 1. Instance State Tracking
The `GET /renderers/:id/target` response includes extra fields for each `GraphicInstance`:
- `state` — Current instance state: `loaded`, `playing`, or `stopped`
- `currentStep` — Last reported step from `playAction` (persisted between actions)
- `data` — Last confirmed data from the renderer

**Rationale**: Helps dashboards and UIs display more than "a graphic is loaded here" without requiring clients to track state themselves.

#### 2. Lenient `currentStep` Parsing
If a renderer's `playActionResult` contains a non-numeric `currentStep`, it defaults to `0.0` instead of rejecting the entire message.

**Rationale**: Prevents timeout/failure when a template returns unexpected values. The action still succeeds; only this one field degrades gracefully.

#### 3. Unscoped Graphics Endpoints
`GET /graphics` and `GET /graphics/:id` have no access control — all graphics are visible to all API keys.

**Rationale**: Real access control happens at the renderer level (via `can_target`). Automation loads known `graphicId`s from config and never browses the list; hiding templates wouldn't be a real security boundary.

**Compatibility**: Clients can safely ignore all extra fields. Renderers see only standard OGraf messages. See [SPEC_COMPLIANCE.md](SPEC_COMPLIANCE.md) for full details.

## Status

Early-stage (`0.1.0`). Suitable for production use as a library. Consumers implement their own access control via the `AccessControl` trait.

## License

Dual-licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
