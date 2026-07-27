# Task 3: Fix SQL Migration Casing

## Status: DONE

## Problem

`infra/migrations/001_create_files_table.sql:9` uses `DEFAULT 'SCANNED'`
(all caps) but Rust code stores status as `"Scanned"` (title case).

This is a latent bug: `save_file()` always sets status explicitly, so the
default is never used in practice. But if any future code path inserts
without setting status, the casing mismatch would cause `parse_status()`
to fail.

## Fix

Change the migration default to match Rust conventions:

```sql
status TEXT NOT NULL DEFAULT 'Scanned',
```

### Note on existing databases

The migration uses `CREATE TABLE IF NOT EXISTS`, so existing databases
won't be re-run. This fix only affects fresh installs. For existing DBs
the default is already set per-row and won't change.

## Files

| File | Change |
|------|--------|
| `infra/migrations/001_create_files_table.sql` | Line 9: `'SCANNED'` → `'Scanned'` |

## Verification

- `cargo test --all`
- Verify no test depends on the uppercase default
