# Riply Improvements Plan

Research-backed recommendations for improving Riply's architecture, robustness, serviceability, and feature set. All suggestions reference verifiable resources you can read yourself.

---

## Table of Contents

1. [Architecture](#1-architecture)
2. [Robustness](#2-robustness)
3. [Serviceability](#3-serviceability)
4. [Missing Features Worth Adding](#4-missing-features-worth-adding)
5. [Quick Wins (Low Effort, High Impact)](#5-quick-wins-low-effort-high-impact)
6. [Resources for Further Reading](#6-resources-for-further-reading)

---

## 1. Architecture

### 1.1 Current State Assessment

Your workspace follows a **Clean Architecture / Hexagonal Architecture** pattern:

```
core (domain) → app (orchestration) → infra (I/O) → web/cli (interfaces)
```

This is a **good pattern**. The dependency direction is correct — inner layers have zero knowledge of outer layers. `core` has no infrastructure dependencies. This is aligned with established Rust best practices.

**Resources you can read:**
- [Lucio Palmieri - "Rust at Scale"](https://www.lpalmieri.com/posts/2020-11-28-rust-at-scale/) — Clean Architecture in Rust
- [Rust API Guidelines - Flexibility](https://rust-lang.github.io/api-guidelines/flexibility.html) — Traits, generics, and object safety
- [The Rust Book - Workspaces](https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-crates.html)

### 1.2 Recommendation: Split the Monolithic Router

**File:** `web/src/api.rs`

All 30+ API routes are defined in a single `create_router()` function. This makes it hard to find routes, understand ownership, and add new features without touching unrelated code.

**What to do:** Split routes into per-domain modules:

```
web/src/
  api.rs          (combine domain routers, add app-wide middleware)
  files.rs        (GET/POST/DELETE /api/files, /api/files/:id)
  review.rs       (GET /api/review, /api/review/:id/accept, /api/review/:id/reject)
  scan.rs         (POST /api/scan)
  analyze.rs      (POST /api/analyze, /api/analyze/retry-failed)
  mover.rs        (POST /api/move, /api/move/status, /api/move/preview)
  jobs.rs         (GET/DELETE /api/jobs, /api/jobs/:id, /api/jobs/:id/cancel)
  ripper.rs       (GET/POST /api/ripper/*)
  config.rs       (GET/PUT /api/config)
```

**Why:** Each domain becomes independently understandable. Adding a new feature only touches one file.

**Resource:** [Axum Router docs](https://docs.rs/axum/latest/axum/struct.Router.html) — `.merge()` and `.route_layer()` patterns

### 1.3 Recommendation: Add a Custom Error Type for the Web Layer

**File:** `web/src/api.rs`

Currently, handlers return `Result<Json<ApiResponse<T>>, (StatusCode, String)>` — a tuple of status code and error message string. This is inconsistent and loses type safety.

**What to do:** Create a typed error type implementing `IntoResponse`:

```rust
#[derive(Debug, thiserror::Error)]
enum WebError {
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Job not found: {0}")]
    JobNotFound(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        // Map each variant to appropriate status code + message
    }
}
```

**Why:** One consistent error pattern for all handlers. Automatic status code mapping. Better documentation.

**Resources:**
- [Axum error handling docs](https://docs.rs/axum/latest/axum/error_handling/index.html)
- [Axum anyhow error example](https://github.com/tokio-rs/axum/tree/main/examples/anyhow-error-response)

### 1.4 Recommendation: Versioned Database Migrations

**File:** `infra/src/db.rs`

Currently, the migration SQL is embedded via `include_str!` at compile time. This works for one migration but doesn't scale. There's no way to track migration history or roll forward/backward.

**What to do:** Use `sqlx`'s built-in migration system:

1. Create a `migrations/` directory with numbered SQL files
2. Use `sqlx::migrate!()` macro or `sqlx::migrate!("path")` at runtime
3. Run `cargo sqlx migrate resolve` to fix state

**Why:** Versioned migrations are the standard for database evolution. They let you track what changed, when, and why.

**Resources:**
- [SQLx Migrations docs](https://docs.rs/sqlx/latest/sqlx/migrate/)
- [SQLx migrate macro](https://docs.rs/sqlx/latest/sqlx/macro.migrate.html)

### 1.5 Recommendation: Consider Merging `web` Crate into `cli`

**Current situation:** `web` is library-only. The binary is in `cli` (`riply serve`). This is noted as a "known gotcha" in AGENTS.md.

**What to do:** If the web server is tightly coupled to the CLI (which it appears to be), merge `web` into `cli` as a module. Many Rust projects (ripgrep, starship, zellij) use a single binary.

**Why:** Reduces workspace complexity. No more confusion about which crate to run.

**When NOT to do this:** If you plan to have multiple binaries that share the web API (e.g., a daemon binary, a server binary), keep `web` separate.

---

## 2. Robustness

### 2.1 Recommendation: Structured Logging with `tracing`

**Current state:** Basic `tracing` is used but not consistently. No request-level logging, no structured fields.

**What to do:**

1. Initialize `tracing-subscriber` with `EnvFilter` in both CLI and web:
   ```rust
   tracing_subscriber::registry()
       .with(tracing_subscriber::EnvFilter::try_from_default_env()
           .unwrap_or_else(|_| "riply=info,tower_http=debug".into()))
       .with(tracing_subscriber::fmt::layer())
       .init();
   ```

2. Add `#[instrument]` attributes on public functions:
   ```rust
   #[instrument(fields(job_id = %job.id))]
   async fn scan_directory(&self, path: &str) -> Result<Vec<FileRecord>> {
       // ...
   }
   ```

3. Add `TraceLayer` from `tower-http` for HTTP request logging:
   ```rust
   Router::new()
       .merge(api_routes)
       .layer(TraceLayer::new_for_http())
   ```

**Why:** Structured logs are machine-readable, searchable, and include context (job IDs, file paths, status codes). Essential for debugging production issues.

**Resources:**
- [tracing crate docs](https://docs.rs/tracing/)
- [tracing-subscriber docs](https://docs.rs/tracing-subscriber/)
- [tower-http TraceLayer](https://docs.rs/tower-http/latest/tower_http/trace/index.html)

### 2.2 Recommendation: Graceful Shutdown

**Current state:** No explicit graceful shutdown handling. The server stops abruptly on Ctrl+C without waiting for in-flight requests or jobs.

**What to do:** Use `axum::serve().with_graceful_shutdown()`:

```rust
let shutdown_handle = tokio::spawn(async move {
    tokio::signal::ctrl_c().await.expect("failed to listen for ctrl-c");
    tracing::info!("Shutdown signal received, finishing in-flight requests...");
});

axum::serve(listener, app)
    .with_graceful_shutdown(async move {
        shutdown_handle.await.ok();
    })
    .await?;
```

**Why:** Prevents data corruption from interrupted file moves, cancelled jobs leaving DB state inconsistent, and broken SSE connections.

**Resources:**
- [Axum serve docs](https://docs.rs/axum/latest/axum/fn.serve.html)
- [Tokio signal docs](https://docs.rs/tokio/latest/tokio/fn.signal.html)

### 2.3 Recommendation: Database Constraints and Indexes

**Current state:** The `files` table has `UNIQUE(path)` but no CHECK constraints on `status`, no foreign keys, and no indexes on commonly queried fields.

**What to do:**

1. Add CHECK constraints for `PipelineStatus` values:
   ```sql
   CHECK(status IN ('Scanned', 'Matched', 'NeedsReview', 'Confirmed', 'Moved', 'Failed'))
   ```

2. Add indexes on frequently queried columns:
   ```sql
   CREATE INDEX idx_files_canonical_title ON files(canonical_title);
   CREATE INDEX idx_files_tmdb_id ON files(tmdb_id);
   CREATE INDEX idx_files_status ON files(status);
   ```

3. Use `#[repr(u8)]` for `PipelineStatus` and store as integers instead of strings (saves space, prevents typos).

**Why:** Database-level constraints prevent invalid state even if application code has a bug. Indexes dramatically speed up queries on large datasets.

**Resources:**
- [SQLite CHECK constraints](https://www.sqlite.org/checkconstraints.html)
- [SQLite indexes](https://www.sqlite.org/indexes.html)

### 2.4 Recommendation: Retry Logic for HTTP Requests

**Current state:** `HttpClient` doc comment says "handles API requests with retry and rate limiting" but there's no actual retry logic.

**What to do:** Use the `backon` crate for retry with exponential backoff:

```toml
# Cargo.toml
backon = "1.2"
```

```rust
use backon::{Retryable, ExponentialBuilder};

async fn search_tmdb(query: &str) -> anyhow::Result<TmdbResponse> {
    let fetch = || reqwest::get(url).await?.json::<TmdbResponse>().await;
    fetch
        .retry(ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(500))
            .with_max_delay(Duration::from_secs(10))
            .with_max_tries(3))
        .await
}
```

**Why:** External APIs (TMDb, OMDb) can be temporarily unavailable. Silent retries prevent entire jobs from failing on transient errors.

**Resources:**
- [backon crate docs](https://docs.rs/backon/)
- [Exponential backoff on Wikipedia](https://en.wikipedia.org/wiki/Exponential_backoff)

### 2.5 Recommendation: Health Check and Readiness Endpoints

**Current state:** `/health` exists but only returns a static string.

**What to do:** Add two endpoints:

1. `/health` — Liveness check (is the process alive?)
2. `/ready` — Readiness check (is the DB connected? Is MakeMKV available?)

```rust
async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Check DB connectivity
    state.db.ping().await.map_err(|e| {
        tracing::error!("DB ping failed: {}", e);
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    (StatusCode::OK, "ready").into_response()
}
```

**Why:** Essential for container orchestration (Docker, Kubernetes) and load balancers. A process can be "alive" but not "ready" (e.g., DB connection lost).

**Resources:**
- [Kubernetes health checks](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-probes)

### 2.6 Recommendation: Input Validation and CORS

**Current state:** No CORS configuration, no rate limiting, no request validation beyond type checking.

**What to do:**

1. Add CORS layer (even if permissive for now):
   ```rust
   use tower_http::cors::CorsLayer;
   let cors = CorsLayer::new()
       .allow_origin(Any)
       .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE]);
   ```

2. Add request timeout middleware:
   ```rust
   use tower::timeout::TimeoutLayer;
   use std::time::Duration;
   ```

**Resources:**
- [tower-http CorsLayer](https://docs.rs/tower-http/latest/tower_http/cors/index.html)
- [Axum CORS example](https://github.com/tokio-rs/axum/tree/main/examples/cors)

---

## 3. Serviceability

### 3.1 Recommendation: Domain Router Modules (see 1.2)

Splitting the router into per-domain modules is the single biggest improvement for serviceability. Each feature lives in its own file with clear ownership.

### 3.2 Recommendation: CLI Progress Indicators

**Current state:** CLI commands (`scan`, `analyze`, `move`) have no progress feedback during long operations.

**What to do:** Use `indicatif` for progress bars:

```toml
# Cargo.toml
indicatif = "0.17"
```

```rust
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total_files);
pb.set_style(ProgressStyle::default_bar()
    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
    .unwrap());

for file in files {
    // process...
    pb.inc(1);
}
pb.finish_with_message("Scan complete");
```

**Why:** Users need feedback during long operations. This is especially important for the `scan` and `move` commands which can take minutes on large directories.

**Resources:**
- [indicatif docs](https://docs.rs/indicatif/)
- [Rust CLI Book - Communicating with Humans](https://rust-cli.github.io/book/guide/human-communication.html)

### 3.3 Recommendation: `--json` Output Flag for CLI

**Current state:** CLI output is human-readable text only.

**What to do:** Add `--json` flag to commands that produce output (`status`, `rip list`, etc.):

```rust
#[derive(Parser)]
struct StatusArgs {
    #[arg(long)]
    json: bool,
}

fn status(args: StatusArgs) {
    let counts = db.count_files_by_status_all().await.unwrap();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&counts).unwrap());
    } else {
        // human-readable format
    }
}
```

**Why:** Enables scripting and automation. Tools like `jq` can parse structured output.

**Resource:** [Rust CLI Book - Output for Humans and Machines](https://rust-cli.github.io/book/guide/output.html)

### 3.4 Recommendation: Metrics Endpoint

**Current state:** No observability beyond basic logging.

**What to do:** Add Prometheus metrics for HTTP requests and job operations:

```toml
# Cargo.toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
```

```rust
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

fn setup_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            metrics::histogram!("http_requests_duration_seconds"),
            &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
        )
        .unwrap()
        .install_recorder()
        .unwrap()
}
```

**Why:** Lets you track request rates, error rates, and latency over time. Essential for understanding system behavior in production.

**Resources:**
- [metrics crate docs](https://docs.rs/metrics/)
- [Axum prometheus example](https://github.com/tokio-rs/axum/tree/main/examples/prometheus-metrics)

### 3.5 Recommendation: Hot Reload for Development

**Current state:** Manual restart required for both backend and frontend changes.

**What to do:**

1. Install `cargo-watch`:
   ```bash
   cargo install cargo-watch
   ```

2. Run with auto-reload:
   ```bash
   cargo watch -x "run -p cli -- serve"
   ```

3. For frontend: Use `cargo-watch` with a separate SvelteKit dev server during development.

**Resource:** [cargo-watch docs](https://crates.io/crates/cargo-watch)

### 3.6 Recommendation: Config Validation

**Current state:** Config is loaded from TOML but not validated. Missing keys use defaults, invalid values are silently accepted.

**What to do:** Use the `config` crate with validation:

```toml
# Cargo.toml
config = { version = "0.15", features = ["toml", "env"] }
```

```rust
use config::{Config, File, Environment};

fn load() -> anyhow::Result<Config> {
    let config = Config::builder()
        .set_default("host", "127.0.0.1")?
        .set_default("port", 3000)?
        .add_source(File::with_name("config").required(true))
        .add_source(Environment::with_prefix("RIPLY").separator("__"))
        .build()?;

    config.try_deserialize::<AppConfig>()
        .map_err(Into::into)
}
```

**Why:** Catches configuration errors at startup rather than at runtime. Environment variable support lets you override config per-environment.

**Resources:**
- [config crate docs](https://docs.rs/config/)
- [config-rs GitHub](https://github.com/rust-cli/config-rs)

---

## 4. Missing Features Worth Adding

### 4.1 Duplicate Detection

**Current state:** `FileRecord` has a `duplicate_path` field but no duplicate detection logic.

**What to do:** Add a service that identifies duplicate files by hash (already computed during scan) and marks them in the review queue.

**Why:** Prevents moving the same file twice. Common in media management workflows.

### 4.2 Library Scanning (Reverse Direction)

**Current state:** Riply scans *downloads* and moves files *into* the library. There's no way to scan an existing library and import it.

**What to do:** Add a `riply library scan` command that walks the library directories and creates `FileRecord`s for existing files.

**Why:** Users often have existing media collections they want to manage with Riply.

### 4.3 Metadata Enrichment

**Current state:** Basic metadata from TMDb/OMDb (title, year, poster). No genres, runtime, cast, or content rating.

**What to do:** Fetch additional metadata fields:
- Genres
- Runtime
- Cast (top 5 actors)
- Content rating (PG-13, R, etc.)
- Overview/description

**Why:** Rich metadata improves the web UI and helps with library organization.

### 4.4 Watch Directory / Filesystem Events

**Current state:** Scanning requires manual command execution.

**What to do:** Use `notify` crate to watch directories for new files and trigger automatic scanning:

```toml
# Cargo.toml
notify = "7.0"
```

**Why:** Continuous media management without manual intervention.

**Resource:** [notify crate docs](https://docs.rs/notify/)

### 4.5 Backup and Restore

**Current state:** No way to back up or restore the Riply database.

**What to do:** Add `riply backup` and `riply restore` commands that use SQLite's `ATTACH DATABASE` + `COPY` for consistent backups.

**Why:** The database is the heart of the application. Losing it means losing all scan/analysis state.

**Resource:** [SQLite backup API](https://www.sqlite.org/backup.html)

### 4.6 API Versioning

**Current state:** API has no versioning. Breaking changes would require a major version bump.

**What to do:** Add version prefix to API routes:

```rust
Router::new()
    .nest("/api/v1", v1_routes())
```

**Why:** Lets you evolve the API without breaking existing clients.

**Resource:** [REST API versioning patterns](https://restfulapi.net/versioning/)

---

## 5. Quick Wins (Low Effort, High Impact)

These are small changes that make a noticeable difference:

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

## 6. Resources for Further Reading

### Rust Fundamentals
| Topic | Resource | URL |
|-------|----------|-----|
| Error handling | The Rust Book, Ch 9 | https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html |
| Error handling (thiserror vs anyhow) | "Rust Error Handling Guide" | https://blog.logrocket.com/error-handling-rust/ |
| Async patterns | Async Rust Book | https://rust-lang.github.io/async-book/ |
| Async traits | David Tolnay - "async fn in traits are hard" | https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/ |
| API guidelines | Rust API Guidelines | https://rust-lang.github.io/api-guidelines/ |

### Architecture & Design
| Topic | Resource | URL |
|-------|----------|-----|
| Clean Architecture in Rust | Lucio Palmieri | https://www.lpalmieri.com/posts/2020-11-28-rust-at-scale/ |
| Workspace management | The Rust Book, Ch 7 | https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-crates.html |
| Hexagonal Architecture | Alistair Cockburn | https://alistair.cockburn.us/hexagonal-architecture/ |
| Rust design patterns | Rust Unofficial | https://rust-unofficial.github.io/design-patterns/ |

### Web Development (Axum)
| Topic | Resource | URL |
|-------|----------|-----|
| Axum docs | Official docs | https://docs.rs/axum/ |
| Error handling | Axum examples | https://github.com/tokio-rs/axum/tree/main/examples |
| Middleware | tower-http docs | https://docs.rs/tower-http/ |
| SSE | Axum SSE example | https://github.com/tokio-rs/axum/tree/main/examples/sse |
| Static files | Axum static file example | https://github.com/tokio-rs/axum/tree/main/examples/static-file-server |
| Prometheus metrics | Axum prometheus example | https://github.com/tokio-rs/axum/tree/main/examples/prometheus-metrics |

### CLI Development
| Topic | Resource | URL |
|-------|----------|-----|
| Clap derive | Clap tutorial | https://docs.rs/clap/latest/clap/_tutorial/index.html |
| CLI best practices | Rust CLI Book | https://rust-cli.github.io/book/ |
| Progress bars | indicatif docs | https://docs.rs/indicatif/ |

### Database
| Topic | Resource | URL |
|-------|----------|-----|
| SQLx migrations | SQLx docs | https://docs.rs/sqlx/latest/sqlx/migrate/ |
| SQLite constraints | SQLite docs | https://www.sqlite.org/checkconstraints.html |
| SQLite indexes | SQLite docs | https://www.sqlite.org/indexes.html |

### Observability
| Topic | Resource | URL |
|-------|----------|-----|
| Structured logging | tracing docs | https://docs.rs/tracing/ |
| Log filtering | tracing-subscriber | https://docs.rs/tracing-subscriber/ |
| HTTP request tracing | tower-http TraceLayer | https://docs.rs/tower-http/latest/tower_http/trace/index.html |
| Metrics | metrics crate | https://docs.rs/metrics/ |

### Testing
| Topic | Resource | URL |
|-------|----------|-----|
| Property-based testing | proptest docs | https://docs.rs/proptest/ |
| HTTP mocking | wiremock docs | https://docs.rs/wiremock/ |
| SQLx tests | SQLx test attribute | https://docs.rs/sqlx/latest/sqlx/attr.test.html |

### Security
| Topic | Resource | URL |
|-------|----------|-----|
| Rust security | Rustsec advisory database | https://rustsec.org/ |
| Dependency auditing | cargo-audit | https://docs.rs/cargo-audit/ |
| CORS | MDN CORS guide | https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS |
| Rate limiting | tower-limit docs | https://docs.rs/tower-limit/ |

### Resilience Patterns
| Topic | Resource | URL |
|-------|----------|-----|
| Retry with backoff | backon crate | https://docs.rs/backon/ |
| Circuit breaker | "Circuit Breaker Pattern" | https://docs.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker |
| Timeout | tower TimeoutLayer | https://docs.rs/tower/latest/tower/timeout/ |
| Concurrency limit | tower ConcurrencyLimitLayer | https://docs.rs/tower/latest/tower/concurrency_limit/ |

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
