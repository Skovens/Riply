---
name: axum-backend-api
description: >
  Axum HTTP API for Riply. Covers AppState, the router, all route handlers
  (files, review, scan, analyze, move, jobs, ripper, config, SSE),
  ApiResponse/PaginatedResponse patterns, Config hot-reload, and
  healthchecks.io integration.
metadata:
  language: rust
  version: "0.7+"
  compatibility: axum, tokio, serde, tower-http, async-stream
---

# Axum Backend API

## Architecture Overview

The web layer is a library crate (`web`) that provides an Axum router,
shared application state, and SSE broadcasting. The binary crate (`cli`)
starts the server via `riply serve`.

```
cli (binary) -> serve.rs -> AppState -> web::create_router() -> axum::serve
```

### Key Modules

| Module | Path | Purpose |
|--------|------|---------|
| `web::api` | `web/src/api.rs` | AppState, create_router, response types |
| `web::sse` | `web/src/sse.rs` | SseManager (broadcast channel) |
| `web::routes` | `web/src/routes/mod.rs` | Re-exports all route modules |
| `infra::config` | `infra/src/config.rs` | Config, RippingConfig, HealthchecksConfig |
| `infra::healthchecks` | `infra/src/healthchecks.rs` | healthchecks.io ping client |

### Module Location

```
web/src/api.rs           - AppState, create_router, ApiResponse, PaginatedResponse, JobStartedResponse
web/src/sse.rs           - SseManager (broadcast channel for status updates)
web/src/routes/mod.rs    - Route module re-exports
web/src/routes/files.rs  - File CRUD with pagination and search
web/src/routes/review.rs - Review queue, accept/reject
web/src/routes/scan.rs   - Start scan job
web/src/routes/analyze.rs - Start analyze job + retry-failed
web/src/routes/move_.rs  - Start move job + preview + status
web/src/routes/jobs.rs   - Job management (list, get, cancel)
web/src/routes/ripper.rs - Drive management, rip, cancel, eject, autorip
web/src/routes/config.rs - Get/update config with healthchecks restart
web/src/routes/sse_handler.rs - SSE endpoint
cli/src/commands/serve.rs - Server startup, AppState construction
infra/src/config.rs      - Config struct, RippingConfig, HealthchecksConfig
infra/src/healthchecks.rs - HealthchecksClient (hc-ping.com)
```

## AppState

The shared state struct holding all dependencies for route handlers.

```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub config_path: Option<String>,
    pub sse: SseManager,
    pub backend: Option<Arc<dyn RipBackend>>,
    pub scheduler: JobScheduler,
    pub active_rips: RwLock<HashMap<usize, JoinHandle<()>>>,
    pub autorip_handle: RwLock<Option<JoinHandle<()>>>,
    pub healthchecks_handle: RwLock<Option<JoinHandle<()>>>,
}
```

### Field Details

| Field | Type | Purpose |
|-------|------|---------|
| `db` | `Arc<Database>` | SQLite connection pool (SQLx) |
| `config` | `Arc<RwLock<Config>>` | Hot-reloadable configuration |
| `config_path` | `Option<String>` | Path to config file for persistence |
| `sse` | `SseManager` | Broadcast channel for real-time updates |
| `backend` | `Option<Arc<dyn RipBackend>>` | MakeMKV ripping backend (nullable) |
| `scheduler` | `JobScheduler` | Background job registry |
| `active_rips` | `RwLock<HashMap<usize, JoinHandle<()>>>` | Active rip tasks by drive index |
| `autorip_handle` | `RwLock<Option<JoinHandle<()>>>` | Autorip polling task handle |
| `healthchecks_handle` | `RwLock<Option<JoinHandle<()>>>` | Healthchecks heartbeat task handle |

### Construction (in `cli/src/commands/serve.rs`)

```rust
let state = Arc::new(AppState {
    db: Arc::new(db),
    config: Arc::new(RwLock::new(config.clone())),
    config_path,
    sse: SseManager::new(256),
    backend: /* from config or detect */,
    scheduler: JobScheduler::new(),
    active_rips: RwLock::new(HashMap::new()),
    autorip_handle: RwLock::new(None),
    healthchecks_handle: RwLock::new(None),
});
```

