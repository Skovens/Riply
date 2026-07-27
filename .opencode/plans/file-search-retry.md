# Task 10: File Text Search + Batch Retry Failed

## Status: DONE

## Problem

1. Files page only filters by status — no text search for filename or title.
2. Failed files have no batch retry mechanism.

## Fix

### Part A: Text search

**Backend** — Add `search` query param to `GET /api/files`:

```rust
// In api.rs list_files handler
if let Some(ref search) = params.search {
    // Add WHERE clause: filename LIKE ? OR canonical_title LIKE ?
}
```

Update `db.get_files_paginated()` (from Task 6) to accept an optional
search string and add `LIKE` clauses.

**Frontend** — Add search input above the files table:

```svelte
<input
  type="text"
  placeholder="Search files..."
  oninput={handleSearch}
/>
```

Debounce input (300ms) to avoid hammering the API.

### Part B: Batch retry failed

**Backend** — Add `POST /api/analyze/retry-failed` endpoint:

```rust
async fn retry_failed(
    State(state): State<Arc<AppState>>,
) -> Result<Json<JobStartedResponse>, ...>
```

Sets all `Failed` files back to `Scanned`, then starts an analyze job.

**Frontend** — Add "Retry All Failed" button on dashboard or files page
when Failed files exist.

## Files

| File | Change |
|------|--------|
| `infra/src/db.rs` | Add search param to paginated query |
| `web/src/api.rs` | Add search param handling, add retry-failed endpoint |
| `web/svelte/src/routes/files/+page.svelte` | Add search input |
| `web/svelte/src/lib/api/files.ts` | Add search param to list() |
| `web/svelte/src/routes/+page.svelte` | Add retry failed button (optional) |

## Verification

- `cargo test --all`
- Search "matrix" — should find files with that in filename/title
- Retry failed — should reset Failed files to Scanned and start analysis
