# OGraf Specification Compliance

Checked against the OGraf Server API `server-api.yaml` on EBU's `main`
branch (last changed 2026-08-12), endpoint by endpoint and schema by schema,
for `ograf-core` 0.6.0.

## HTTP endpoints

All mounted under `/ograf/v1`.

| Spec endpoint | Status | Notes |
|---|---|---|
| `GET /` | ✅ | `name`, `description`, `author` (`name`, `email`, `url`) and `version` from Core's `Cargo.toml`. Answers at `/ograf/v1/` and `/ograf/v1` |
| `GET /graphics` | ✅ | Filtered through `AccessControl::filter_graphics`. Graphics deleted without `force` are left out |
| `GET /graphics/{graphicId}` | ✅ | `graphic` (the manifest) and `metadata.createdAt` (the manifest file's mtime) |
| `DELETE /graphics/{graphicId}?force=` | ✅ | Without `force`, the graphic is unlisted but its files stay and keep being served to on-air instances, as the spec recommends. They're removed after `OGRAF_DELETED_GRAPHIC_RETENTION_SECS` (default 24 h) once no connected renderer has an instance of it. `force=true` removes them at once. Authorized by `AccessControl::can_delete_graphic` |
| `GET /graphics/{graphicId}/thumbnail?file=` | ✅ | PNG/JPEG/GIF/WebP; 400 for another type or a path outside the graphic, 404 if missing |
| `GET /renderers` | ✅ | `id`, `name`, `description`, plus `status` and `renderTarget` as extensions. Lists connected renderers, ones that disconnected since the process started, and ones a `RendererDirectory` knows |
| `GET /renderers/{rendererId}` | ✅ | `id`, `name`, `status`, `renderTargets`, `renderTargetSchema`, and `description`, `customActions`, `renderCharacteristics` when the renderer declared them in `hello` |
| `GET /renderers/{rendererId}/target?renderTarget=` | ✅ | `renderTarget` is the JSON-stringified identifier. 404 if it isn't the renderer's, or the renderer isn't connected |
| `POST /renderers/{rendererId}/customActions/{customActionId}` | ✅ | `{ "result": … }` |
| `PUT /renderers/{rendererId}/target/graphicInstance/clear` | ✅ | Filters are OR'd, a filter's fields AND'ed, no filters = everything. A `graphicInstanceId` that matches nothing just clears nothing |
| `POST …/graphicInstance/load` | ✅ | Core generates `graphicInstanceId`. 404 for an unknown graphic or RenderTarget |
| `POST …/graphicInstance/playAction` | ✅ | `currentStep` always present |
| `POST …/graphicInstance/stopAction` | ✅ | |
| `POST …/graphicInstance/updateAction` | ✅ | |
| `POST …/graphicInstance/customActions/{customActionId}` | ✅ | |

## Schemas

| Spec schema | Status | Notes |
|---|---|---|
| `ErrorResponse` (RFC 7807) | ✅ | `title`, `status`, `detail` — plus `error` (same text as `detail`), the only field before 0.6.0 |
| `RendererInfo.status` | ✅ | See "Renderer status" below |
| `GraphicInstanceId` (a string) | ✅ | Core creates UUIDs, but accepts any string: one that isn't a UUID is a 404, not a request error |
| `RenderTargetIdentifier` | ✅ | Opaque shallow object, declared by the renderer in `hello`, compared for equality |
| `RenderCharacteristics`, renderer `customActions` | ✅ | Passed through from `hello.capabilities`; Core doesn't interpret them |

## Renderer status

| Situation | `status` | `message` |
|---|---|---|
| Connected | `OK` | – |
| Connected, pending requests ≥ half of `OGRAF_RENDERER_MAX_PENDING`, or its last request timed out (until it answers again) | `WARNING` | e.g. `"3 pending requests"`, `"last request timed out"` |
| Disconnected since the process started | `ERROR` | `"disconnected since <RFC 3339>"` |
| Only known through a `RendererDirectory` | `ERROR` | `"not connected"` |

Actions against a renderer that isn't connected answer `503`.

## Status codes beyond the spec

The spec lists 200, 404, 500 and 550 (and 400 for thumbnails). Core also
answers with more specific codes where they apply — a client that only knows
the spec's codes still sees an error:

| Code | When |
|---|---|
| 400 | Malformed `renderTarget` query / thumbnail reference, or a body that isn't JSON |
| 403 | `AccessControl` refused the call |
| 415 | Body sent without `Content-Type: application/json` |
| 422 | JSON body that doesn't match the spec's shape |
| 429 | Renderer has `OGRAF_RENDERER_MAX_PENDING` requests in flight |
| 503 | Renderer isn't connected |
| 504 | Renderer didn't answer within `OGRAF_ACTION_TIMEOUT_MS` |

## Extensions

Not in the spec; clients can ignore them.

- **Instance state**: each `graphicInstances[]` entry in RenderTargetInfo also
  has `data`, `state` (`loaded`/`playing`/`stopped`) and `currentStep` — the
  renderer-confirmed state as of the last successful action.
- **Renderer list**: `status` and `renderTarget` per renderer.
- **Metrics** (Rust API): `RendererInfo.metrics` — pending requests, message
  counts, uptime.
- **Assets**: `GET /graphics/{id}/assets/*path` and
  `GET /serverApi/internal/graphics/{id}/*path` serve a graphic's files to
  renderers.
- **Health**: `GET /ograf/v1/health`.
- **RenderTarget `name`**: the renderer's name, not the stringified identifier.
- **Lenient `currentStep`**: a non-numeric value from a renderer reads as `0`.

## Renderer WebSocket protocol

Not part of the Server API spec — Core's own. At
`GET /rendererApi/v1/connect` (`RENDERER_CONNECT_PATH`), deliberately outside
`/ograf/v1`.

**Renderer → Core**
- `hello` — `rendererId` (`name` accepted as an alias), `renderTarget`,
  optional `instances` (reconnect resync) and `capabilities`:
  `renderTargetSchema`, `description`, `customActions`,
  `renderCharacteristics`
- `ping` — every 10 s; the connection is dropped after 30 s without one
- `loadResult`, `playActionResult`, `stopActionResult`, `updateActionResult`,
  `customActionResult`, `rendererCustomActionResult`, `clearResult` — each
  with the command's `requestId`, `statusCode` and optional `statusMessage`

**Core → Renderer**
- `welcome` (with `rendererId`), `pong`
- `load`, `playAction`, `stopAction`, `updateAction`, `customAction`,
  `rendererCustomAction`, `clear` — each with a `requestId`

## Access control and renderer directory

The spec leaves security to the vendor. Core delegates it to two traits a
consumer implements:

- `AccessControl` — `authorize_connect`, `authorize_name`,
  `on_renderer_connected`, `filter_visible`, `filter_graphics`,
  `can_target`, `can_load_graphic`, `can_delete_graphic`. The API key is the
  `X-OGraf-Key` header. `AllowAllAccessControl` allows everything.
- `RendererDirectory` — `known_renderers`, for renderers beyond the
  connected ones (e.g. from a database). `NoDirectory` knows none.
