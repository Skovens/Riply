# AGENTS.md

## Purpose

This file defines how AI coding agents should operate in this repository.

The repository is the source of truth. Conversation history is temporary
and may be compacted or lost. The agent must preserve important context
by writing it down in appropriate places.

---

## Project Overview

Riply is a Rust workspace for ripping, managing, and moving media files
into a structured library.

### Workspace Crates

| Crate | Role | Key Types |
|-------|------|-----------|
| `core` | Domain types, traits, business rules | `FileRecord`, `MediaType`, `RipBackend`, `RipSession`, `MetadataSearcher`, `PipelineStatus`, `DriveInfo`, `DriveState` |
| `app` | Orchestration, services | `ScannerService`, `AnalyzerService`, `MoverService`, `JobScheduler`, `JobInfo`, `JobStatus`, `ProgressHandle` |
| `infra` | I/O adapters (DB, HTTP, MakeMKV) | `Database`, `HttpClient`, `Config`, `MakemkvBackend`, `MakemkvSession`, `DryRunBackend` |
| `web` | HTTP API (Axum) + embedded frontend | `AppState`, `create_router`, `SseManager` |
| `cli` | CLI interface (clap) | Commands: scan, analyze, review, move, rip, serve, status |

### Architecture

```
core (domain) → app (orchestration) → infra (I/O) → web/cli (interfaces)
```

- `core` has zero infrastructure dependencies
- `app` depends only on `core`
- `infra` depends on `core` and `app`
- `web` depends on `core`, `app`, and `infra`
- `cli` depends on all crates

### Import Convention

The `core` package is imported as `riply_core` (not `core`) to avoid
conflicting with Rust's std `core` crate:

```rust
use riply_core::{FileRecord, MediaType, RipBackend};
```

---

## Build & Test Commands

Before completing any Rust changes, run:

```bash
cargo fmt
cargo clippy --all
cargo build --all
cargo test --all
```

Expected: **0 clippy warnings**, **131+ tests passing**.

Quick iteration: `cargo check` (fast compile check).

To run a single test:

```bash
cargo test -p riply_core -- test_name
cargo test -p app -- test_name
cargo test -p infra -- test_name
```

---

## Commit Messages

Use conventional commits with a scope:

```
feat(scope): description
fix(scope): description
refactor(scope): description
```

Examples from this repo:
- `feat(web): add basic dashboard with extensible frontend`
- `fix(web): embed static files in binary so they work from any directory`
- `refactor(ripping): trait-based backend abstraction with Flatpak support`

Scope options: `core`, `app`, `infra`, `web`, `cli`, `scanner`, `metadata`,
`jobs`, `makemkv`, `ripping`, `mover`, `review` (or omit for cross-cutting).

---

## Testing Strategy

- **Unit tests**: inline `#[cfg(test)]` modules in source files
- **Integration tests**: `cli/tests/` for end-to-end CLI testing
- **HTTP mocking**: `wiremock` crate for external API tests (TMDb, OMDb)
- **Property tests**: `proptest` for data validation (file records, media types)
- **Dry-run modes**: `DryRunBackend` for MakeMKV testing without a drive

---

## Context Preservation

### Where to Store Things

| Information | Location |
|-------------|----------|
| Task board / project plan | `.opencode/plans/todo.md` (always update this first) |
| Detailed plan files | `.opencode/plans/<name>.md` |
| Architecture decisions | `docs/adr/` |
| Project overview | `README.md` |

### Before Compaction or Session End

If you've made significant progress or discovered important information,
update the relevant plan file (`.opencode/plans/`) with:

- What was changed and why
- Important decisions made
- Remaining work
- Any blockers or gotchas

This ensures the next session can pick up without guessing.

---

## Code Conventions

### Rust

- Edition 2021, `max_width = 100`, 4-space indent, Unix newlines
- Prefer compiler-guided design, explicit error handling
- Avoid unnecessary cloning and `Arc<Mutex<T>>`
- Use `anyhow::Result` in app/infra/cli, `thiserror` for domain errors in `core`
- Binary: `riply` (defined in `cli/Cargo.toml`)

