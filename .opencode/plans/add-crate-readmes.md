# Add Crate READMEs

## Status: DONE

## Problem

No crate-level documentation exists. An agent working on `app/src/mover.rs`
has to read every file in the `app` crate to understand its purpose. Crate
READMEs give agents a quick entry point without reading all source code.

## Plan

Create a `README.md` in each workspace crate. Each should be 10-20 lines
describing: what the crate does, key public types, how to test it, and
common patterns to follow.

### `core/README.md`

**Purpose:** Domain types, traits, business rules. Zero infrastructure deps.

**Key public types:**
- `FileRecord` — Media file metadata (path, status, title, year, etc.)
- `MediaType` — Movie or TV episode
- `MetadataMatch` / `MetadataSearcher` — Trait for searching TMDb/OMDb
- `PipelineStatus` — File lifecycle states (Scanned → Analyzed → Reviewed → Moved)
- `RipBackend` / `RipSession` — Traits for ripping backends (MakeMKV, DryRun)
- `DriveInfo` / `DriveState` / `RipEvent` / `RipError` — Ripping domain types

**Testing:** `cargo test -p riply_core`

**Patterns:** Use `thiserror` for domain errors. Import as `riply_core::`.

### `app/README.md`

**Purpose:** Service orchestration and job scheduling. Depends only on `core`.

**Key public types:**
- `ScannerService` — Walk directories, hash files, produce `FileRecord`s
- `AnalyzerService` — Parse filenames, search metadata APIs
- `MoverService` — Calculate destination paths, move files
- `JobScheduler` — Background job management with progress tracking

**Testing:** `cargo test -p app`

**Patterns:** Services are zero-sized structs with static methods. Use
`anyhow::Result` for errors. Job closures use `tokio::spawn`.

### `infra/README.md`

**Purpose:** I/O adapters — database, HTTP clients, MakeMKV integration.

**Key public types:**
- `Database` — SQLite persistence (save, query, update file records)
- `HttpClient` — TMDb v3 and OMDb API client
- `Config` — TOML configuration management
- `MakemkvBackend` / `MakemkvSession` — MakeMKV CLI integration
- `DryRunBackend` — Testing backend that simulates ripping

**Testing:** `cargo test -p infra`

**Patterns:** Uses `anyhow::Result`. Database uses `sqlx` with WAL mode.
HTTP client uses `reqwest`. MakeMKV parses robot-mode output.

### `web/README.md`

**Purpose:** Axum HTTP API + embedded frontend.

**Key public types:**
- `AppState` — Shared application state (db, config, backend, jobs)
- `create_router` — Builds the Axum router with all endpoints
- `SseManager` — Server-Sent Events broadcast channel

**Testing:** `cargo test -p web`

**Patterns:** API returns `Json<ApiResponse<T>>`. SSE uses
`event:status`, `event:progress`, `event:log` events. Frontend
is embedded via `rust-embed`.

### `cli/README.md`

**Purpose:** CLI binary entry point with clap subcommands.

**Key subcommands:**
- `scan` — Scan directory for media files
- `analyze` — Analyze files against metadata APIs
- `review` — Interactive review of matched files
- `move` — Move confirmed files to library
- `rip` — Disc ripping with MakeMKV
- `serve` — Start web server
- `status` — Show pipeline status

**Testing:** `cargo test -p cli`

**Patterns:** Uses `clap` derive macros. Commands delegate to `app`
services. Binary name: `riply`.

## Files

| File | Action |
|------|--------|
| `core/README.md` | Create |
| `app/README.md` | Create |
| `infra/README.md` | Create |
| `web/README.md` | Create |
| `cli/README.md` | Create |

## Verification

After creating all 5 READMEs:
- Each should be under 30 lines ✅
- Key types match actual `pub use` exports ✅
- Testing commands are correct ✅
- Patterns match AGENTS.md conventions ✅
- `cargo clippy --all` — 0 warnings ✅
- `cargo test --all` — all passing ✅
