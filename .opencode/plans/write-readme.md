# Write README.md

## Status: DONE

## Problem

The current README.md is only 2 lines:

```
# Riply
Rip, manage, move media files
```

No onboarding, no usage, no architecture overview. Agents and humans
alike have no quick entry point.

## What the README Should Contain

1. **Project description** — What Riply does (1-2 sentences)
2. **Features** — Bullet list of capabilities
3. **Architecture** — Workspace crate diagram with dependency graph
4. **Quick start** — Clone, build, run
5. **CLI usage** — Common commands (scan, analyze, move, serve)
6. **Web UI** — How to access the dashboard
7. **Configuration** — Where config lives, key settings
8. **Development** — How to build, test, lint
9. **Agent instructions** — Pointer to AGENTS.md

## Architecture to Document

```
core (domain) → app (orchestration) → infra (I/O) → web/cli (interfaces)
```

Crate summary:
- `core` — Domain types, traits, business rules
- `app` — Services, job scheduler, orchestration
- `infra` — SQLite database, HTTP clients, MakeMKV backend
- `web` — Axum HTTP API + embedded frontend
- `cli` — CLI binary with clap

## Files

| File | Action |
|------|--------|
| `README.md` | Rewrite (currently 2 lines) |

## Files Modified

| File | Action |
|------|--------|
| `README.md` | Rewritten (2 lines → full README) |

## Verification

After writing, verify:
- All 5 crates are mentioned ✅
- Architecture diagram is accurate ✅
- Quick start commands are correct (`cargo build --all`, `cargo run -p cli -- serve`) ✅
- Link to AGENTS.md is present ✅
- `cargo clippy --all` — 0 warnings ✅
- `cargo test --all` — all passing ✅
