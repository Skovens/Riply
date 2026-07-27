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
