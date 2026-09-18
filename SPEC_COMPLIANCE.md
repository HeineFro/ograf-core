# OGraf Specification Compliance

This document verifies that `ograf-core` implements the OGraf v1 Server API specification.

## HTTP REST API Endpoints

All endpoints are mounted under `/ograf/v1` (configurable by consumer).

### Server Info
- `GET /` - Returns server name, description, and version

### Graphics Management
- `GET /graphics` - List all available graphics
- `GET /graphics/:id` - Get specific graphic details and manifest
- `GET /graphics/:id/thumbnail?file=<path>` - Get graphic thumbnail
- `GET /graphics/:id/assets/*path` - Serve graphic asset files

### Renderer Management
- `GET /renderers/connect` - WebSocket upgrade endpoint for renderer connection
- `GET /renderers` - List all connected renderers (filtered by AccessControl)
- `GET /renderers/:id` - Get specific renderer details
- `GET /renderers/:id/target?renderTarget=<json>` - Get render target details

### Renderer Actions (Instance-scoped)
- `POST /renderers/:id/target/graphicInstance/load` - Load a graphic onto a render target
- `POST /renderers/:id/target/graphicInstance/playAction` - Play/step a graphic instance
- `POST /renderers/:id/target/graphicInstance/stopAction` - Stop a graphic instance
- `POST /renderers/:id/target/graphicInstance/updateAction` - Update graphic instance data
- `PUT /renderers/:id/target/graphicInstance/clear` - Clear graphic instances (with filters)
- `POST /renderers/:id/target/graphicInstance/customActions/:actionId` - Execute custom instance action

### Renderer Actions (Renderer-scoped)
- `POST /renderers/:id/customActions/:actionId` - Execute custom renderer action

### Internal Routes
- `GET /serverApi/internal/graphics/:graphic_id/*path` - Internal asset serving for renderer HTML

## WebSocket Protocol

### Client → Server Messages

#### Connection Lifecycle
- `hello` - Initial handshake with renderer name, renderTarget, and capabilities
- `ping` - Keepalive ping (30s timeout)

#### Action Results
All results include `requestId` for correlation, `statusCode`, and optional `statusMessage`:

- `loadResult` - Confirms graphic load (includes instanceId, graphicId, data)
- `playActionResult` - Confirms play action (includes currentStep)
- `stopActionResult` - Confirms stop action
- `updateActionResult` - Confirms update action (includes updated data)
- `customActionResult` - Confirms custom instance action
- `rendererCustomActionResult` - Confirms custom renderer action (includes result payload)
- `clearResult` - Confirms instance clear

### Server → Client Messages

All commands include `requestId` for correlation.

- `welcome` - Sent after successful hello (includes assigned rendererId)
- `pong` - Response to ping
- `load` - Load graphic (includes requestId, instanceId, graphicId, data)
- `playAction` - Play/step instance (includes goto, delta, skipAnimation)
- `stopAction` - Stop instance (includes skipAnimation)
- `updateAction` - Update instance data (includes data, skipAnimation)
- `customAction` - Execute custom instance action (includes actionId, payload, skipAnimation)
- `rendererCustomAction` - Execute custom renderer action (includes actionId, payload, skipAnimation)
- `clear` - Clear instance (includes instanceId)

## Access Control

OGraf spec does not mandate access control. `ograf-core` implements it via the `AccessControl` trait:

- `authorize_connect(&self, query: &str)` - Authorize renderer WebSocket connection
- `on_renderer_connected(&self, name: &str, query: &str)` - Post-connection hook
- `filter_visible(&self, api_key: &str, renderers)` - Filter visible renderers
- `can_target(&self, api_key: &str, renderer_name: &str)` - Authorize renderer targeting

API key is read from `X-OGraf-Key` header.

## Spec Extensions (Non-breaking)

These additions are **not** in the official OGraf spec but don't break compatibility:

### 1. InstanceState Tracking
- **Location**: `GraphicInstance` in internal state
- **Purpose**: Track whether instance is `Loaded`, `Playing`, or `Stopped`
- **Exposed**: Yes, in `GET /renderers/:id/target` response
- **Breaking**: No - clients can ignore these extra fields

### 2. Extra GraphicInstance Fields
Added to `GET /renderers/:id/target` response:
- `data` - Last confirmed data from renderer
- `state` - Instance state (Loaded/Playing/Stopped)
- `currentStep` - Last reported step from playAction

**Breaking**: No - extra fields are additive and optional for clients

### 3. Lenient currentStep Parsing
- **Behavior**: If renderer returns non-numeric currentStep, defaults to 0.0
- **Rationale**: Prevents entire message failure on malformed template response
- **Breaking**: No - graceful degradation

### 4. Graphics Endpoints Unscoped
- **Behavior**: No access control on `/graphics` endpoints
- **Rationale**: Access control happens at renderer level, not graphic level
- **Breaking**: No - spec doesn't mandate graphics access control

## Storage

Graphics are stored on disk at `OGRAF_STORAGE` (default: `./graphics`):
```
./graphics/
  my-graphic-id/
    template.ograf.json  (manifest)
    index.html
    assets/...
```

Any directory with a `*.ograf.json` file is a valid graphic. The directory name is the `graphicId`.

## Compliance Summary

✅ **Full OGraf v1 Server API compliance** with additive, non-breaking extensions for improved observability.

All core endpoints, WebSocket messages, and behaviors match the official specification. Extensions are opt-in (clients can ignore extra fields) and don't affect spec-compliant clients or renderers.
