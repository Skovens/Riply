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

## Toolchain Installation

**Always install toolchains before working on the project.** Never assume
they are present. Check availability first, install if missing.

### Rust

```bash
# Check if available
rustc --version 2>/dev/null || echo "NOT INSTALLED"

# Install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Make available in current session
source "$HOME/.cargo/env"

# Add required components
rustup component add clippy
```

### Node.js

```bash
# Check if available
node --version 2>/dev/null || echo "NOT INSTALLED"
npm --version 2>/dev/null || echo "NOT INSTALLED"

# Install via nvm if needed
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
source "$HOME/.nvm/nvm.sh"
nvm install --lts
```

### After Installation

After installing any toolchain, **verify the build succeeds** before starting
work:

```bash
cargo build && cd web/svelte && npm run build && cd ../..
```

If the build fails, fix the pre-existing issues before proceeding.

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
4. **Install missing toolchains.** If the project uses Rust, Python, Node,
   etc., check if the toolchain is available. If not, install it immediately.
   See "Toolchain Installation" below.
5. **Verify the codebase builds before starting work.** Run the full build
   pipeline (Rust: `cargo build`; Svelte: `npm run build` in `web/svelte/`)
   to catch any pre-existing issues.

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

9. **Workspace can be wiped mid-session** — the checkout directory (and
   its `.git`) may be reset to an empty repo with no commits, no remote,
   and no git identity. Recovery: re-clone with `gh repo clone Skovens/Riply`
   using `GH_TOKEN=$GITHUB_TOKEN` (gh is not otherwise authed), re-set the
   remote URL to `https://${GITHUB_TOKEN}@github.com/Skovens/Riply.git`,
   and re-set `git config user.name`/`user.email` (fresh clones have none,
   so commits fail with "unable to auto-detect email address"). Everything
   pushed to `origin/main` survives; everything uncommitted is lost.

10. **`web/svelte/build/` is gitignored and baked into the binary at compile
    time** — after any `git pull`, the embedded frontend is stale (the pull
    refreshes source but not the build folder). Running `cargo run -p cli -- serve`
    after a pull serves the old frontend unless you first run `npm run build`
    in `web/svelte` (or `make install`, which rebuilds). If a user reports
    missing UI features right after pulling, this is the first thing to check.
    Rip progress in the web UI comes from `rip:progress` SSE events (backend
    `web/src/routes/ripper.rs`); the dashboard renders them inline in the
    drive card via `ripProgress` state in `web/svelte/src/routes/+page.svelte`.

---

## Workflow Rules — Preventing "I Said It's Done But It Isn't"

These rules exist to prevent the pattern where the agent claims work is complete
and verified, but the user finds it doesn't actually work. **Follow them strictly.**

### Claim Levels — Never Say "Done" Without the Right Level

| Claim | What it means | What you MUST do |
|-------|---------------|------------------|
| **Draft** | Code written, unverified | Show the diff, explain changes |
| **Verified** | Compiles, tests pass, output shown | Actually run build/tests and paste output |
| **Ready** | Verified + user has reviewed | Wait for user approval before moving on |

**Never claim "done" or "tested" unless you've actually run verification.**
If you can't verify something (e.g., no toolchain installed), say:
> "I wrote the code but couldn't verify it locally because [reason].
> Please check on your machine: [list of commands]."

### Before Coding — Write Acceptance Criteria

For every task, write acceptance criteria **before** writing code:

```markdown
Acceptance criteria:
- [ ] `unique_rip_destination("Rips/Inception", Some("Inception"))` returns
      "Rips/Inception (Copy 1)" when dir exists with files
- [ ] CLI `riply rip start` on Ctrl+C cancels the rip and exits cleanly
- [ ] `cargo clippy -- -D warnings` passes with 0 warnings
- [ ] `cargo test` passes with no failures
```

Show these to the user for approval before starting implementation.

### After Coding — Show Diff + Verification

Instead of:
> Done, pushed.

Say:
> Here's what I changed:
> ```
> cli/src/commands/rip.rs: +50 lines
> infra/src/makemkv/backend.rs: +10 lines
> ```
>
> Verification:
> ```
> $ cargo build && cargo clippy --all-targets -- -D warnings && cargo test
> Finished `dev` profile... 0 warnings, 216 tests passed
> ```
>
> I couldn't verify X because [reason]. Check on your machine:
> - `command to run`
>
> Acceptance criteria status:
> - [x] Criterion 1 — verified
> - [x] Criterion 2 — verified
> - [ ] Criterion 3 — I can't check this, please verify
>
> Ready for your review.

### What NOT to Do

- **Don't** say "I tested it" without showing test output
- **Don't** say "clippy is clean" without running clippy
- **Don't** say "the build passes" without running the build
- **Don't** write code based on assumptions about what the user described —
  confirm ambiguous requirements before implementing
- **Don't** skip the build/test step even if it takes time

### When the User Says "It's Still Wrong"

Respond with:
1. What I changed vs what you asked for (diff or description)
2. What I actually verified vs what I assumed
3. What I can't verify without your machine
4. What exactly needs to change

No more "I tested it" claims unless I actually ran the tests.

### Toolchain Installation — Always Check First

Never assume a toolchain is installed. Always check and install if missing:

1. Check: `rustc --version` (or equivalent for the language)
2. If not installed: install it (see "Toolchain Installation" section above)
3. After installing: verify the build succeeds
4. Only then: start working on the task

This is part of the session start ritual (step 4).