## Response Types

### ApiResponse<T>

Standard JSON wrapper for all API responses.

```rust
#[derive(serde::Serialize)]
pub struct ApiResponse<T: serde::Serialize> {
    pub success: bool,
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: serde::Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}
```

### PaginatedResponse<T>

Used for list endpoints with pagination.

```rust
#[derive(serde::Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}
```

### JobStartedResponse

Returned when a job is queued.

```rust
#[derive(serde::Serialize)]
pub struct JobStartedResponse {
    pub job_id: String,
    pub status: String,
}
```

## Router Registration

The router is built in `web/src/api.rs::create_router()`:

```rust
Router::new()
    .route("/health", get(|| async { "OK" }))
    // Files
    .route("/api/files", get(files::list_files))
    .route("/api/files/:id", get(files::get_file)
        .post(files::update_file)
        .delete(files::delete_file))
    // Review
    .route("/api/review", get(review::get_review_queue))
    .route("/api/review/:id/accept", post(review::accept_file))
    .route("/api/review/:id/reject", post(review::reject_file))
    // Scan
    .route("/api/scan", post(scan::start_scan))
    // Analyze
    .route("/api/analyze", post(analyze::start_analyze))
    .route("/api/analyze/retry-failed", post(analyze::retry_failed))
    // Move
    .route("/api/move", post(mover_routes::start_move))
    .route("/api/move/status", get(mover_routes::get_move_status))
    .route("/api/move/preview", get(mover_routes::preview_move))
    // Jobs
    .route("/api/jobs", get(jobs::list_jobs))
    .route("/api/jobs/:id", get(jobs::get_job))
    .route("/api/jobs/:id/cancel", post(jobs::cancel_job))
    // Ripper
    .route("/api/ripper/drives", get(ripper::list_drives))
    .route("/api/ripper/status", get(ripper::ripper_status))
    .route("/api/ripper/:index/start", post(ripper::start_rip))
    .route("/api/ripper/:index/cancel", post(ripper::cancel_rip))
    .route("/api/ripper/:index/eject", post(ripper::eject_disc))
    .route("/api/ripper/autorip", get(ripper::autorip_status))
    .route("/api/ripper/autorip/toggle", post(ripper::autorip_toggle))
    // Config
    .route("/api/config", get(cfg::get_config).put(cfg::update_config))
    // SSE
    .route("/api/sse", get(sse_handler::sse_endpoint))
    // Static files
    .fallback_service(static_service)
    .with_state(state)
```

## Route Handlers

### Files (`web/src/routes/files.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/files` | GET | List files with pagination, filtering, search |
| `/api/files/:id` | GET | Get single file by ID |
| `/api/files/:id` | POST | Update file metadata |
| `/api/files/:id` | DELETE | Delete file by ID |

**Query params (GET /api/files):**

| Param | Type | Default | Constraint |
|-------|------|---------|------------|
| `status` | `Option<String>` | None | Filter by PipelineStatus string |
| `search` | `Option<String>` | None | Text search |
| `page` | `Option<u32>` | 1 | Minimum 1 |
| `per_page` | `Option<u32>` | 50 | Maximum 200 |

**Request body (POST /api/files/:id):**

```json
{
  "canonical_title": "Movie Name",
  "canonical_year": 2023,
  "series_name": "Show Name",
  "season_number": 1,
  "episode_number": 5,
  "media_type": "tv" | "movie"
}
```

**Media type parsing:** Accepts `"tv"`, `"Tv"`, `"TV"` -> `MediaType::Tv`, anything else -> `MediaType::Movie`.

### Review (`web/src/routes/review.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/review` | GET | Get review queue (NeedsReview files) |
| `/api/review/:id/accept` | POST | Accept file -> Confirmed |
| `/api/review/:id/reject` | POST | Reject file -> Failed |

**Accept logic:**
- Only allows files in `Matched` or `NeedsReview` status
- Updates to `Confirmed`
- Broadcasts `review:accepted:{id}` via SSE

