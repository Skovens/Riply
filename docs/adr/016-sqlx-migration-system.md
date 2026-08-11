# ADR-016: SQLx Migration System

## Status

Accepted

## Context

ADR-002's negative consequences noted migrations were `include_str!`-based
and should move to SQLx's built-in system. The database is ephemeral
(AGENTS.md), so schema evolution is cheap and frequent.

## Decision

Use `sqlx::migrate!()` with numbered `.sql` files in
`infra/src/migrations/`:

- `001_create_files_table.sql`
- `002_create_title_groups.sql`
- `003_add_constraints.sql`
- `004_add_metadata_enrichment.sql`

`Database::migrate()` runs `sqlx::migrate!("src/migrations").run(&pool)`
on startup. SQLx records applied versions in the `_sqlx_migrations`
table and embeds the migrations at compile time.

## Consequences

- Schema is versioned, ordered, and applied exactly once per DB.
- Migrations are embedded in the binary - no runtime file reads.
- New schema changes always add a numbered file; never edit an old one.
- Since the DB is ephemeral, migration history can be reset freely.
