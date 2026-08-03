# Riply Priority Roadmap

Recommendations for improving Riply, ordered by impact vs effort.

See `docs/improvements-details.md` for full analysis, implementation notes, and resources.

---

## Phase 1 — Quick Wins

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 1 | Add `--json` flags to CLI output commands | Low | Low |
| 2 | Add retry logic to HTTP client (`backon`) | Low | Medium |
| 3 | Add graceful shutdown to server | Low | Medium |
| 4 | Add `TraceLayer` to HTTP router | Low | Medium |

## Phase 2 — Structure Improvements

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 5 | Split router into domain modules | Medium | High |
| 6 | Add typed error type (`WebError`) | Medium | High |
| 7 | Add database constraints and indexes | Low | Medium |
| 8 | Add `#[instrument]` to key functions | Low | High |

## Phase 3 — Observability

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 9 | Add health/readiness endpoints | Low | Medium |
| 10 | Add metrics endpoint (Prometheus) | Medium | Medium |
| 11 | Add config validation (`config` crate) | Low | Medium |

## Phase 4 — Features

| # | Task | Effort | Impact |
|---|------|--------|--------|
| 12 | Duplicate detection | Low | Medium |
| 13 | Library scanning | Medium | Medium |
| 14 | Watch directory / filesystem events | Medium | Medium |
| 15 | CLI progress bars | Medium | Medium |
