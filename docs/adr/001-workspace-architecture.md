# ADR-001: Workspace Crate Architecture

## Status

Accepted

## Context

Riply needs a clear separation of concerns across its codebase. The project
handles domain logic, service orchestration, infrastructure I/O, and multiple
interface layers (CLI, web). Without explicit boundaries, dependencies would
tangle and testing would become difficult.

## Decision

Organize the workspace into five crates with a strict dependency hierarchy:

```
core (domain) → app (orchestration) → infra (I/O) → web/cli (interfaces)
```

- `core`: Domain types, traits, business rules. Zero infrastructure deps.
- `app`: Service orchestration, job scheduling. Depends only on `core`.
- `infra`: I/O adapters — database, HTTP, MakeMKV. Depends on `core` + `app`.
- `web`: HTTP API (Axum) + embedded frontend. Depends on all crates.
- `cli`: CLI interface (clap). Depends on all crates.

The `core` crate is imported as `riply_core` to avoid conflicting with
Rust's std `core` crate.

## Consequences

- `core` can be tested in isolation with no external dependencies.
- `app` services can be tested with mock traits from `core`.
- `infra` can swap implementations (e.g., `MakemkvBackend` → `DryRunBackend`)
  without touching `app` or `core`.
- `web` and `cli` are thin interface layers that delegate to `app` services.
- New interface layers (e.g., TUI, daemon) can be added without changing
  domain or orchestration code.
