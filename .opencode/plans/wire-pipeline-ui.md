# Plan: Wire Scan/Analyze/Move Pipeline to Web UI

## Status: COMPLETE

All phases implemented and verified. The web UI is fully functional for the
core pipeline. See commit history for details.

---

## What Was Built

### Backend
- `JobScheduler` wired into `AppState` for scan/analyze/move/rip jobs
- `POST /api/scan`, `POST /api/analyze`, `POST /api/move` — background jobs
- `GET /api/jobs`, `GET /api/jobs/:id`, `POST /api/jobs/:id/cancel` — job management
- `POST /api/review/:id/accept`, `POST /api/review/:id/reject` — pipeline state enforcement
- Config persistence to disk via `PUT /api/config`
- `GET /api/drives` — returns error status instead of swallowing
- `GET /api/move/status` — confirmed/moved counts for dashboard
- SSE events standardized: `event:status`, `event:progress`, `event:log`

### Frontend
- Jobs component with progress bars, cancel buttons, auto-refresh
- Files component with pagination and status filter
- Review component with accept/reject queue
- Ripper component with drive list and rip/cancel
- Dashboard with 5 stat cards (Scanned, Matched, Review, Confirmed, Moved)
- Header buttons wired to trigger pipeline actions
- Hash-based routing: `#/`, `#/files`, `#/review`, `#/ripper`, `#/jobs`
- Shared `utils.js` with `escapeHtml`, `formatSize`, `statusColor`

### Correctness Fixes
- `start_move` converted to background job (was blocking HTTP)
- `start_rip` accepts destination from request body
- Division-by-zero in rip progress fixed
- Rescan preserves review decisions (INSERT OR IGNORE + UPDATE)
- `hash_file` uses `spawn_blocking` (was blocking tokio runtime)
- CLI `riply status` uses single GROUP BY query (was 12 redundant queries)
- API keys configurable via `PUT /api/config`
- OMDb hardcoded `type=movie` removed

---

## Files Modified

| File | Changes |
|------|---------|
| `web/src/api.rs` | All API endpoints, AppState, static file embedding |
| `web/src/sse.rs` | Standardized SSE event format |
| `web/src/lib.rs` | Public exports |
| `web/static/js/app.js` | Routing, header buttons, toast notifications |
| `web/static/js/api.js` | API client methods |
| `web/static/js/store.js` | State management |
| `web/static/js/utils.js` | Shared helpers (NEW) |
| `web/static/js/components/dashboard.js` | Dashboard with 5 stat cards |
| `web/static/js/components/files.js` | Paginated file list |
| `web/static/js/components/review.js` | Review queue |
| `web/static/js/components/jobs.js` | Job list with progress |
| `web/static/js/components/ripper.js` | Drive list, rip/cancel |
| `web/static/css/layout.css` | Nav link styles, grid--5 |
| `web/static/css/components.css` | Progress, job, drive styles |
| `app/src/scanner.rs` | spawn_blocking for hash_file |
| `app/src/jobs.rs` | JobScheduler (already existed) |
| `cli/src/commands/status.rs` | GROUP BY query optimization |
| `cli/src/main.rs` | Pass config_path to serve |
| `infra/src/db.rs` | count_files_by_status_all, save_file preserves reviews |
| `infra/src/http.rs` | OMDb type param removed |
