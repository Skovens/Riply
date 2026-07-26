# Plan: Wire Scan/Analyze/Move Pipeline to Web UI

## Overview

The web dashboard has header buttons (Scan, Analyze, Move) that are dead. The
`JobScheduler` is built but not wired. Goal: make the web UI fully functional
for the core pipeline.

---

## Phase 1: Backend — Add JobScheduler to AppState

**File: `web/src/api.rs`**

1. Add `scheduler: Arc<JobScheduler>` and `db: Arc<Database>` to `AppState`
2. Add `frontend: Option<Arc<Frontend>>` (already exists)
3. Update `create_router` to accept `AppState` directly (already does)

---

## Phase 2: Backend — Pipeline API Endpoints

**File: `web/src/api.rs`**

### `POST /api/scan`
- Body: `{ "directory": "/path/to/scan" }`
- Validates directory exists
- Calls `JobScheduler::start_scan_job` with a closure that:
  - Uses `ScannerService::scan_directory` to scan
  - Saves each record to DB via `Database::save_file`
  - Reports progress via `ProgressHandle`
  - Checks `CancellationToken` periodically
- Returns `{ "job_id": "job_1", "status": "Queued" }`

### `POST /api/analyze`
- No body needed (analyzes all `Scanned` status files from DB)
- Calls `JobScheduler::start_analyze_job` with a closure that:
  - Loads files with status "Scanned" from DB
  - For each: runs `AnalyzerService::analyze_file`
  - Saves updated records to DB
  - Reports progress
- Returns `{ "job_id": "job_2", "status": "Queued" }`

### `POST /api/move`
- Body: `{ "movies_dir": "/path", "tv_dir": "/path" }` (optional, falls back to config)
- Calls `JobScheduler::start_move_job` with a closure that:
  - Loads files with status "Confirmed" from DB
  - For each: runs `MoverService::move_file`
  - Saves updated records to DB
  - Reports progress
- Returns `{ "job_id": "job_3", "status": "Queued" }`

### `GET /api/jobs`
- Returns `Vec<JobInfo>` from scheduler

### `GET /api/jobs/:id`
- Returns `JobInfo` for specific job

### `POST /api/jobs/:id/cancel`
- Calls `JobScheduler::cancel_job`
- Returns `{ "ok": true }`

---

## Phase 3: Frontend — API Client Methods

**File: `web/static/js/api.js`**

Add methods:
- `scanDirectory(directory)` — POST /api/scan
- `analyzeFiles()` — POST /api/analyze
- `moveFiles(moviesDir, tvDir)` — POST /api/move
- `getJobs()` — GET /api/jobs
- `getJob(id)` — GET /api/jobs/:id
- `cancelJob(id)` — POST /api/jobs/:id/cancel

---

## Phase 4: Frontend — Jobs Component

**File: `web/static/js/components/jobs.js`**

New component showing:
- Active jobs with progress bars
- Completed/failed/cancelled jobs
- Cancel button for running jobs
- Auto-refreshes via SSE or polling

---

## Phase 5: Frontend — Wire Header Buttons

**File: `web/static/js/app.js`**

- Scan button → prompts for directory (default: config `scan_directory`)
- Analyze button → calls analyze API
- Move button → confirms, then calls move API
- All show toast notifications on success/failure
- Navigate to Jobs view after triggering

---

## Phase 6: Frontend — Navigation

**File: `web/static/js/app.js`**

Hash-based routing:
- `#/` → Dashboard
- `#/files` → Files list
- `#/review` → Review queue
- `#/ripper` → Drive/rip control
- `#/jobs` → Job list

Add nav links to sidebar/header.

---

## Phase 7: Fix Ancillary Issues

1. **Config persistence** — `PUT /api/config` should write to disk via `Config::save`
2. **`riply status`** — Query actual backends instead of hardcoded zeros
3. **SSE events** — Broadcast job started/completed/failed events

---

## Files to Modify

| File | Changes |
|------|---------|
| `web/src/api.rs` | Add scheduler+db to AppState, 6 new endpoints |
| `web/static/js/api.js` | 6 new API methods |
| `web/static/js/app.js` | Button handlers, navigation, routing |
| `web/static/js/components/jobs.js` | New jobs list component |
| `web/static/index.html` | Jobs nav link, jobs view container |
| `web/static/css/components.css` | Jobs component styles |

## Files to Read (reference)

| File | Why |
|------|-----|
| `app/src/jobs.rs` | JobScheduler API |
| `app/src/scanner.rs` | ScannerService::scan_directory |
| `app/src/analyzer.rs` | AnalyzerService::analyze_file |
| `app/src/mover.rs` | MoverService::move_file |
| `infra/src/database.rs` | Database methods |
| `infra/src/config.rs` | Config fields |

---

## Verification

1. `cargo fmt && cargo clippy --all && cargo build --all && cargo test --all`
2. Manual test: start server, click Scan/Analyze/Move buttons, verify jobs appear
3. Test cancellation via UI
4. Verify SSE events fire for job state changes
