# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-09-23

### Breaking Changes

- **`RendererId` changed from `Uuid` to `String` (renderer's name)** - For OGraf v1 spec compliance and stability across reconnects, renderer IDs are now the renderer's declared name instead of a server-generated UUID.
  
  **Impact:**
  - Renderer IDs remain stable across reconnects (same name = same ID)
  - Only one live session per renderer name allowed (first-wins policy)
  - Renderer names must be valid: 1-64 characters, `[A-Za-z0-9._-]`, case-sensitive
  - Path parameters like `/ograf/v1/renderers/{rendererId}` now accept renderer names instead of UUIDs
  
  **Migration:**
  ```rust
  // Before (0.3.0)
  use ograf_core::models::RendererId;  // was Uuid
  let id: RendererId = Uuid::new_v4();
  
  // After (0.4.0)
  use ograf_core::models::RendererId;  // now String
  let id: RendererId = "main-output".to_string();
  
  // Validate renderer names before use
  use ograf_core::models::is_valid_renderer_name;
  assert!(is_valid_renderer_name("main-output"));
  ```

- **`RendererRegistry::register()` now returns `Result<RendererId>`** - Previously returned `RendererId` unconditionally. Now returns `Err(AppError::Conflict)` if the renderer name is already connected (first-wins policy).
  
  **Migration:**
  ```rust
  // Before (0.3.0)
  let id = registry.register(session).await;
  
  // After (0.4.0)
  let id = registry.register(session).await?;  // Handle Err case
  ```

- **`RendererRegistry::unregister()` now requires `connection_id`** - Signature changed to `unregister(&self, id: &str, connection_id: Uuid)` to prevent race conditions where a late-cleaning-up connection unregisters a different session that took over the name.
  
  **Migration:**
  ```rust
  // Before (0.3.0)
  registry.unregister(&renderer_id).await;
  
  // After (0.4.0)
  registry.unregister(&renderer_id, connection_id).await;
  // (connection_id is the internal Uuid from RendererSession.connection_id)
  ```

- **`RendererRegistry` method signatures updated** - Methods `get_info()`, `send_and_await()`, and `resolve()` now take `&str` instead of `RendererId` for consistency and to avoid unnecessary clones.

### Added

- **`is_valid_renderer_name()` validation helper** - Public function to validate renderer names before use. Valid names: 1-64 characters, `[A-Za-z0-9._-]`, case-sensitive, URL-safe.
  
  ```rust
  use ograf_core::models::is_valid_renderer_name;
  
  assert!(is_valid_renderer_name("main-output"));
  assert!(is_valid_renderer_name("Renderer_1.backup"));
  assert!(!is_valid_renderer_name(""));  // too short
  assert!(!is_valid_renderer_name("renderer with spaces"));  // invalid chars
  ```

- **`AccessControl::authorize_name()` hook** - New trait method with default implementation (returns `true` for all names). Allows custom implementations to prevent renderer name squatting by unauthorized clients.
  
  ```rust
  async fn authorize_name(&self, name: &str, query: &str) -> bool {
      // Default implementation allows all names
      true
  }
  ```
  
  **Use case:** Implement custom name authorization logic to ensure only authorized clients can claim specific renderer names.

- **`RendererSession.connection_id` field** - Internal `Uuid` for cleanup safety. Ensures a refused or late-cleaning-up connection doesn't unregister a different session that took over the name.

### Migration Guide (0.3.0 → 0.4.0)

#### If you only use the HTTP API (no custom Rust code):
- Update renderer paths: `/ograf/v1/renderers/{uuid}` → `/ograf/v1/renderers/{renderer-name}`
- Renderer names must be valid: 1-64 characters, `[A-Za-z0-9._-]`
- Only one live session per renderer name allowed

#### If you implement `AccessControl`:
- Add `authorize_name()` implementation if you need renderer name authorization (optional, default allows all names)

#### If you use `RendererRegistry` directly:
- Handle `register()` returning `Result` (may fail with `Conflict` error)
- Update `unregister()` calls to pass `connection_id` parameter
- Update method calls to pass `&str` instead of cloning `RendererId`

## [0.3.0] - 2026-09-23

### Breaking Changes

- **Removed `WsMessage` deprecated type alias** - The alias was deprecated in 0.2.0 and is now removed. Use `ServerMessage` instead.

  **Migration:**
  ```rust
  // Before
  use ograf_core::models::WsMessage;
  
  // After
  use ograf_core::models::ServerMessage;
  ```

### Added

- **Access control extensions** - Added two new methods to the `AccessControl` trait with default implementations (maintaining backward compatibility):
  - `filter_graphics()` - Graphics-level access control for `GET /graphics`. Default implementation returns all graphics.
  - `can_load_graphic()` - Per-graphic load authorization checked before `load()` sends a LoadMessage. Default implementation allows all loads.
  
  **Impact:** Enables zone/role-based graphics visibility and renderer-specific graphic restrictions in custom `AccessControl` implementations.

- **Renderer reconnect state resync** - Renderers can now send an optional `instances` array in their `Hello` message to resync Core's view of loaded instances after reconnect. Each instance snapshot includes `instanceId`, `graphicId`, `data`, and `currentStep`. Core infers `InstanceState` from `currentStep` (Playing if set, Loaded otherwise).
  
  **Use case:** Seamless renderer reconnect after network blips or Core restarts without losing instance state.

- **Inline renderer metrics** - `RendererInfo` now includes an optional `metrics` field with:
  - `pendingRequests` - Current number of in-flight renderer requests
  - `messagesSent` / `messagesReceived` - Cumulative WebSocket message counts
  - `uptimeSeconds` - Renderer connection uptime
  
  **Impact:** Observability without requiring a separate metrics endpoint.

- **Health check endpoint** - Added `GET /ograf/v1/health` endpoint returning `{"status": "ok"}`. No authentication required (designed for load balancers and orchestrators).

### Security

- **Bounded pending requests** - Renderer sessions now reject new requests when `pending.len() >= max_pending` (default: 100, configurable via `OGRAF_RENDERER_MAX_PENDING`). Returns `429 Too Many Requests` instead of unbounded memory growth.
  
  **Impact:** DoS protection against renderer request flooding.

- **Sanitized internal error messages** - `AppError::Internal` errors now return a generic "Internal server error" message to users instead of leaking file paths and stack traces. Full errors are logged server-side for debugging.

- **Log injection prevention** - Renderer-provided strings (name, render target) are sanitized before logging by filtering control characters and limiting length to 100 characters.

### Dependencies

- No new dependencies

### Internal

- Metrics tracking uses `AtomicU64` for thread-safe counters without locks
- Added `InstanceSnapshot` wire protocol type for reconnect state resync
- `RendererRegistry` constructor changes:
  - `new()` now calls `Default` (backward compatible)
  - Added `with_max_pending(usize)` for custom limits

## [0.2.0] - 2026-09-21

### Breaking Changes

- **Renamed `WsMessage` to `ServerMessage`** for consistency with `RendererMessage`. A deprecated type alias `WsMessage` is provided for backward compatibility and will be removed in 0.3.0.
  
  **Migration:**
  ```rust
  // Before
  use ograf_core::models::WsMessage;
  
  // After (recommended)
  use ograf_core::models::ServerMessage;
  
  // Or (temporary, deprecated)
  use ograf_core::models::WsMessage; // Still works but triggers deprecation warning
  ```

- **Split protocol types into separate module** - Wire protocol types (`ServerMessage`, `RendererMessage`, `RenderTarget`, `InstanceId`) have been moved to `ograf_core::protocol`. Backward compatibility is maintained via `pub use` re-exports in `models`, but the canonical import path is now `ograf_core::protocol::*`.

  **Migration:**
  ```rust
  // Old (still works via re-export)
  use ograf_core::models::{ServerMessage, RendererMessage};
  
  // New (recommended)
  use ograf_core::protocol::{ServerMessage, RendererMessage};
  ```

### Added

- **Smart constructors for `PlayAction`** - Added `ServerMessage::play_goto()`, `play_delta()`, and `play_continue()` constructors to make invalid states (both `goto` and `delta` set) unrepresentable at compile time.

- **Graphics list caching** - New `OGRAF_GRAPHICS_CACHE_TTL_SECS` environment variable (default: 30 seconds) to cache `GraphicStore::list()` results. Reduces disk I/O from O(requests) to O(1/TTL). Set to `0` to disable caching.
  
  **Performance:** With 500 graphics and 10 requests/sec, reduces disk reads from 5000/sec to ~17/sec (~300× improvement).

- **`is_success()` helper on `RendererMessage`** - Eliminates duplicate `status_code < 400` checks throughout the codebase. Returns `true` for successful result messages, `false` for `Hello`/`Ping`.

### Changed

- **`target_info` response now uses renderer's `name`** instead of stringified `renderTarget` JSON. This provides more meaningful human-readable labels in multi-machine deployments.

  **Before:** `"name": "{\"channel\":1,\"layer\":10}"`  
  **After:** `"name": "Main Output"`

### Performance

- **Replaced global `RwLock` with `DashMap`** for renderer registry. Eliminates global lock contention by using per-shard locking (16 shards). Renderer A's WebSocket messages no longer block `get_info()` calls for Renderer B/C/D.
  
  **Impact:** At 3 renderers @ 60 FPS (180 resolve calls/sec):
  - Before: 180 global write locks/sec blocking all reads
  - After: ~11 writes/shard/sec, 94% conflict-free concurrent access

### Dependencies

- Added `dashmap = "6.1"` for concurrent renderer registry

### Internal

- Refactored `apply_result()` to use Composed Method pattern with `is_success()` helper
- Fixed clippy warning about unnecessary deref in clear handler
- Added `ARCHITECTURE.md` to `.gitignore` (local documentation only)

## [0.1.1] - 2026-09-20

### Changed

- Updated README to reflect early-stage status and remove overstated production-readiness claims
- Added credit to SuperFly.tv's TypeScript implementation
- Improved honesty about testing status

## [0.1.0] - 2026-09-20

### Added

- Initial release
- 100% OGraf v1 Server API spec compliance
- HTTP endpoints for graphics, renderers, and actions
- WebSocket renderer protocol
- File-based graphics storage (no database)
- Dependency Inversion via `AccessControl` trait
- Extensions: `InstanceState` tracking, lenient `currentStep` parsing

[Unreleased]: https://github.com/HeineFro/ograf-core/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/HeineFro/ograf-core/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/HeineFro/ograf-core/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/HeineFro/ograf-core/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/HeineFro/ograf-core/releases/tag/v0.1.0
