# Riply Priority Roadmap

Recommendations for improving Riply, ordered by impact vs effort.

---

## Architecture

### Current State Assessment

The workspace follows a **Clean Architecture / Hexagonal Architecture** pattern:

```
core (domain) → app (orchestration) → infra (I/O) → web/cli (interfaces)
```

This is a good pattern. The dependency direction is correct — inner layers have zero knowledge of outer layers.

### Split the Monolithic Router

**File:** `web/src/api.rs` — All 30+ API routes are defined in a single `create_router()` function.

**What to do:** Split routes into per-domain modules:

```
web/src/
  api.rs          (combine domain routers, add app-wide middleware)
  organizer.rs    (folder operations)
  ripper.rs       (drive/rip operations)
  config.rs       (configuration)
  jobs.rs         (job operations)
```

**Why:** Each domain becomes independently understandable. Adding a new feature only touches one file.

**Resource:** [Axum Router docs](https://docs.rs/axum/latest/axum/struct.Router.html)

### Add a Custom Error Type for the Web Layer

**File:** `web/src/api.rs` — Handlers return `Result<Json<ApiResponse<T>>, (StatusCode, String)>`.

**What to do:** Create a typed error type implementing `IntoResponse`:

```rust
#[derive(Debug, thiserror::Error)]
enum WebError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Bad request: {0}")]
    BadRequest(String),
}
```

**Why:** One consistent error pattern for all handlers. Automatic status code mapping.

### Consider Merging `web` Crate into `cli`

**Current situation:** `web` is library-only. The binary is in `cli` (`riply serve`).

**What to do:** If the web server is tightly coupled to the CLI (which it appears to be), merge `web` into `cli` as a module.

**When NOT to do this:** If you plan to have multiple binaries that share the web API, keep `web` separate.

---

## Robustness

### Structured Logging with `tracing`

**Current state:** Basic `tracing` is used but not consistently. No request-level logging.

**What to do:**
1. Initialize `tracing-subscriber` with `EnvFilter`
2. Add `#[instrument]` attributes on public functions
3. Add `TraceLayer` from `tower-http` for HTTP request logging

**Why:** Structured logs are machine-readable, searchable, and include context.

### Graceful Shutdown

**Current state:** No explicit graceful shutdown handling.

**What to do:** Use `axum::serve().with_graceful_shutdown()` to finish in-flight requests and jobs.

**Why:** Prevents data corruption from interrupted file moves and cancelled jobs.

### Database Constraints and Indexes

**Current state:** The `files` table has `UNIQUE(path)` but no CHECK constraints on `status`, no indexes on commonly queried fields.

**What to do:**
1. Add CHECK constraints for `PipelineStatus` values
2. Add indexes on frequently queried columns

---

## Missing Features Worth Adding

### Duplicate Detection

**Current state:** `FileRecord` has a `duplicate_path` field but no duplicate detection logic.

**What to do:** Add a service that identifies duplicate files by hash and marks them in the review queue.

### Library Scanning (Reverse Direction)

**Current state:** Riply scans *downloads* and moves files *into* the library. There's no way to scan an existing library.

**What to do:** Add a `riply library scan` command that walks the library directories and creates records for existing files.

### Metadata Enrichment

**Current state:** Basic metadata from TMDb/OMDb (title, year, poster). No genres, runtime, cast, or content rating.

**What to do:** Fetch additional metadata fields: genres, runtime, cast (top 5 actors), content rating, overview.

### Watch Directory / Filesystem Events

**Current state:** Scanning requires manual command execution.

**What to do:** Use `notify` crate to watch directories for new files and trigger automatic scanning.

### Backup and Restore

**Current state:** No way to back up or restore the Riply database.

**What to do:** Add `riply backup` and `riply restore` commands using SQLite's `ATTACH DATABASE` + `COPY`.

### API Versioning

**Current state:** API has no versioning. Breaking changes would require a major version bump.

**What to do:** Add version prefix to API routes: `Router::new().nest("/api/v1", v1_routes())`

---

## Quick Wins (Low Effort, High Impact)

| # | Change | Effort | Impact | Location |
|---|--------|--------|--------|----------|
| 1 | Add `#[instrument]` to public functions | Low | High | `app/src/*.rs` |
| 2 | Add `TraceLayer` to HTTP router | Low | Medium | `web/src/api.rs` |
| 3 | Add CHECK constraints to DB | Low | Medium | `infra/migrations/` |
| 4 | Add `--json` flag to `riply status` | Low | Low | `cli/src/main.rs` |
| 5 | Add `--json` flag to `riply rip list` | Low | Low | `cli/src/main.rs` |
| 6 | Replace `(StatusCode, String)` with typed errors | Medium | High | `web/src/api.rs` |
| 7 | Split router into domain modules | Medium | High | `web/src/` |
| 8 | Add retry logic to HTTP client | Low | Medium | `infra/src/http.rs` |
| 9 | Add graceful shutdown | Low | Medium | `cli/src/main.rs` |
| 10 | Add progress bars to CLI | Medium | Medium | `cli/src/main.rs` |

---

## Priority Roadmap

If you want to tackle these in order of impact vs effort:

**Phase 1 — Quick wins (this weekend):**
1. Add `--json` flags to CLI output commands
2. Add retry logic to HTTP client (`backon`)
3. Add graceful shutdown to server
4. Add `TraceLayer` to HTTP router

**Phase 2 — Structure improvements (next week):**
5. Split router into domain modules
6. Add typed error type (`WebError`)
7. Add database constraints and indexes
8. Add `#[instrument]` to key functions

**Phase 3 — Observability (ongoing):**
9. Add health/readiness endpoints
10. Add metrics endpoint (Prometheus)
11. Add config validation (`config` crate)

**Phase 4 — Features:**
12. Duplicate detection
13. Library scanning
14. Watch directory / filesystem events
15. CLI progress bars