**Reject logic:**
- Only allows files in `NeedsReview` status
- Updates to `Failed`
- Broadcasts `review:rejected:{id}` via SSE

### Scan (`web/src/routes/scan.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/scan` | POST | Start scan job |

**Request body:** `{ "directory": "/path/to/scan" }`

Validates directory existence before queuing. Uses `JobScheduler::start_scan_job()`.

**Job work:**
1. Scans directory with `ScannerService`
2. Checks for duplicates by hash
3. Saves records to database
4. Reports progress via SSE every ~10 files
5. Checks cancellation cooperatively

### Analyze (`web/src/routes/analyze.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/analyze` | POST | Start analyze job |
| `/api/analyze/retry-failed` | POST | Reset failed -> scanned, then analyze |

Both endpoints use `JobScheduler::start_analyze_job()`.

**Retry-failed flow:**
1. Reset all `Failed` files back to `Scanned` status
2. Start new analyze job on all `Scanned` files

**Job work:**
1. Reads API keys from config to decide if metadata search is available
2. Analyzes each file with `AnalyzerService`
3. Updates database records
4. Reports progress via SSE every ~10 files
5. Reports final counts: matched + needs review

### Move (`web/src/routes/move_.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/move` | POST | Start move job |
| `/api/move/status` | GET | Count confirmed vs moved |
| `/api/move/preview` | GET | Show where files would go |

**Request body (POST /api/move):** `{ "movies_dir": "...", "tv_dir": "..." }` — both optional, defaults to config.

**Move job work:**
1. Reads `Confirmed` files from database
2. Moves each file with `MoverService`
3. Updates database records
4. Reports progress via SSE every ~10 files
5. Reports final counts: moved + errors

### Jobs (`web/src/routes/jobs.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/jobs` | GET | List all jobs |
| `/api/jobs/:id` | GET | Get single job info |
| `/api/jobs/:id/cancel` | POST | Cancel a job |

**Response shape:**
```json
{
  "success": true,
  "data": {
    "id": "job_3",
    "kind": "Scan (/mnt/media)",
    "status": "Running",
    "progress_total": 150,
    "progress_processed": 42
  }
}
```

### Ripper (`web/src/routes/ripper.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/ripper/drives` | GET | List available drives |
| `/api/ripper/status` | GET | MakeMKV installation status |
| `/api/ripper/:index/start` | POST | Start ripping on a drive |
| `/api/ripper/:index/cancel` | POST | Cancel rip on a drive |
| `/api/ripper/:index/eject` | POST | Eject disc from drive |
| `/api/ripper/autorip` | GET | Get autorip enabled status |
| `/api/ripper/autorip/toggle` | POST | Toggle autorip on/off |

**Drive detection:** Runs in `spawn_blocking` since `detect_drives()` is a blocking MakeMKV CLI call.

**Start rip flow:**
1. Validates backend exists
2. Resolves destination (explicit param or from config + disc name)
3. Creates `RipHandle` via `backend.start_rip()`
4. Spawns event processing loop (`process_rip_events`)
5. Stores handle in `active_rips` for cancellation
6. Reports `rip:starting:{index}` via SSE

**Event processing loop (`process_rip_events`):**
- Reads events from `RipHandle`
- Calculates ETA from progress data
- Maps events to SSE messages:
  - `Progress` -> `rip:progress:{index}:{pct}[:{eta}]`
  - `Complete` -> `rip:complete:{index}`
  - `Error` -> `rip:error:{index}:{message}`
  - `Message` -> `rip:message:{index}:{message}`

**Autorip polling loop (`autorip_poll_loop`):**
- Polls every 5 seconds
- Detects `Empty -> Ready` transition (disc inserted)
- Skips excluded drives and active rips
- Starts rip in background task
- Broadcasts `drive:changed:{index}:{state}` on state changes
- Respects `autorip_enabled`, `autorip_eject_after`, `autorip_excluded_drives` from config

**Autorip toggle:** Sets `autorip_handle` to `Some(handle)` or `None`. The polling loop checks the handle each iteration and exits when `None`.

