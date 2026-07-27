# Task 12: Duplicate Detection

## Status: DONE

## Problem

`FileRecord.duplicate_path` exists in the schema but is never populated.
No logic detects when two scanned files have the same SHA-256 hash.

## Fix

### Detection approach

After scanning a directory, compare hashes of all newly scanned files
against existing files in the database. If a hash match is found, set
`duplicate_path` on the newer record pointing to the existing one.

### Implementation

Add to `ScannerService` or as a post-scan step:

```rust
pub async fn detect_duplicates(
    &self,
    db: &Database,
    new_files: &[FileRecord],
) -> Result<Vec<FileRecord>> {
    let mut updated = Vec::new();
    for file in new_files {
        if let Some(existing) = db.find_file_by_hash(&file.hash).await? {
            let mut dup = file.clone();
            dup.duplicate_path = Some(existing.path.clone());
            updated.push(dup);
        }
    }
    Ok(updated)
}
```

### New database method

```rust
pub async fn find_file_by_hash(
    &self,
    hash: &str,
) -> Result<Option<FileRecord>>
```

### Frontend

Add a "Duplicates" filter option to the files page status dropdown.
Show `duplicate_path` in the file details if present.

### Workflow

1. Scanner runs, produces new FileRecords with hashes
2. Before saving, check each hash against existing DB records
3. If duplicate found, set `duplicate_path` on the new record
4. Save with the duplicate info populated
5. User can filter by duplicates and decide what to do (delete, ignore)

## Files

| File | Change |
|------|--------|
| `app/src/scanner.rs` | Add duplicate detection after scanning |
| `infra/src/db.rs` | Add `find_file_by_hash()` |
| `web/svelte/src/routes/files/+page.svelte` | Add "Duplicates" filter option |

## Tests

- Scan two identical files → second should have `duplicate_path` set
- Scan unique files → no duplicates detected
- Test `find_file_by_hash` returns correct record

## Verification

- `cargo test --all`
- Scan a directory with duplicate files → verify duplicate_path populated
