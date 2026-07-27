# Task 6: SQL Pagination (LIMIT/OFFSET)

## Status: DONE

## Problem

`web/src/api.rs:list_files` calls `db.get_all_files()` or
`db.get_files_by_status()`, fetches ALL records into memory, then
paginates in Rust. For large libraries (10K+ files) this is wasteful
and slow.

## Fix

### New database methods

Add to `infra/src/db.rs`:

```rust
pub async fn get_files_paginated(
    &self,
    status: Option<&str>,
    page: u32,
    per_page: u32,
) -> Result<(Vec<FileRecord>, i64), anyhow::Error>
```

Returns `(files, total_count)` in one query using SQL `LIMIT`/`OFFSET`
and a separate `COUNT(*)` query.

```rust
pub async fn count_files(&self, status: Option<&str>) -> Result<i64, anyhow::Error>
```

### Updated API handler

Replace the in-memory pagination in `list_files` with:

```rust
let (files, total) = state.db.get_files_paginated(
    params.status.as_deref(),
    page,
    per_page,
).await?;
```

### SQL queries

```sql
-- Count
SELECT COUNT(*) FROM files WHERE status = ?  -- or no WHERE if status is None

-- Paginated
SELECT ... FROM files
WHERE status = ?  -- or no WHERE
ORDER BY updated_at DESC
LIMIT ? OFFSET ?
```

## Files

| File | Change |
|------|--------|
| `infra/src/db.rs` | Add `get_files_paginated()`, `count_files()` |
| `web/src/api.rs` | Update `list_files` handler to use new method |

## Verification

- `cargo test --all`
- Test with page=1, per_page=10 — should return correct slice
- Test count matches total items