### Config (`web/src/routes/config.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/config` | GET | Get current config |
| `/api/config` | PUT | Update config fields |

**Request body (PUT /api/config):**

```json
{
  "database_path": "...",
  "scan_directory": "...",
  "movies_directory": "...",
  "tv_directory": "...",
  "rip_output_directory": "...",
  "verbose": true,
  "host": "0.0.0.0",
  "port": 3000,
  "tmdb_api_key": "...",
  "omdb_api_key": "...",
  "ripping_command": "...",
  "autorip_enabled": true,
  "autorip_eject_after": true,
  "autorip_excluded_drives": [0],
  "healthchecks_uuid": "...",
  "healthchecks_ping_interval_minutes": 5
}
```

**Config update flow:**
1. Acquires write lock on `state.config`
2. Applies each non-None field
3. Saves to file if `config_path` is set
4. **Dynamically restarts healthchecks heartbeat** if `healthchecks_uuid` or `healthchecks_ping_interval_minutes` changed:
   - Aborts existing handle
   - Creates new `HealthchecksClient` from updated config
   - Spawns new heartbeat loop

**Config struct (`infra::Config`):**

| Field | Type | Default |
|-------|------|---------|
| `database_path` | `String` | `"riply.db"` |
| `scan_directory` | `Option<String>` | `None` |
| `movies_directory` | `String` | `"movies"` |
| `tv_directory` | `String` | `"tv"` |
| `rip_output_directory` | `Option<String>` | `None` |
| `verbose` | `bool` | `false` |
| `tmdb_api_key` | `Option<String>` | `None` |
| `omdb_api_key` | `Option<String>` | `None` |
| `ripping` | `RippingConfig` | defaults |
| `healthchecks` | `HealthchecksConfig` | defaults |
| `host` | `String` | `"0.0.0.0"` |
| `port` | `u16` | `3000` |

**RippingConfig:**

| Field | Type | Default |
|-------|------|---------|
| `command` | `Option<String>` | Auto-detected |
| `autorip_enabled` | `bool` | `true` |
| `autorip_eject_after` | `bool` | `true` |
| `autorip_excluded_drives` | `Option<Vec<usize>>` | `None` (all drives) |

**HealthchecksConfig:**

| Field | Type | Default |
|-------|------|---------|
| `uuid` | `String` | `""` (disabled) |
| `ping_interval_minutes` | `u64` | `5` |

### SSE Handler (`web/src/routes/sse_handler.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/sse` | GET | SSE endpoint for real-time updates |

```rust
pub async fn sse_endpoint(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>
```

**Flow:**
1. Subscribes to `SseManager` broadcast channel
2. Yields a `connected` event on first connect
3. Streams all messages from the channel
4. Uses `axum::response::sse::KeepAlive` for persistent connection
5. Messages converted to SSE events via `SseManager::message_to_event()`

## SSE Event Format

All SSE messages follow the format: `event:{name}\ndata:{payload}`

### Event Types

| Event | SSE Type | Source | Example |
|-------|----------|--------|---------|
| `connected` | `Event::connected` | SSE handler | SSE connection established |
| `status` | `send_status(msg)` | Jobs, ripper | `scan:progress:5:10` |
| `log` | `send_log(level, msg)` | Jobs | `warn:Failed to save file.mkv` |
| `drive:changed` | `send_status` | Autorip | `drive:changed:0:Ready` |
| `review:accepted` | `send_status` | Review | `review:accepted:abc123` |
| `review:rejected` | `send_status` | Review | `review:rejected:abc123` |

## Common Patterns

### State Access

All route handlers receive `State(Arc<AppState>)`:

```rust
pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<T>>, (StatusCode, String)> {
    let db = state.db.clone();
    let config = state.config.read().await;
    // ...
}
```

### Blocking Operations

Always use `tokio::task::spawn_blocking` for:
- `backend.detect_drives()` (MakeMKV CLI)
- `infra::check_installation()` (MakeMKV CLI)
- `infra::eject_disc()` (system call)
- Any filesystem I/O or process spawning

```rust
let drives = tokio::task::spawn_blocking(move || backend.detect_drives())
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
```