### TypeScript/JavaScript (web frontend — SvelteKit)

- SvelteKit SPA (`web/svelte/`), static adapter, no SSR
- Pages in `web/svelte/src/routes/`, components in `web/svelte/src/lib/components/`
- API client in `web/svelte/src/lib/api/`
- CSS design system: custom properties in `web/svelte/src/lib/css/` (variables, base, layout, components, utilities)
- Dark mode default, light mode via `prefers-color-scheme`
- SSE for real-time updates (`web/svelte/src/lib/api/sse.ts`)
- Current UI plan: `.opencode/plans/todo.md` Batch 7 (web UI redesign — dashboard layout, detail drawer, per-panel loading)

### Structured Logging

Logging uses `tracing` crate with structured key=value fields — never string
interpolation. Every log line carries parseable fields so AI agents can consume
them directly without human interpretation.

**Flag placement:** `--log-format` is a top-level clap flag on `Cli`. It must
come **before** the subcommand:

```bash
riply --log-format json status       # correct
riply status --log-format json       # error
```

**Formats:** `compact` (default), `pretty` (multi-line, colored), `json` (one
JSON object per line, for programmatic ingestion). Set via `--log-format`.

**Runtime level control:** Use `RUST_LOG` env var (`RUST_LOG=warn`,
`RUST_LOG=debug`). When `--verbose` is passed, defaults to `debug` level.

**Structured field rules:**

| Type | Syntax | Example |
|------|--------|---------|
| `Display` (String, i64, &str) | `%value` | `info!(file = %file.filename, "Moved")` |
| `Debug`-only (Path, PathBuf) | `?value` | `warn!(dir = ?path, error = %e, "Cannot read")` |
| Errors | `%error` | `warn!(error = %e, "Failed")` |

Never use `format!`-style interpolation like `"Failed: {}", val`. Always use
structured fields — the human-readable message is a static string.

**Level conventions:**

| Level | When |
|-------|------|
| `info!` | Normal progress (scan complete, config loaded) |
| `warn!` | Expected failures (unreadable dir, dest exists) |
| `error!` | Unexpected failures (job crash, rip failure, IO error) |

**`#[instrument]` spans:** Use `skip(self)` and capture key args as explicit
fields. Never `skip(self)` without adding fields — that loses context for child
spans:

```rust
// Good — child spans inherit `file`
#[instrument(skip(self), fields(file = %file.filename))]
pub async fn move_file(&self, file: &FileRecord, ...) { ... }

// Bad — no context propagated to child spans
#[instrument(skip(self))]
pub async fn move_file(&self, file: &FileRecord, ...) { ... }
```

**Prefix conventions:** Don't prefix messages with module names (e.g. avoid
`"healthchecks: ping failed"`). The span's `target` field already identifies
the module. Write `warn!(error = %e, "Ping failed")` instead.

### What NOT to Commit

- `riply.db*` (SQLite files)
- `config.toml` (user config)
- `target/` (build artifacts)
- `.env` files

---

## Known Gotchas

1. **Axum 0.7 route syntax**: Use `:param` not `{param}`. Chain methods:
   `.route("/path", get(h1).post(h2))`

2. **MakeMKV is proprietary**: Treat as replaceable backend behind `RipBackend`
   trait. Auto-detects native install, common paths, and Flatpak.

3. **`Pin<Box<dyn Future>>`**: Used in `MetadataSearcher` and `RipSession`
   traits for dyn compatibility. Follow this pattern for new async traits.

4. **Database auto-creation**: Uses `SqlitePoolOptions` with
   `create_if_missing(true)` + WAL journal mode.

5. **`web` is library-only**: The binary is in `cli` (`riply`). Running
   `cargo run -p web` does nothing — use `cargo run -p cli -- serve` instead.
