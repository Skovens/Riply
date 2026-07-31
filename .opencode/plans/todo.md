# Riply — Agent Task Board

## Completed Tasks

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
| 20 | Structure: Add versioned database migrations with `sqlx::migrate!()` | PENDING | `improvements.md` §1.4 |
| 21 | Robustness: Add CHECK constraints and indexes to database | PENDING | `improvements.md` §2.3 |
| 22 | Robustness: Add `/ready` health/readiness endpoint | PENDING | `improvements.md` §2.5 |
| 23 | Robustness: Add CORS and timeout middleware layers | PENDING | `improvements.md` §2.6 |
| 24 | Serviceability: Add CLI progress bars (`indicatif` crate) | PENDING | `improvements.md` §3.2 |
| 25 | Serviceability: healthchecks.io heartbeat integration | PENDING | Simple background task pings hc-ping.com/<uuid> on interval |
| 26 | Serviceability: Add config validation with `config` crate | PENDING | `improvements.md` §3.6 |
| 27 | Feature: Library scanning (`riply library scan` command) | PENDING | `improvements.md` §4.2 |
| 28 | Feature: Watch directory / filesystem events (`notify` crate) | PENDING | `improvements.md` §4.4 |
| 29 | Feature: Metadata enrichment (genres, runtime, cast, content rating) | PENDING | `improvements.md` §4.3 |
| 30 | Feature: Backup and restore commands | PENDING | `improvements.md` §4.5 |
| 31 | Consider: Merge `web` crate into `cli` | PENDING | `improvements.md` §1.5 |
| 32 | Consider: API versioning (`/api/v1` prefix) | PENDING | `improvements.md` §4.6 |

---

## Batch 6 Details

### #33 — Extract eject to `infra` + fix device path mapping

**Problem:** `eject_disc()` lives in `cli/src/commands/rip.rs:118-152` and assumes
`/dev/sr{index}` which is wrong — MakeMKV's drive index 0 can map to `/dev/sr1`.
The parser discards the 7th DRV field (device path).

**Changes:**
- `infra/src/makemkv/parser.rs` — extract 7th field into `ParsedDrive.device_path`
- `core/src/ripping.rs` — add `device_path: Option<String>` to `DriveInfo`
- `infra/src/makemkv/backend.rs` — populate `device_path` in `detect_drives()`
- `infra/src/makemkv/mod.rs` — new `pub fn eject_disc(device_path: &str) -> Result<()>`
- `cli/src/commands/rip.rs` — call `infra::eject_disc()` with the correct device path
- `web/src/routes/ripper.rs` — expose eject via `POST /api/ripper/:index/eject`

### #34 — Config validation: refuse to rip if directories not set

**Changes:**
- `infra/src/config.rs` — add `Config::validate_for_ripping() -> Result<(), String>` checking
  `rip_output_directory` is `Some` and the path is writable
- `web/src/routes/ripper.rs:76-86` — call validation before starting rip, return 400 with
  descriptive message if it fails
- `cli/src/commands/rip.rs:29-31` — same validation, print user-friendly message
- `web/svelte/src/routes/ripper/+page.svelte` — optional warning banner when
  `rip_output_directory` is empty

### #35 — Disc name parsing: extract title + year

**Problem:** MakeMKV disc names follow patterns like `INCEPTION_2010`, `THE_DARK_KNIGHT`,
`BRAVEHEART`. We need a utility to parse these into `(title, Option<year>)`.

**Changes:**
- `infra/src/makemkv/parser.rs` — new `pub fn parse_disc_name(name: &str) -> (String, Option<u16>)`
  that: replaces underscores with spaces, checks for trailing 4-digit year, title-cases the result
- Tests for edge cases: all-caps, with/without year, multi-word, special chars

### #36 — Auto-create parent directory from disc name

**Depends on:** #35

**Changes:**
- `web/src/routes/ripper.rs:start_rip` — after resolving destination, look up disc name from
  `detect_drives()` output, parse it, append `/{title}/` to dest path
- `infra/src/makemkv/backend.rs:start_rip` — already calls `create_dir_all`, no change needed
- `cli/src/commands/rip.rs` — same logic for CLI path

### #37 — Rip progress with time-remaining estimate

**Problem:** `PRGV:current,total,max` provides percentage but no time data. Early progress is
unstable (disc spin-up), so ETA should only show after ~15 seconds.

**Changes:**
- `web/src/routes/ripper.rs:start_rip` — track `Option<Instant>` of first meaningful progress
  (>0%). After 15s, compute `elapsed / pct * (100 - pct)` for ETA. Send via SSE:
  `rip:progress:{index}:{pct}:{eta_secs}` (eta_secs = 0 until stable)
