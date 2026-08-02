# Riply — Agent Task Board

## Next Up

**#13** — Add `#[instrument]` to public functions in `app/src/*.rs`
- **Priority:** Quick win (Phase 1)
- **Depends on:** nothing
- **Source:** `improvements.md` §2.1
- **Plan file:** N/A — follow `#[instrument]` pattern from tracing crate docs

---

## Active Tasks

| # | Task | Status | Depends On | Plan File |
|---|------|--------|------------|-----------|
| 13 | Add `#[instrument]` to public functions in `app/src/*.rs` | PENDING | — | — |
| 14 | Add `TraceLayer` to HTTP router in `web/src/api.rs` | PENDING | — | — |
| 15 | Add retry logic to HTTP client (`backon` crate) | PENDING | — | — |
| 16 | Add graceful shutdown to server in `cli/src/main.rs` | PENDING | — | — |
| 17 | Add `--json` flags to `riply status` and `riply rip list` | PENDING | — | — |
| 19 | Add typed `WebError` type implementing `IntoResponse` | PENDING | — | — |
| 20 | Add versioned database migrations with `sqlx::migrate!()` | PENDING | — | — |
| 21 | Add CHECK constraints and indexes to database | PENDING | — | — |
| 22 | Add `/ready` health/readiness endpoint | PENDING | — | — |
| 23 | Add CORS and timeout middleware layers | PENDING | — | — |
| 24 | Add CLI progress bars (`indicatif` crate) | PENDING | — | — |
| 25 | healthchecks.io heartbeat integration | PENDING | — | — |
| 26 | Add config validation with `config` crate | PENDING | — | — |
| 27 | Library scanning (`riply library scan` command) | PENDING | — | — |
| 28 | Watch directory / filesystem events (`notify` crate) | PENDING | — | — |
| 29 | Metadata enrichment (genres, runtime, cast, rating) | PENDING | — | — |
| 30 | Backup and restore commands | PENDING | — | — |
| 31 | Consider: Merge `web` crate into `cli` | PENDING | — | — |
| 32 | Consider: API versioning (`/api/v1` prefix) | PENDING | — | — |

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
| 33–43 | Ripping overhaul (11 tasks) | `makemkv-implementation.md`, `makemkv-setup.md` |
| Legacy | Wire pipeline UI, MakeMKV abstraction, SvelteKit migration | — |

## How to Use

1. Read the **Next Up** callout — it tells you what to work on
2. If the Next Up task has a plan file, open it for implementation details
3. Update status to `IN_PROGRESS` when you start
4. Update status to `DONE` when complete, add a completion note
5. Move the next pending task into the **Next Up** callout
