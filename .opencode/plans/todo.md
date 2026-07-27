# Riply — Agent Task Board

All 12 tasks completed. See git log for commit history.

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

### Legacy (folded into above)

| Task | Status | Folded Into |
|------|--------|-------------|
| Remove dead code | FOLDED | #2 (types), #3 (migration), #12 (duplicate_path) |
| TV metadata enrichment | FOLDED | #11 (ffprobe), #8 (review editing) |
| Wire pipeline UI | DONE | — |
| MakeMKV abstraction | DONE | — |
| Migrate to SvelteKit | DONE | — |

### Batch 5: Architecture & Robustness Improvements 🔄

Source: `improvements.md`

| # | Task | Status | Plan File |
|---|------|--------|-----------|
| 13 | Quick win: Add `#[instrument]` to public functions in `app/src/*.rs` | PENDING | `improvements.md` §2.1 |
| 14 | Quick win: Add `TraceLayer` to HTTP router in `web/src/api.rs` | PENDING | `improvements.md` §2.1 |
| 15 | Quick win: Add retry logic to HTTP client (`backon` crate) | PENDING | `improvements.md` §2.4 |
| 16 | Quick win: Add graceful shutdown to server in `cli/src/main.rs` | PENDING | `improvements.md` §2.2 |
| 17 | Quick win: Add `--json` flags to `riply status` and `riply rip list` | PENDING | `improvements.md` §3.3 |
| 18 | Structure: Split monolithic router into per-domain modules (`files.rs`, `review.rs`, `scan.rs`, `analyze.rs`, `mover.rs`, `jobs.rs`, `ripper.rs`, `config.rs`) | PENDING | `improvements.md` §1.2 |
| 19 | Structure: Add typed `WebError` type implementing `IntoResponse` | PENDING | `improvements.md` §1.3 |
| 20 | Structure: Add versioned database migrations with `sqlx::migrate!()` | PENDING | `improvements.md` §1.4 |
| 21 | Robustness: Add CHECK constraints and indexes to database | PENDING | `improvements.md` §2.3 |
| 22 | Robustness: Add `/ready` health/readiness endpoint | PENDING | `improvements.md` §2.5 |
| 23 | Robustness: Add CORS and timeout middleware layers | PENDING | `improvements.md` §2.6 |
| 24 | Serviceability: Add CLI progress bars (`indicatif` crate) | PENDING | `improvements.md` §3.2 |
| 25 | Serviceability: Add Prometheus metrics endpoint (`metrics` + `metrics-exporter-prometheus`) | PENDING | `improvements.md` §3.4 |
| 26 | Serviceability: Add config validation with `config` crate (env var support) | PENDING | `improvements.md` §3.6 |
| 27 | Feature: Library scanning (`riply library scan` command) | PENDING | `improvements.md` §4.2 |
| 28 | Feature: Watch directory / filesystem events (`notify` crate) | PENDING | `improvements.md` §4.4 |
| 29 | Feature: Metadata enrichment (genres, runtime, cast, content rating) | PENDING | `improvements.md` §4.3 |
| 30 | Feature: Backup and restore commands (`riply backup` / `riply restore`) | PENDING | `improvements.md` §4.5 |
| 31 | Consider: Merge `web` crate into `cli` (reduce workspace complexity) | PENDING | `improvements.md` §1.5 |
| 32 | Consider: API versioning (`/api/v1` prefix) | PENDING | `improvements.md` §4.6 |