- `web/svelte/src/routes/ripper/+page.svelte` — parse ETA from SSE, display as
  "Remaining: ~Xm Ys" next to progress bar. Hide until ETA > 0.

### #38 — Auto-rip: background polling detects disc insertion

**Depends on:** #33, #34, #36

**Changes:**
- `infra/src/config.rs` — add to `RippingConfig`:
  - `autorip_enabled: bool` (default `true`, `#[serde(default)]`)
  - `autorip_eject_after: bool` (default `true`)
- `web/src/api.rs` — add `autorip_handle: RwLock<Option<JoinHandle<()>>>` to `AppState`
- `web/src/routes/ripper.rs` — new `start_autorip()` / `stop_autorip()` functions that spawn
  a `tokio::time::interval(5s)` loop:
  1. Call `detect_drives()` (via `spawn_blocking`)
  2. Compare against previous state snapshot
  3. On `Empty → Ready` transition for an autorip-enabled drive:
     - Check `active_rips` to avoid double-ripping
     - Call `start_rip` with the drive index
     - On completion, call `eject_disc` if `autorip_eject_after` is true
  4. Send SSE events: `autorip:started:{index}`, `autorip:ejected:{index}`
- `web/src/routes/ripper.rs` — new endpoints:
  - `POST /api/ripper/autorip/toggle` — enable/disable
  - `GET /api/ripper/autorip` — current status
- `cli/src/commands/serve.rs` — start autorip task at server startup if `autorip_enabled`
- `web/svelte/src/routes/ripper/+page.svelte` — autorip toggle button, show autorip events

### #39 — Per-drive autorip toggle

**Depends on:** #38

**Changes:**
- `infra/src/config.rs` — add to `RippingConfig`:
  - `autorip_excluded_drives: Option<Vec<usize>>` (default `None` = all drives)
- `web/src/routes/config.rs` — support `autorip_excluded_drives` in update body
- `web/svelte/src/routes/ripper/+page.svelte` — when autorip is enabled, show a toggle
  per drive card. Toggling adds/removes the drive index from `autorip_excluded_drives`
  via `PUT /api/config`.
- Autorip polling task checks the excluded list before starting a rip

### #40 — Fix time estimation display

**Problem:** ETA from SSE `rip:progress:{index}:{pct}:{eta_secs}` is not rendering in the UI.
The frontend may not be parsing the SSE event correctly, or the ETA value is 0/never stabilizing.

**Investigation:**
- Check if SSE event `rip:progress` is properly received in `+page.svelte`
- Verify ETA calculation in `start_rip` — is 15s stabilization too long? Is first progress >0% being hit?
- Check if `eta_secs` is being sent as a number or string
- Add console logging to debug SSE parsing

**Fix:**
- Ensure SSE event format matches frontend parser
- If ETA never stabilizes, debug the progress data from MakeMKV
- Display ETA next to progress bar when > 0

### #41 — Per-drive autorip toggle UI

**Problem:** Autorip is a global on/off toggle. Users want to exclude specific drives from autorip.

**Changes:**
- `web/svelte/src/routes/ripper/+page.svelte` — on each drive card, show an autorip toggle icon/button
- Clicking toggle calls `PUT /api/config` to update `autorip_excluded_drives` array
- Drive card shows visual indicator: autorip enabled (green) vs excluded (gray)
- When global autorip is disabled, per-drive toggles are hidden/disabled

### #42 — Progress bar with percentage

**Problem:** Rip progress is sent via SSE but no visual progress bar with percentage exists.

**Changes:**
- `web/svelte/src/routes/ripper/+page.svelte` — add progress bar component per active rip
- Parse `rip:progress:{index}:{pct}:{eta_secs}` SSE events
- Display: `<progress value={pct} max="100"> {pct}%</progress>` or custom styled bar
- Show percentage text next to bar
- Auto-ripple to 100% when rip completes

### #43 — Optimize dashboard loading during active rip

**Problem:** Dashboard takes a long time to load when a rip is in progress. Something is blocking.

**Likely causes:**
- SSE connection from ripper page is blocking other page loads
- `/api/status` endpoint may be waiting on rip progress data
- `detect_drives()` or other blocking calls in status endpoint

**Investigation:**
- Check if `/api/status` endpoint uses `spawn_blocking` for any blocking calls
- Check if SSE connections are held open and blocking resources
- Check if rip progress polling is using too frequent intervals
- Profile server response times during active rip

**Fix:**
- Ensure all blocking calls use `spawn_blocking`
- Add timeouts to SSE connections
- Consider debouncing or throttling status updates during active rip
