# Design: Database Layer

## Purpose
SQLite-based persistence for file records, title groups, and configuration. Uses `sqlx` for type-safe queries and `sqlx::migrate!()` for versioned migrations. This document describes the *current* behavior of the database layer.

---

## Database Connection

### `Database::new(path: &str) -> Result<Self, anyhow::Error>`

**Behavior:**
1. Creates SQLite connection URL: `sqlite:{path}`
2. Creates connection pool:
   - Max connections: 5
   - WAL journal mode (enables concurrent reads)
   - Auto-create if missing (`create_if_missing(true)`)
3. Verifies connection with `SELECT 1`
4. Returns `Database { pool, db_path }`

### `Database::migrate() -> Result<(), anyhow::Error>`

**Behavior:**
1. Runs `sqlx::migrate!("src/migrations")` against the pool
2. Applies all pending migrations in order
3. Migrations are in `infra/src/migrations/`

### `Database::ping() -> Result<(), anyhow::Error>`

Health check: executes `SELECT 1`.

---

## Schema

### `files` Table

```sql
CREATE TABLE files (
    id TEXT PRIMARY KEY,              -- First 16 hex chars of SHA-256 hash
    path TEXT NOT NULL,               -- Full path on disk
    filename TEXT NOT NULL,           -- File name only
    size INTEGER,                     -- File size in bytes
    duration INTEGER,                 -- Duration in seconds (ffprobe)
    hash TEXT NOT NULL,               -- Full SHA-256 hex digest
    status TEXT NOT NULL DEFAULT 'Scanned',
    suggested_title TEXT,             -- Title from analysis
    suggested_year INTEGER,
    confidence REAL,                  -- 0.0 to 1.0
    canonical_title TEXT,             -- User-confirmed title
    canonical_year INTEGER,
    tmdb_id INTEGER,                  -- TMDb ID
    content_type TEXT NOT NULL DEFAULT 'unknown',  -- "main" or "extra"
    media_type TEXT NOT NULL DEFAULT 'Movie',      -- "Movie" or "Tv"
    series_name TEXT,                 -- TV series name
    season_number INTEGER,
    episode_number INTEGER,
    episode_title TEXT,               -- TV episode title
    source TEXT,                      -- "tmdb", "omdb", "Manual"
    subtitle_text TEXT,               -- OCR-extracted subtitles
    duplicate_path TEXT,              -- Duplicate file path
    title_group_id TEXT,              -- TitleGroup ID
    genres TEXT,                      -- JSON array: ["Drama", "Crime"]
    runtime INTEGER,                  -- Runtime in minutes
    content_rating TEXT,              -- "PG-13", "TV-MA"
    overview TEXT,                    -- Plot summary
    cast_list TEXT,                   -- JSON array: ["Brad Pitt", ...]
    reviewed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,         -- RFC3339 timestamp
    updated_at TEXT NOT NULL          -- RFC3339 timestamp
);

CREATE INDEX idx_files_status ON files(status);
```

### `title_groups` Table

```sql
CREATE TABLE title_groups (
    id TEXT PRIMARY KEY,              -- SHA-256 of directory path (first 16 hex)
    path TEXT NOT NULL,               -- Directory path
    title TEXT NOT NULL,              -- Cleaned title
    year INTEGER,
    tmdb_id INTEGER,
    status TEXT NOT NULL DEFAULT 'Scanned',
    media_type TEXT NOT NULL DEFAULT 'Movie',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_title_groups_status ON title_groups(status);
```

---

## File Operations

### `save_file(file: &FileRecord) -> Result<(), anyhow::Error>`

**Behavior:**
1. Checks if file with same `id` already exists
2. If exists: **upsert** — updates only scan-related fields (path, filename, size, hash, updated_at), **preserves** pipeline status, canonical titles, and review decisions
3. If new: inserts full record

**Why upsert?** Re-scanning a directory should update file metadata without losing user review decisions.

### `update_file(file: &FileRecord) -> Result<(), anyhow::Error>`

Full record replacement — updates all fields.

### `get_files_by_status(status: &str) -> Result<Vec<FileRecord>, anyhow::Error>`

Query files by pipeline status.

**Used by:** analyze command, review command, move command

### `get_all_files() -> Result<Vec<FileRecord>, anyhow::Error>`

Return all file records.

**Used by:** duplicate command

### `get_file_by_id(id: &str) -> Result<Option<FileRecord>, anyhow::Error>`

Get a single file by ID.

**Used by:** file API, review API

### `delete_file(id: &str) -> Result<(), anyhow::Error>`

Delete a file record.

**Used by:** file API

### `find_file_by_hash(hash: &str) -> Result<Option<FileRecord>, anyhow::Error>`

Check if a file with the given hash exists.

