# ADR-002: Database Technology Choice

## Status

Accepted

## Context

Riply needs persistent storage for media file records, job state, configuration, and ripping metadata. The application is designed to be:
- Portable (no separate database server required)
- Lightweight (single binary distribution)
- Reliable (data integrity for media library state)
- Simple to configure (end users should not need to install PostgreSQL or MySQL)

## Decision

Use **SQLite** as the embedded database, accessed via the **SQLx** crate with compile-time checked queries.

Key choices:
- SQLite with WAL (Write-Ahead Logging) journal mode for better concurrency
- Database auto-creation via `SqlitePoolOptions` with `create_if_missing(true)`
- Configurable database path via `config.toml` (defaults to `riply.db`)
- SQLx for type-safe query execution at compile time

## Consequences

### Positive
- Zero configuration — no database server to install or maintain
- Single file storage simplifies backups and portability
- WAL mode provides good read concurrency without locking
- SQLx catches SQL errors at compile time
- SQLite is battle-tested and widely understood

### Negative
- Limited concurrent write throughput under heavy load (unlikely to be a problem for Riply's scale)
- No remote database access (by design — Riply is single-machine)
- Migration strategy needs to be managed carefully (currently `include_str!`, should migrate to SQLx's built-in migration system per improvements.md)

### Future Considerations
If Riply eventually needs multi-machine support or higher write throughput, the `infra` crate's `Database` trait can be swapped for a PostgreSQL implementation without changing `app` or `core` layers.
