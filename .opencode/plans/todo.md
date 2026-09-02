# Riply — Agent Task Board

## Next Up

> **Next Up (2026-08-31, hardware-verification):** Full real-hardware test of the
> DVD ripping subsystem is DONE (see `hardware-verification-2026-08.md`). It
> surfaced **4 bugs**, now tracked as **GitHub issues** (this is the canonical
> tracker going forward — see `#10`–`#13`). Pick one, one at a time:
>
> - **#12 — Organizer move writes to hardcoded `TODO_MOVIE_DIR`** (final move
>   non-functional; highest user impact on the organize path).
> - **#11 — No live rip progress / ETA** (UI bar frozen; recommended fix is a
>   file-size-growth fallback in `process_rip_events`).
> - **#10 — Rip cancel leaves orphaned `makemkvcon`** (kill the process group,
>   not just the `flatpak`/`bwrap` wrapper).
> - **#13 — `flatpak_permissions_ok` true but rip dir not sandbox-writable**
>   (add a destination-writability probe; fail fast instead of silent empty rip).
>
> Deferred (needs hardware we don't have): TV-series DVD, mid-rip disc removal,
> multi-drive, `Loading` state. TMDb/OMDb auto-naming + Organizer TMDb matching
> could not be exercised here (API keys not reachable in the agent sandbox) —
> re-test once `tmdb_api_key`/`omdb_api_key` are in `config.toml`.
>
> **Previous:** No pending roadmap items — all of `improvements.md` (Batch 5,
> §2–§4) is DONE. The last "consider" item **#31** (Merge `web` crate into `cli`) was
> **decided against on 2026-08-02** — pure internal refactor with no user value;
> keeping `web` separate preserves the option of a future second binary (daemon /
> other UI) sharing the router. The roadmap is now fully closed out.
>
> **Last session (2026-08-13):** Fixed two web-UI gaps a user hit after pulling:
> - **Rip progress bars in the web UI** — backend already emitted `rip:progress`
>   SSE events with % and ETA, but the dashboard never handled them. Wired them up
>   in `web/svelte/src/routes/+page.svelte` (`ripProgress` state + inline striped
>   bar in the Drives card). No backend change.
> - **Name media when starting a rip** — the Rip button now opens the shared
>   `openMediaNamingModal` (ADR-022) pre-filled from the parsed disc name; the
>   chosen title (+year) becomes the rip subdirectory name.
> - Root cause of the user's symptom: `web/svelte/build/` is gitignored, so after
>   `git pull` the embedded frontend was stale — added gotcha 10 in AGENTS.md
>   (run `npm run build` in `web/svelte` before `cargo run -p cli -- serve`).
> - Also fixed pre-existing failures the latest commits left behind: vitest
>   `request.test.ts` still asserted `/api/` paths after the `/v1/` migration,
>   and clippy errors in `infra/src/makemkv/dryrun.rs` (moved `collect_events`
>   into `mod tests`, `while let Some(_)` → `.is_some()`).
> Full suite green: 220 tests pass, clippy clean, frontend vitest 14/14.

### Batch 1: Quick Frontend & Config Fixes ✅

| # | Task | Status | Plan File |
|---|------|--------|-----------|
| 1 | Fix hardcoded scan directory in layout | DONE | `fix-hardcoded-dir.md` |
| 2 | Fix stale TypeScript types | DONE | `fix-ts-types.md` |
| 3 | Fix SQL migration casing + filter value | DONE | `fix-migration-casing.md` |

### Batch 2: Backend Hardening ✅

| # | Task | Status | Plan File |
|---|------|--------|-----------|
| 4 | Cross-device move fallback (copy+delete) | DONE | `cross-device-move.md` |
| 5 | Regex lazy_static in Analyzer | DONE | `regex-lazy-static.md` |
| 6 | SQL pagination (LIMIT/OFFSET) | DONE | `sql-pagination.md` |
| 7 | Fix cancel_rip CLI no-op | DONE | `fix-cancel-rip.md` |

### Batch 3: UX Improvements ✅

| # | Task | Status | Plan File |
|---|------|--------|-----------|
| 8 | Review page: add metadata editing before accept | DONE | `review-editing.md` |
| 9 | Move preview in web UI | DONE | `move-preview.md` |
| 10 | File text search + batch retry failed | DONE | `file-search-retry.md` |

### Batch 4: Major Features ✅

| # | Task | Status | Plan File |
|---|------|--------|-----------|
| 11 | ffprobe metadata extraction (duration, resolution, content_type) | DONE | `ffprobe-metadata.md` |
| 12 | Duplicate detection (hash-based) | DONE | `duplicate-detection.md` |

### Batch 6: Ripping Overhaul ✅

| # | Task | Status | Depends On |
|---|------|--------|------------|
| 33 | Extract eject to `infra` + fix device path mapping (parse 7th DRV field) | DONE | — |
| 34 | Config validation: refuse to rip if required directories not set | DONE | — |
| 35 | Disc name parsing: extract title + year from disc name | DONE | — |
| 36 | Auto-create parent directory from disc name when ripping | DONE | #35 |
| 37 | Rip progress with time-remaining estimate (15s stabilization) | DONE | — |
| 38 | Auto-rip: background polling task detects disc insertion, starts rip | DONE | #33, #34, #36 |
| 39 | Per-drive autorip toggle in settings + config | DONE | #38 |
| 40 | Fix time estimation display (SSE ETA not rendering in UI) | DONE | Fixed rate calculation: delta_pct/delta_time instead of delta_pct/total_elapsed |
| 41 | Add per-drive autorip toggle UI (independent on/off per drive card) | DONE | #39 |
| 42 | Add progress bar with percentage display during rip | DONE | Animated striped bar + `--done` state + accent-colored text |
| 43 | Optimize dashboard loading during active rip (SSE polling blocking?) | DONE | — |

### Batch 6a: Auto-rip lifecycle rework ✅

**Date:** 2026-08-04 — Changed autorip from a persisted, on-by-default config setting to a
**per-drive, session-only** toggle (never auto-rips at startup).

**Changes:**
- `infra/src/config.rs` — removed `RippingConfig.autorip_enabled` and `.autorip_excluded_drives`;
  kept `autorip_eject_after`. Old `config.toml` keys are ignored (serde) — no migration.
- `web/src/api.rs` — added `AppState.autorip_drives: RwLock<HashSet<usize>>` (in-memory, empty at
  startup). Routes: `GET /api/ripper/autorip` → `Vec<usize>`; `POST /api/ripper/autorip/:index/toggle`.
- `web/src/routes/ripper.rs` — `autorip_poll_loop` now runs for the server lifetime but does zero
  `detect_drives` calls while a rip is active or when no drive has autorip enabled. Only drives in
  the enabled set are tracked (`prev_states` pruned each tick), so enabling a drive with a disc
  present starts the rip, and re-rips are prevented via `prev_states` (Empty/None → Ready trigger).
  Disabling autorip mid-rip does not cancel the running rip.
- `cli/src/commands/serve.rs` — always spawns the monitor; aborts it on graceful shutdown so
  Ctrl+C exits promptly.
- Frontend — `DrivePanel.svelte`: single Rip/Cancel button per drive (swaps while ripping) + a
  per-drive auto-rip toggle. `types.ts`/`ripper.ts` updated.

**Tests:** `web/tests/api.rs` — default-off, toggle on/off session-only, multi-drive, config
ignores `autorip_enabled`.

### Legacy (folded into above)

| Task | Status | Folded Into |
|------|--------|-------------|
| Remove dead code | FOLDED | #2 (types), #3 (migration), #12 (duplicate_path) |
| TV metadata enrichment | FOLDED | #11 (ffprobe), #8 (review editing) |
| Wire pipeline UI | DONE | — |
| MakeMKV abstraction | DONE | — |
| Migrate to SvelteKit | DONE | — |

### Batch 7: Web UI Redesign 🔄

**Goal:** Simplify the web UI from 6 pages to 3, with a command-center dashboard layout.
The file list should be the main content area that expands. Per-panel loading (no global loading gate).
Detail drawer with confidence-aware prefilling. TMDb/OMDb search in drawer. Mobile-responsive.

**Design Decisions:**
- 3 nav links: Dashboard, Files, Settings (removed Review, Ripper, Jobs)
- Dashboard = command center with sidebar (Drives, Review, Jobs panels) + main area (pipeline bar, file list)
- Files = focused data view with search/filter, DetailDrawer, quick accept, move preview
- DetailDrawer: confidence-aware prefilling (matched → pre-filled, needs review → suggested, failed → empty)
- Search: TMDb + OMDb, 300ms debounce in drawer
- Auto-scan: when rip starts, auto-scan the rip output directory (not the configured scan directory)
- Quick accept threshold: confidence >= 0.8 (revisit later)
- Move preview: moved to Files page (not Dashboard)
- No global loading gate — each panel loads independently with its own "Loading..." state
- Mobile-first considerations: CSS grid with responsive breakpoints, touch targets >= 44px, drawer full-screen on mobile

**Implementation Order:**
1. Backend: search endpoint → 2. Frontend: search API + types → 3. DetailDrawer component → 4. Layout shell → 5. Dashboard page → 6. Sidebar panels → 7. PipelineProgress component → 8. Files page → 9. Remove old pages → 10. Mobile CSS → 11. Test + lint

| # | Task | Status | Depends On |
|---|------|--------|------------|
| 44 | Backend: add `GET /api/search?q=...&type=movie|tv` endpoint | DONE | — |
| 45 | Frontend: add `search.ts` API client + update `types.ts` | DONE | 44 |
| 46 | Frontend: create `DetailDrawer.svelte` component | DONE | 45 |
| 47 | Frontend: update `+layout.svelte` with new nav (Dashboard, Files, Settings) and header buttons (Scan, Move) | DONE | — |
| 48 | Frontend: rewrite `+page.svelte` (Dashboard) with sidebar grid, per-panel loading, SSE, auto-scan on rip | DONE | 46, 47 |
| 49 | Frontend: create sidebar panel components (`DrivePanel.svelte`, `ReviewPanel.svelte`, `JobPanel.svelte`) | DONE | 48 |
| 50 | Frontend: create `PipelineProgress.svelte` component | DONE | 48 |
| 51 | Frontend: refactor `files/+page.svelte` with DetailDrawer, quick accept, search/filter, Move Preview button | DONE | 46, 48, 49, 50 |
| 52 | Remove old pages: `review/+page.svelte`, `ripper/+page.svelte`, `jobs/+page.svelte` | DONE | 48, 49, 51 |
| 53 | Mobile CSS: responsive styles for sidebar layout, file list cards, drawer, touch targets | DONE | 48, 49, 51 |
| 54 | Test + lint: `cargo fmt && cargo clippy --all && cargo build --all && cargo test --all` | DONE | All above |

### Batch 8: Post-Redesign Fixes + Reset Controls ✅

| # | Task | Status |
|---|------|--------|
| 55 | Fix: dashboard sidebar on LEFT, file list on right | DONE |
| 56 | Fix: Reject button fails (FK violation — NULL columns decoded as `""`) | DONE |
| 57 | Reset DB button on Files page (`DELETE /api/files`) | DONE |
| 58 | Reset to default settings button on Settings page (`POST /api/config/reset`) | DONE |

### Batch 9: Single Binary Deployment ✅

**Goal:** Make the deployable artifact one self-contained binary — the SvelteKit
frontend is embedded at compile time so the server no longer reads
`web/svelte/build` from disk at runtime.

**Decisions:** Always-embed (single code path, simpler mental model), accepted
tradeoff that every frontend change requires `npm run build` + Rust rebuild.

| # | Task | Status |
|---|------|--------|
| 59 | Embed frontend in binary (`rust-embed` + custom axum fallback handler) | DONE |
| 60 | `web/build.rs`: clear error if `web/svelte/build/index.html` missing; `rerun-if-changed` per file | DONE |
| 61 | Frontend serving tests (SPA root, SPA fallback route, immutable cache for `_app/*`) | DONE |
| 62 | Docs: README quick start/deploy + AGENTS.md note | DONE |
| 63 | Verify: full test suite + release build + single-file smoke test | DONE |
| 64 | `make` / `make install`: build release + symlink to `~/.local/bin/riply` | DONE |

### Batch 10: Web-UI Rip Progress + Rip-time Naming ✅

**Date:** 2026-08-13 — User pulled latest, ran `cargo run -p cli -- serve`, and saw
no rip progress bars and no way to name a rip.

**Root cause:** `web/svelte/build/` is gitignored and baked into the binary at
compile time, so after `git pull` the embedded frontend was stale (missing the
ADR-022 naming UI). Fix documented as gotcha 10 in AGENTS.md: always run
`npm run build` in `web/svelte` before `cargo run -p cli -- serve`.

**Changes (frontend only, `web/svelte/src/routes/+page.svelte`):**
- Rip progress bar inline in the Drives card: handles `rip:progress:<index>:<pct>[:<eta]>`
  and `rip:starting`/`rip:complete`/`rip:error` SSE events into a new
  `ripProgress` map; renders the striped `.progress__bar` with % + "~Xm Ys left"
  ETA; prunes stale entries on `fetchDrives`. Backend already emitted these events.
- Rip-time naming: the Rip button opens `openMediaNamingModal` pre-filled from the
  parsed disc name (`Title (YYYY)` → title + year) via the `suggest` callback
  (ADR-022 prefill-not-commit); the chosen title (+year) is used as the rip
  subdirectory name under the configured rip output dir. Cancelling aborts the rip.

**Pre-existing failures fixed along the way:**
- `web/svelte/src/lib/api/request.test.ts` still asserted `/api/...` paths after
  the `/v1/` migration (5 failing vitest tests) — updated to `/v1/`.
- Clippy `-D warnings` errors in `infra/src/makemkv/dryrun.rs` left by the last
  commit: moved `collect_events` into `mod tests` (dead-code +
  items-after-test-module) and `while let Some(_)` → `.is_some()`.

**Verify:** 220 Rust tests pass, clippy clean, frontend `npm test` 14/14,
`npm run build` succeeds.

### Batch 5: Architecture & Robustness Improvements 🔄

Source: `improvements.md`

| # | Task | Status | Plan File |
|---|------|--------|-----------|
| 13 | Quick win: Add `#[instrument]` to public functions in `app/src/*.rs` | DONE | `improvements.md` §2.1 |
| 14 | Quick win: Add `TraceLayer` to HTTP router in `web/src/api.rs` | DONE | `improvements.md` §2.1 |
| 15 | Quick win: Add retry logic to HTTP client (`backon` crate) | DONE | `improvements.md` §2.4 |
| 16 | Quick win: Add graceful shutdown to server in `cli/src/main.rs` | DONE | `improvements.md` §2.2 |
| 17 | Quick win: Add `--json` flags to `riply status` and `riply rip list` | DONE | `improvements.md` §3.3 |
| 18 | Structure: Split monolithic router into per-domain modules | DONE | `improvements.md` §1.2 |
| 19 | Structure: Add typed `WebError` type implementing `IntoResponse` | DONE | `improvements.md` §1.3 |
| 20 | Structure: Add versioned database migrations with `sqlx::migrate!()` | DONE | `improvements.md` §1.4, `infra/src/migrations/001-004` |
| 21 | Robustness: Add CHECK constraints and indexes to database | DONE | `improvements.md` §2.3, migration 003 |
| 22 | Robustness: Add `/ready` health/readiness endpoint | DONE | `improvements.md` §2.5, `web/src/api.rs` |
| 23 | Robustness: Add CORS and timeout middleware layers | DONE | `improvements.md` §2.6, `web/src/api.rs` (2026-08-02) |
| 24 | Serviceability: Add CLI progress bars (`indicatif` crate) | DONE | `improvements.md` §3.2 |
| 25 | Serviceability: healthchecks.io heartbeat integration | DONE | Background task pings hc-ping.com/<uuid>; config + serve.rs + AppState handle |
| 26 | Serviceability: Add config validation with `config` crate | DONE | `improvements.md` §3.6, `infra/src/config.rs` |
| 27 | Feature: Library scanning (`riply library scan` command) | DONE | `improvements.md` §4.2 |
| 28 | Feature: Watch directory / filesystem events (`notify` crate) | DONE | `improvements.md` §4.4 |
| 29 | Feature: Metadata enrichment (genres, runtime, cast, content rating) | DONE | `improvements.md` §4.3, migration 004 (2026-08-02) |
| 30 | Feature: Backup and restore commands | DONE | `improvements.md` §4.5 (2026-08-02) |
| 31 | Consider: Merge `web` crate into `cli` | DECIDED AGAINST | `improvements.md` §1.5 — closed 2026-08-02: no user value, pure refactor; keep `web` separate for a future second binary |
| 32 | Consider: API versioning (`/api/v1` prefix) | DONE | `improvements.md` §4.6, ADR-017 (`/v1/` prefix) |

---

## Active Tasks

*None — the roadmap is fully closed out. All items are DONE or DECIDED AGAINST.

## Completed Tasks

| # | Task | Plan File |
|---|------|-----------|
| 1 | Fix hardcoded scan directory in layout | `fix-hardcoded-dir.md` |
| 2 | Fix stale TypeScript types | `fix-ts-types.md` |
| 3 | Fix SQL migration casing + filter value | `fix-migration-casing.md` |
| 4 | Cross-device move fallback (copy+delete) | `cross-device-move.md` |
| 5 | Regex lazy_static in Analyzer | `regex-lazy-static.md` |
| 6 | SQL pagination (LIMIT/OFFSET) | `sql-pagination.md` |
| 7 | Fix cancel_rip CLI no-op | `fix-cancel-rip.md` |
| 8 | Review page: add metadata editing before accept | `review-editing.md` |
| 9 | Move preview in web UI | `move-preview.md` |
| 10 | File text search + batch retry failed | `file-search-retry.md` |
| 11 | ffprobe metadata extraction | `ffprobe-metadata.md` |
| 12 | Duplicate detection (hash-based) | `duplicate-detection.md` |
| 18 | Split monolithic router into per-domain modules | `improvements.md` §1.2 |
| 20 | Versioned DB migrations (`sqlx::migrate!()`) | `improvements.md` §1.4, `infra/src/migrations/001-004` |
| 21 | CHECK constraints + indexes | `improvements.md` §2.3, migration 003 |
| 22 | `/ready` health/readiness endpoint | `improvements.md` §2.5, `web/src/api.rs` |
| 23 | CORS + request timeout middleware | `improvements.md` §2.6, `web/src/api.rs` (2026-08-02) |
| 24 | CLI progress bars (`indicatif`) | `improvements.md` §3.2 |
| 26 | Config validation | `improvements.md` §3.6, `infra/src/config.rs` |
| 27 | Library scanning (`riply library scan`) | `improvements.md` §4.2 |
| 28 | Watch directory (`notify` crate) | `improvements.md` §4.4 |
| 29 | Metadata enrichment (genres, runtime, cast, rating) | `improvements.md` §4.3, migration 004 (2026-08-02) |
| 30 | Backup and restore commands | `improvements.md` §4.5 (2026-08-02) |
| 32 | API versioning (`/v1/` prefix, ADR-017) | `improvements.md` §4.6 |
| 31 | ~~Merge `web` crate into `cli`~~ — **decided against** 2026-08-02 (no user value; keep `web` separate) | `improvements.md` §1.5 |
| 13–17, 19 | Architecture & robustness quick wins (instrument spans, TraceLayer, retry, graceful shutdown, `--json`, WebError) | `improvements.md` §2–3 |
| 25 | healthchecks.io heartbeat integration | `improvements.md` §2.5, `infra/src/healthchecks.rs` |
| 33–43 | Ripping overhaul (11 tasks) | `makemkv-implementation.md`, `makemkv-setup.md` |
| Legacy | Wire pipeline UI, MakeMKV abstraction, SvelteKit migration | — |

## How to Use

1. Read the **Next Up** callout — it tells you what to work on
2. If the Next Up task has a plan file, open it for implementation details
3. Update status to `IN_PROGRESS` when you start
4. Update status to `DONE` when complete, add a completion note
5. Move the next pending task into the **Next Up** callout
