# Design: HTTP Router & Web Server

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Axum-based HTTP API server with embedded SvelteKit frontend. This document describes the *current* behavior of the web server architecture, routing, and request handling.

---

## Server Startup

### `cli/src/commands/serve.rs`

**Entry point:** `riply serve [--host <addr>] [-p <port>]`

**Startup sequence:**
1. Resolve host/port (CLI args override config)
2. Initialize MakeMKV backend (from config or auto-detect)
3. Initialize metadata searcher (if API keys configured)
4. Create `AppState`:
   ```rust
   pub struct AppState {
       pub db: Arc<Database>,
       pub config: Arc<RwLock<Config>>,
       pub config_path: Option<String>,
       pub sse: SseManager,           // 256-event broadcast channel
       pub backend: Option<Arc<dyn RipBackend>>,
       pub searcher: Option<Arc<dyn MetadataSearcher>>,
       pub scheduler: JobScheduler,
       pub active_rips: RwLock<HashMap<usize, JoinHandle<()>>>,
       pub autorip_drives: RwLock<HashSet<usize>>,
       pub autorip_handle: RwLock<Option<JoinHandle<()>>>,
       pub healthchecks_handle: RwLock<Option<JoinHandle<()>>>,
       pub metrics: Metrics,
       pub disc_names: RwLock<HashMap<usize, DiscNameInfo>>,
   }
   ```
5. Start autorip poll loop (background task)
6. Start healthchecks.io heartbeat (if configured)
7. Create Axum router
8. Bind TCP listener
9. Start serving with graceful shutdown

**Graceful shutdown:**
- Listens for Ctrl+C (`SIGINT`) and SIGTERM
- On signal: aborts autorip handle, logs "Server shut down gracefully"

---

## Router Structure

### `web/src/api.rs::create_router(state)`

```
Router
├── /v1/*          (API sub-router, 30s timeout)
│   ├── /files/*   (files module)
│   ├── /review/*  (review module)
│   ├── /scan/*    (scan module)
│   ├── /analyze/* (analyze module)
│   ├── /move/*    (mover module)
│   ├── /jobs/*    (jobs module)
│   ├── /ripper/*  (ripper module)
│   ├── /config/*  (config module)
│   └── /search/*  (search module)
├── /v1/sse        (SSE handler, NO timeout)
├── /health        (health check)
├── /ready         (readiness check)
├── /metrics       (Prometheus metrics)
└── /* (fallback: embedded SvelteKit assets)
```

### Layer Stack (outer to inner)

| Layer | Purpose |
|---|---|
| `CorsLayer::permissive()` | Allow all origins (dev server, reverse proxy) |
| `TraceLayer::new_for_http()` | HTTP request tracing/logging |
| `TimeoutLayer::new(30s)` | Timeout for API handlers only (NOT SSE) |

### Timeout Design

The 30s `TimeoutLayer` applies only to the API sub-router (`/v1/*` routes merged together). SSE and static fallback are intentionally outside the timed router because:
- SSE is a long-lived stream (would be cut after 30s)
- Static file serving should not timeout

---

## API Response Format

### `ApiResponse<T>`

```rust
pub struct ApiResponse<T: serde::Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,  // Omitted if None
}
```

**Usage:**
```rust
// Success
Json(ApiResponse::ok(data))

// Error
Err(WebError::NotFound("..."))
```

### `WebError`

Custom error type implementing `IntoResponse`:

| Variant | HTTP Status |
|---|---|
| `BadRequest(msg)` | 400 |
| `Unauthorized(msg)` | 401 |
| `NotFound(msg)` | 404 |
| `BadRequest(msg)` | 400 |
| `ServiceUnavailable(msg)` | 503 |
| `Internal(msg)` | 500 |

### `PaginatedResponse<T>`

```rust
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}
```

### `JobStartedResponse`

```rust
pub struct JobStartedResponse {
    pub job_id: String,
    pub status: String,
}
```

---

## Route Modules

### Files (`web/src/routes/files.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/files` | `list_files` | Paginated file list with filters |
| GET | `/v1/files/:id` | `get_file` | Single file by ID |
| POST | `/v1/files/:id` | `update_file` | Update canonical metadata |
| DELETE | `/v1/files/:id` | `delete_file` | Delete file record |
| DELETE | `/v1/files` | `reset_db` | Wipe all data |

**List params:** `status`, `search`, `page`, `per_page`

### Review (`web/src/routes/review.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/review` | `list_review_items` | Files needing review |
| POST | `/v1/review/:id/accept` | `accept_item` | Accept match → Confirmed |
| POST | `/v1/review/:id/reject` | `reject_item` | Reject → Failed |

### Scan (`web/src/routes/scan.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/scan` | `scan_directory` | Start scan job |

### Analyze (`web/src/routes/analyze.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/analyze` | `analyze_files` | Start analyze job |

