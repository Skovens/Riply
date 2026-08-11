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
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all
cargo test --all
```

Expected: **0 clippy warnings**, **216+ tests passing**.

The SvelteKit frontend is embedded into the binary at build time via
`web/build.rs` + `rust-embed` (folder `web/svelte/build`). `cargo build` fails
with a clear error if that folder is missing. **Frontend changes require
`npm run build` in `web/svelte` before the Rust build** — the embedded files
are baked in at compile time. The release binary (`target/release/riply`) is
fully self-contained and runs from any directory.

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
| Improvement roadmap | `improvements.md` |

### How to Track Progress

**All progress documentation lives in the repo** — committed task board, plan
files, ADRs, and AGENTS.md. Do not use OpenHands memory dirs or external
notes; anything worth recording must be a committed repo file so it survives
workspace resets.

**Session start ritual — always do this first:**

1. Read `AGENTS.md` (this file).
2. Read `.opencode/plans/todo.md` — the "Next Up" callout tells you what to
   work on. It is the canonical task board; keep it accurate.
3. Run `git --no-pager log --oneline -15` — the git history is the final
   source of truth for what has already been done. **Never re-implement or
   re-commit work that git already shows as done.**

**Session-end handoff checklist** (before finishing or being compacted):

1. **Code done → test → commit → push.** Every feature or fix must be
   committed before you stop. Do not leave uncommitted work; the workspace is
   reset occasionally and uncommitted work is lost.
2. Update task status in `.opencode/plans/todo.md` (`PENDING` →
   `IN_PROGRESS` → `DONE`), add completion notes, and update the "Next Up"
   callout so the next session starts in the right place.
3. Record decisions, blockers, and gotchas in the repo: a decision goes in
   `docs/adr/` (or a `## Decisions` note in the relevant plan file); gotchas
   and conventions go in AGENTS.md.
4. Push your commits to the remote.

Update the task board as you go (not just at the end) so a compaction or
handoff mid-task still has accurate state.

See `docs/adr/013-progress-tracking.md` for the full tracking system.

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

5. **Database is ephemeral**: The SQLite DB is treated as disposable — it holds
   no data that can't be rebuilt on a new scan. Schema changes are made freely
   via **versioned migrations** (`infra/src/migrations/`, applied with
   `sqlx::migrate!()`; currently 001–004). Always add a new numbered migration
   for schema changes rather than mutating an old one — fresh DBs are cheap.
   `riply backup` / `riply restore` also exist for preserving data. The `files`
   enrichment columns use `cast_list` (not `cast`) because `CAST` is a SQLite
   keyword; genres/cast are JSON-encoded TEXT, serialized in `infra/src/db.rs`.

6. **`web` is library-only**: The binary is in `cli` (`riply`). Running
   `cargo run -p web` does nothing — use `cargo run -p cli -- serve` instead.

7. **TimeoutLayer excludes SSE**: `web/src/api.rs` applies a 30s
   `TimeoutLayer` to the API sub-router only. Do NOT move it over the whole
   router — `/v1/sse` is a long-lived stream and would be cut. Static
   fallback is also outside the timed router. Router tests assert both.

8. **New async trait methods** (e.g. `MetadataSearcher`) return
   `Pin<Box<dyn Future<...> + Send + '_>>` (see gotcha 3). When a real
   implementation like `infra::HttpClient` adds such a method, mirror the
   feature in the `web::api`/`app::analyzer` wiring with graceful degradation
   (warn + fallback, never fail the pipeline).
