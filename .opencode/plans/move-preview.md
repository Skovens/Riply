# Task 9: Move Preview in Web UI

## Status: DONE

## Problem

`MoverService::preview_move()` exists but is never called from the web API.
Users can't see where files will go before committing to the move.

## Fix

### New API endpoint

Add `POST /api/move/preview` to `web/src/api.rs`:

```rust
async fn preview_move(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<MovePreviewItem>>, ...>
```

Returns a list of `{ file_id, filename, source_path, dest_path }` for all
Confirmed files.

### New database method (if needed)

`db.get_files_by_status("Confirmed")` already exists. Use it.

### Frontend

Add a "Preview" button next to the "Move" button on the dashboard.
On click, fetch `/api/move/preview` and display a table of
source → destination mappings.

Or: add a `/move` route with preview table + confirm button.

## Files

| File | Change |
|------|--------|
| `web/src/api.rs` | Add `POST /api/move/preview` endpoint |
| `app/src/mover.rs` | `preview_move()` already exists, may need to return structured data |
| `web/svelte/src/routes/+page.svelte` or new `move/+page.svelte` | Add preview UI |
| `web/svelte/src/lib/api/move.ts` | Add `preview()` method |

## Verification

- `cargo test --all`
- Start server, click Preview — should show file destinations
- Verify paths match what `calculate_destination()` would produce
