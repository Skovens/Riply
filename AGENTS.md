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
| Task board (what to do next) | `.opencode/plans/todo.md` |
| Implementation plans | `.opencode/plans/<id>-<name>.md` |
| Architecture decisions | `docs/adr/` |
| Project overview | `README.md` |
| Improvement roadmap | `improvements.md` |

### How to Track Progress

Always start by reading `.opencode/plans/todo.md`. The "Next Up" callout
tells you what to work on. Update task status as you go:
`PENDING` → `IN_PROGRESS` → `DONE`.

See `docs/adr/013-progress-tracking.md` for the full tracking system.

### Before Compaction or Session End

If you've made significant progress or discovered important information:

1. Update the task status in `.opencode/plans/todo.md`
2. Update the relevant plan file (`.opencode/plans/`) with:
   - What was changed and why
   - Important decisions made
   - Remaining work
   - Any blockers or gotchas
3. If a new architectural decision was made, create an ADR in `docs/adr/`

This ensures the next session can pick up without guessing.

---

## Code Conventions

### Rust

- Edition 2021, `max_width = 100`, 4-space indent, Unix newlines
- Prefer compiler-guided design, explicit error handling
- Avoid unnecessary cloning and `Arc<Mutex<T>>`
- Use `anyhow::Result` in app/infra/cli, `thiserror` for domain errors in `core`
- Binary: `riply` (defined in `cli/Cargo.toml`)

### TypeScript/JavaScript (web frontend)

- ES modules (no bundler)
- Components in `web/static/js/components/`
- API client in `web/static/js/api.js`
- CSS custom properties for theming (dark/light)

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