**Used by:** library scan, watch command (duplicate detection)

### `count_files_by_status() -> Result<HashMap<String, i64>, anyhow::Error>`

Count files by status.

**Used by:** status command, dashboard stats

### `count_files_by_status_all() -> Result<HashMap<String, i64>, anyhow::Error>`

Count files by status including "Failed" status.

**Used by:** status command

---

## Title Group Operations

### `save_title_group(group: &TitleGroup) -> Result<(), anyhow::Error>`

Save or update a title group.

**Used by:** scan command, library scan command

### `get_title_group_by_id(id: &str) -> Result<Option<TitleGroup>, anyhow::Error>`

Get a title group by ID.

### `get_all_title_groups() -> Result<Vec<TitleGroup>, anyhow::Error>`

Get all title groups.

---

## Backup and Restore

### `backup(&destination: Path) -> Result<(), anyhow::Error>`

Create a timestamped snapshot of the database.

**Behavior:**
1. Uses SQLite's `sqlite3_backup` API for hot backup
2. Copies the entire database to `destination`

### `restore(&source: Path) -> Result<(), anyhow::Error>`

Restore the database from a backup file.

**Behavior:**
1. Attaches backup database: `ATTACH DATABASE '{source}' AS backup_db`
2. Begins transaction
3. Drops existing user tables
4. Recreates tables from backup DDL
5. Copies data from backup: `INSERT INTO main."table" SELECT * FROM backup_db."table"`
6. Recreates indexes from backup DDL
7. Commits transaction
8. Detaches backup database

**Edge case handling:**
- `DETACH` must run outside the transaction — an open read transaction on the attached DB makes SQLite refuse to detach with "database is locked"
- Single dedicated connection used for attach/detach operations
- Schema and data copied atomically within transaction

---

## JSON Columns

### `genres` and `cast_list`

Stored as JSON-encoded TEXT columns.

**Encoding:**
```rust
fn json_encode_list(value: &Option<Vec<String>>) -> Option<String> {
    value.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default())
}
```

**Decoding:**
```rust
fn json_decode_list(value: Option<String>) -> Option<Vec<String>> {
    value.and_then(|s| serde_json::from_str(&s).ok())
}
```

### Why `cast_list` not `cast`?

`CAST` is a SQLite keyword. The column is named `cast_list` to avoid conflicts.

### Row to FileRecord Conversion

```rust
fn row_to_file_record(&self, row: &sqlx::sqlite::SqliteRow) -> Result<FileRecord, anyhow::Error>
```

Maps SQL columns to `FileRecord` fields, applying JSON decode for `genres` and `cast_list`.

---

## Migrations

### Migration Files

Located in `infra/src/migrations/`:

| File | Description |
|---|---|
| `001_create_files_table.sql` | Initial files table |
| `002_create_title_groups_table.sql` | Title groups table |
| `003_add_enrichment_columns.sql` | Add genres, runtime, content_rating, overview, cast_list |
| `004_add_subtitle_and_group_columns.sql` | Add subtitle_text, duplicate_path, title_group_id |

### Migration Behavior

- `sqlx::migrate!()` embeds migration files at compile time
- `sqlx::migrate!().run()` applies pending migrations in order
- Migrations are idempotent — safe to run multiple times
- Failed migrations stop the pipeline

---

## Connection Pool

| Setting | Value |
|---|---|
| Max connections | 5 |
| Journal mode | WAL (Write-Ahead Logging) |
| Create if missing | true |

**WAL mode benefits:**
- Concurrent reads while writes are in progress
- Better performance for read-heavy workloads (dashboard, API)
- No reader blocking writer

---

## Where This Lives

| Concern | File |
|---|---|
| Database struct | `infra/src/db.rs` |
| File operations | `infra/src/db.rs` |
| Title group operations | `infra/src/db.rs` |
| Backup/Restore | `infra/src/db.rs` |
| JSON helpers | `infra/src/db.rs` |
| Row conversion | `infra/src/db.rs` |
| Migrations | `infra/src/migrations/*.sql` |

---

## Notes

- **Ephemeral database** — SQLite DB is disposable; can be rebuilt via re-scan
- **Schema changes** — Made via versioned migrations (001–004)
- **Upsert on save** — Re-scanning preserves review decisions
- **JSON columns** — Genres and cast are JSON-encoded TEXT
- **WAL mode** — Enables concurrent reads during writes
- **Backup is hot** — Uses `sqlite3_backup` API, no downtime
- **Restore is atomic** — Schema + data copied in a transaction
- **ID is content-based** — First 16 hex chars of SHA-256 hash
- **Indexes** — `idx_files_status` and `idx_title_groups_status` for status queries
- **Timestamps** — RFC3339 format for `created_at` and `updated_at`
