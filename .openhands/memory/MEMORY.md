# Riply — Session Memory Index

## How to resume work here
1. Read `AGENTS.md` (session-start ritual is documented there).
2. Read `.opencode/plans/todo.md` "Next Up" callout — canonical task board.
3. Read this file + the newest daily log (`.openhands/memory/YYYY-MM-DD.md`).
4. Check `git --no-pager log --oneline -15` — the source of truth for what's done.

## Roadmap status (as of 2026-08-02)
All `improvements.md` items are DONE. Only remaining: #31 "Merge web crate into cli" (consider item, low priority).
Task board tracks every item with commit references.

## Durable facts
- Session log for each work session: `.openhands/memory/2026-08-02.md`.
- SQLite: versioned migrations in `infra/src/migrations/` (001–004, `sqlx::migrate!()`); DB is ephemeral but schema changes go through new numbered migrations.
- `files.cast_list` column is named that way because `CAST` is a SQLite keyword; genres/cast are JSON-encoded TEXT via helpers in `infra/src/db.rs`.
- Web router: 30s `TimeoutLayer` applies to the API sub-router ONLY — never over `/v1/sse` (long-lived stream) or the static fallback.
- `riply backup` / `riply restore` exist (SQLite ATTACH-based; restore uses a dedicated PoolConnection for ATTACH/DETACH).
- Enrichment data (genres, runtime, content_rating, overview, cast_list) fetched via TMDb `/movie|/tv/{id}` with `append_to_response=credits,release_dates`; cast truncated to top 5; analyzer degrades gracefully if enrichment fails.