Note the double `.map_err()`: first unwraps the `JoinError`, then unwraps the inner `Result`.

### Error Handling

Standard pattern for error responses:

```rust
state
    .db
    .get_file_by_id(&id)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, format!("File not found: {}", id)))
```

For optional results, use `.ok_or_else()` after `.map_err()`.

### Config Hot-Reload

Config is wrapped in `Arc<RwLock<Config>>`:

```rust
// Read
let config = state.config.read().await;
let tmdb_key = config.tmdb_api_key.clone();

// Write
let mut config = state.config.write().await;
config.movies_directory = new_dir;
```

### SSE Broadcasting

```rust
sse.send_status(&format!("operation:event:{details}"));
sse.send_log("warn", "Some warning message");
```

## Serving Static Files

The SvelteKit frontend is embedded via `tower_http::services`:

```rust
fn svelte_build_dir() -> std::path::PathBuf {
    let candidates = [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("svelte/build"),
        std::path::PathBuf::from("web/svelte/build"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    candidates[0].clone()
}

let spa_fallback = ServeFile::new(build_dir.join("index.html"));
let static_service = ServeDir::new(&build_dir).not_found_service(spa_fallback);
```

The `fallback_service` serves `index.html` for any unmatched route, enabling client-side routing.

## Server Startup (`cli/src/commands/serve.rs`)

```rust
pub async fn serve(db, config, config_path, host, port) -> Result<(), anyhow::Error> {
    // 1. Resolve host/port
    // 2. Detect MakeMKV backend
    // 3. Build AppState
    // 4. Start autorip if enabled
    // 5. Start healthchecks heartbeat if configured
    // 6. Create router and serve
}
```

**Autorip startup:** If `config.ripping.autorip_enabled` is true, spawns `autorip_poll_loop(state_clone)` and stores the handle in `state.autorip_handle`.

**Healthchecks startup:** If `config.healthchecks.uuid` is non-empty, creates `HealthchecksClient`, calls `start()` (ping `/start` endpoint), then spawns a heartbeat loop that pings the `/` endpoint at the configured interval.

## Testing Patterns

### Config Tests

Use `tempfile::tempdir()` for temporary config files:

```rust
let dir = tempdir().unwrap();
let config_path = dir.path().join("config.toml");
config.save(config_path.to_str().unwrap()).unwrap();
let loaded = Config::load(config_path.to_str().unwrap()).unwrap();
assert_eq!(loaded.database_path, "test.db");
```

### Route Handler Testing

Use `wiremock` to mock HTTP clients and test routes in isolation. For database-dependent routes, use an in-memory SQLite pool.

## Common Pitfalls

1. **Double `.map_err()` for spawn_blocking** — `spawn_blocking` returns `Result<Result<T, E>, JoinError>`. You need two `.map_err()` calls: one for the JoinError, one for the inner error.

2. **Config is RwLock, not Arc** — Always use `.read().await` or `.write().await`. Forgetting the lock causes a borrow checker error.

3. **Backend can be None** — `state.backend` is `Option<Arc<dyn RipBackend>>`. Always check before use and return `SERVICE_UNAVAILABLE` if missing.

4. **Healthchecks restart is manual** — Changing `healthchecks_uuid` or `ping_interval_minutes` in the config does NOT automatically restart the heartbeat. The `update_config` handler must explicitly abort and restart it.

5. **Autorip uses handle check, not cancellation token** — The polling loop checks `state.autorip_handle.read().await.is_none()` each iteration. Toggling off sets the handle to `None`, which causes the next iteration to break.

6. **Drive detection is blocking** — `backend.detect_drives()` calls MakeMKV CLI synchronously. Always wrap in `spawn_blocking`.

7. **SSE channel capacity is 256** — If messages are produced faster than clients consume them, the oldest messages are silently dropped. This is by design but means rapid progress updates may skip.

8. **Static files use fallback, not catch-all** — The `.fallback_service()` pattern serves `index.html` for unmatched routes, enabling SPA routing. Do NOT use `.fallback()` which returns a plain response.
