---
name: sqlx-database
description: >
  Database management for Riply using SQLx.
  Covers migration strategies, type-safe queries, connection pooling,
  WAL mode configuration, and schema versioning.
metadata:
  language: rust
  version: "0.7+"
  compatibility: sqlx, sqlite, postgres
---

# SQLx Database Standards

## General Principles
- **Type Safety:** Always use `sqlx::query!` and `sqlx::query_as!` for compile-time checked SQL.
- **Migrations:** Use `sqlx migrate` for version-controlled schema changes.
- **WAL Mode:** Enable Write-Ahead Logging for SQLite to allow concurrent reads.

## Migration Strategy
- Store migrations in `migrations/` directory.
- Each migration is a numbered SQL file (e.g., `001_create_files_table.sql`).
- Use `sqlx migrate run` to apply pending migrations.
- Use `sqlx migrate revert` to rollback migrations.
- Track schema version in `schema_version` table.

## Type-Safe Queries
- **Compile-Time Checked:**
  ```rust
  let record = sqlx::query_as!(
      FileRecord,
      "SELECT * FROM files WHERE id = ?",
      file_id
  )
  .fetch_one(&pool)
  .await?;
  ```
- **Avoid Raw SQL:** Never use `query()` with string interpolation.
- **Parameterized Queries:** Always use `?` placeholders for user input.

## Connection Pooling
- Use `sqlx::Pool` for efficient connection management.
- Configure pool size based on expected concurrency:
  - Minimum: 2 connections.
  - Maximum: 10 connections (adjust based on workload).
- Enable connection timeout and idle timeout.

## SQLite Configuration
- **WAL Mode:**
  ```sql
  PRAGMA journal_mode=WAL;
  PRAGMA synchronous=NORMAL;
  PRAGMA busy_timeout=5000;
  ```
- **File Locking:** SQLite handles file locking automatically in WAL mode.
- **Backup:** Use `sqlx::query!("VACUUM INTO ?", backup_path)` for backups.

## Schema Design
- **Files Table:**
  ```sql
  CREATE TABLE files (
      id TEXT PRIMARY KEY,
      path TEXT NOT NULL,
      filename TEXT NOT NULL,
      size INTEGER,
      duration INTEGER,
      hash TEXT,
      status TEXT NOT NULL DEFAULT 'SCANNED',
      suggested_title TEXT,
      suggested_year INTEGER,
      confidence REAL,
      canonical_title TEXT,
      canonical_year INTEGER,
      tmdb_id INTEGER,
      content_type TEXT DEFAULT 'unknown',
      media_type TEXT DEFAULT 'movie',
      series_name TEXT,
      season_number INTEGER,
      episode_number INTEGER,
      episode_title TEXT,
      source TEXT,
      subtitle_text TEXT,
      duplicate_path TEXT,
      reviewed INTEGER DEFAULT 0,
      created_at TEXT DEFAULT CURRENT_TIMESTAMP,
      updated_at TEXT DEFAULT CURRENT_TIMESTAMP
  );
  ```
- **Indexes:**
  - `idx_files_status` on `status` column.
  - `idx_files_path` on `path` column (unique).
  - `idx_files_series_season` on `(series_name, season_number)`.

## Error Handling
- Handle `sqlx::Error::Database` for database-specific errors.
- Handle `sqlx::Error::RowNotFound` for missing records.
- Log SQL errors with context (query, parameters).
- Retry transient errors (e.g., database locked).

## Testing
- Use in-memory SQLite for unit tests.
- Use `sqlx::test` attribute for integration tests.
- Mock database connections where appropriate.
- Test migration scripts independently.