### Move (`web/src/routes/move_.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/move/preview` | `preview_move` | Where files would move |
| POST | `/v1/move/execute` | `execute_move` | Move confirmed files |

### Jobs (`web/src/routes/jobs.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/jobs` | `list_jobs` | List all jobs |
| GET | `/v1/jobs/:id` | `get_job` | Get job by ID |
| POST | `/v1/jobs/:id/cancel` | `cancel_job` | Cancel a job |

### Ripper (`web/src/routes/ripper.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/ripper/drives` | `list_drives` | List drives |
| POST | `/v1/ripper/drives/:index/rename` | `rename_disc` | Set disc name |
| GET | `/v1/ripper/status` | `ripper_status` | MakeMKV installation status |
| POST | `/v1/ripper/:index/start` | `start_rip` | Start rip |
| POST | `/v1/ripper/:index/cancel` | `cancel_rip` | Cancel rip |
| POST | `/v1/ripper/:index/eject` | `eject_disc` | Eject disc |
| GET | `/v1/ripper/autorip` | `autorip_status` | Autorip-enabled drives |
| POST | `/v1/ripper/autorip/:index/toggle` | `autorip_toggle` | Toggle autorip |

### Config (`web/src/routes/config.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/config` | `get_config` | Get current config |
| POST | `/v1/config` | `update_config` | Update config |
| POST | `/v1/config/reset` | `reset_config` | Reset to defaults |

### Search (`web/src/routes/search.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/search/tmdb` | `search_tmdb` | Search TMDb |
| POST | `/v1/search/omdb` | `search_omdb` | Search OMDb |

### SSE Handler (`web/src/routes/sse_handler.rs`)

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/sse` | `sse_handler` | SSE endpoint |

---

## Health & Metrics

### `GET /health`

**Response:**
```json
{
    "status": "ok",
    "version": "0.1.0"
}
```

### `GET /ready`

Readiness check — verifies database connectivity.

**Response (ready):**
```json
{
    "status": "ready",
    "database": "ok"
}
```

**Response (unavailable):**
```json
{
    "status": "unavailable",
    "database": "error",
    "error": "Database unavailable"
}
```
(HTTP 503)

### `GET /metrics`

Prometheus metrics endpoint. Returns formatted metrics string.

---

## Static File Serving

### Embedded Frontend

The SvelteKit frontend is embedded into the binary at build time using `rust-embed`:

```rust
#[derive(rust_embed::RustEmbed)]
#[folder = "svelte/build"]
struct SvelteAssets;
```

### Serving Logic

```rust
async fn embedded_assets(uri: Uri) -> Response
```

**Behavior:**
1. Trims leading `/` from path
2. Looks up asset in `SvelteAssets`
3. Falls back to `index.html` (SPA routing)
4. Sets appropriate `Content-Type` from asset metadata
5. Sets cache control:
   - `index.html`: `no-cache`
   - `_app/*` (static assets): `public, max-age=31536000, immutable`
6. Returns 404 if asset not found

### Build Integration

**Build process:**
1. `npm run build` in `web/svelte/` produces `web/svelte/build/`
2. `cargo build` embeds `build/` contents via `rust-embed`
3. Binary is self-contained (no external files needed)

**Gotcha:** After `git pull`, the embedded frontend is stale. Must run `npm run build` before `cargo build`.

---

## CORS

**Configuration:** `CorsLayer::permissive()` — allows all origins, methods, and headers.

**Why permissive?** The dashboard is served from the same host, but development servers may run on different ports/hosts.

---

## Metrics

### `web/src/metrics.rs`

Tracks request counts, response times, and error rates.

**Prometheus format:**
```
# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{method="GET",path="/v1/files",status="200"} 42
```

---

## Where This Lives

| Concern | File |
|---|---|
| Router creation | `web/src/api.rs` |
| AppState | `web/src/api.rs` |
| Response types | `web/src/api.rs` |
| Static file serving | `web/src/api.rs` |
| Health/Readiness | `web/src/api.rs` |
| Metrics | `web/src/metrics.rs` |
| SSE manager | `web/src/sse.rs` |
| SSE handler | `web/src/routes/sse_handler.rs` |
| Error handling | `web/src/error.rs` |
| Build script | `web/build.rs` |
| Serve command | `cli/src/commands/serve.rs` |

---

## Notes

- **Library-only crate** — `web` is a library; the binary is in `cli` (`riply`)
- **Timeout on API only** — SSE and static files are outside the 30s timeout
- **CORS permissive** — All origins allowed
- **Embedded frontend** — Built at compile time, no external files
- **Graceful shutdown** — Handles SIGINT and SIGTERM
- **Auto-rip runs always** — Background task starts on server init
- **Healthchecks opt-in** — Only active if `healthchecks.uuid` is set
- **State is in-memory** — `active_rips`, `autorip_drives`, `disc_names` are lost on restart
- **DB is persistent** — SQLite file survives restarts
