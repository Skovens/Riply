# Feature: Disc Name Derivation & Post-Rip Move

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
When a disc is inserted, derive a clean media name from it, let the user override it, then after the rip completes, move the ripped content to the derived-named folder. If no name is derived or set, skip the move — the rip folder already has the disc name, which is the final state.

## Behavior

### 1. Disc Inserted — Auto-Derive

When `listDrives()` returns a drive with `disc_name`, the backend tries to derive a clean name:

- **Auto-derive succeeds:** The drive card shows both names:
  ```
  Disc: Inception DVD
  → Inception (2007)
  ```
  The derived name is stored in `AppState` (in-memory, session-scoped) and used as the default destination for the rip.

- **Auto-derive fails:** The drive card shows only the raw disc name:
  ```
  Disc: Inception DVD
  ```
  No derived name is stored. The raw disc name is used as the default destination.

**Derivation logic:**
- Uses the existing `parse_disc_name()` from `infra/src/makemkv/parser.rs` (already extracts title + year from strings like "INCEPTION_2010")
- For non-parseable names (e.g., "My Home Video"), the derivation is considered "failed" — no derived name is set
- The derivation happens in `list_drives()` handler in `web/src/routes/ripper.rs`, after detecting drives. It's called as part of the drive poll.

### 2. User Override — Click to Name

The drive card's disc name area becomes clickable **only when a disc is inserted** (not when empty).

- Clicking opens the existing `openMediaNamingModal()` with the disc name pre-filled
- The user searches for the correct media via `searchApi.search()`
- When the user picks a result, the choice is stored in `AppState` (in-memory, session-scoped)
- The drive card updates to show the user-set name
- The user can click again to change the name at any time before starting the rip

### 3. Start Rip — Use Current Name

When the user clicks "Start Rip":

- **If a name is set** (derived or user-set): the rip starts in `{movies_directory}/{derived_name}/` (or `{tv_directory}/{series_name}/Season {N}/` for TV)
- **If no name is set**: the rip starts in `{movies_directory}/{disc_name}/` (raw disc name, e.g., "Inception DVD")
- The rip runs independently — no waiting for naming to resolve
- The destination folder path is passed to `backend.start_rip(drive_index, destination)`

### 4. Rip Complete — Post-Rip Move

When the rip completes successfully (`RipEvent::Complete`):

- **If a final name was set AND differs from the rip folder name:**
  - Move all ripped content from `{movies_directory}/{rip_folder}/` to `{movies_directory}/{final_name}/`
  - This is a simple `std::fs::rename()` (atomic on same filesystem)
  - If the final name already exists (collision), append ` (Copy N)` to the destination (same logic as `unique_rip_destination()`)
  - Emit a `RipEvent::Message` with the move result: "Moved to Inception (2007)"

- **If no final name was set OR final name == rip folder name:**
  - Skip the move — the rip is already in the correct folder
  - Emit a `RipEvent::Message` with: "Rip complete" (no move needed)

### 5. Rip Failed — Cleanup

When the rip fails or errors occur (`RipEvent::Error`):

- Delete the rip folder and all its contents
- Emit a `RipEvent::Message` with: "Rip failed, cleaned up"

### 6. UI — Drive Card Display

The drive card shows the naming state:

```svelte
<!-- When disc name + derived name exist -->
<div class="drive-card__disc">
  <span class="disc-name">{drive.disc_name}</span>
  <span class="drive-card__arrow">→</span>
  <span class="drive-card__derived-name">{drive.derived_name}</span>
</div>
<!-- Clickable: opens naming modal -->

<!-- When only disc name exists (no derived name) -->
<div class="drive-card__disc">
  <span class="disc-name">{drive.disc_name}</span>
</div>
<!-- Clickable: opens naming modal -->

<!-- When no disc (empty drive) -->
<div class="drive-card__disc">No disc</div>
<!-- Not clickable -->
```

**Styling:**
- `drive-card__disc` is clickable when a disc is inserted (cursor: pointer, hover underline)
- Arrow `→` is subtle gray between disc name and derived name
- Derived name is shown in a muted color to distinguish from the raw disc name

## Acceptance Criteria

- [ ] AC1: When a disc with a parseable name (e.g., "INCEPTION_2010") is inserted, the drive card shows both the raw disc name and the derived name ("Inception (2007)")
- [ ] AC2: When a disc with an unparsable name (e.g., "My Home Video") is inserted, only the raw disc name is shown — no derived name
- [ ] AC3: The disc name area is clickable when a disc is inserted, opening the naming modal with the disc name pre-filled
- [ ] AC4: When the user picks a search result, the derived name updates on the drive card
- [ ] AC5: Starting a rip uses the derived/user-set name as the destination folder (if one exists), or the raw disc name (if not)
- [ ] AC6: When the rip completes successfully and the final name differs from the rip folder, content is moved to the final-named folder
- [ ] AC7: When the rip completes successfully and no final name was set, no move occurs — the rip folder is the final state
- [ ] AC8: When the rip fails, the rip folder is deleted
- [ ] AC9: `cargo clippy --all-targets -- -D warnings` passes with 0 warnings
- [ ] AC10: `cargo test` passes with no failures
- [ ] AC11: Screenshot of the drive card showing derived name is provided for review

## Files Likely Affected

### Backend (Rust)

| File | What changes |
|---|---|
| `core/src/ripping.rs` | Add `derived_name: Option<String>` and `final_name: Option<String>` to `DriveInfo` |
| `infra/src/makemkv/backend.rs` | Call derivation logic after parsing disc names in `detect_drives()` |
| `infra/src/makemkv/parser.rs` | Add `derive_name(disc_name: &str) -> Option<(String, Option<u16>)>` — wraps existing `parse_disc_name()` with a clean API |
| `web/src/routes/ripper.rs` | Add `/v1/ripper/drives/:index/rename` POST endpoint; update `list_drives` to return derived/final names; add post-rip move/cleanup logic |
| `web/src/routes/ripper.rs` (AppState) | Add `disc_names: RwLock<HashMap<usize, DiscNameInfo>>` to store derived/user-set names |

### Frontend (TypeScript/Svelte)

| File | What changes |
|---|---|
| `web/svelte/src/lib/types.ts` | Add `derived_name?: string` and `final_name?: string` to `DriveInfo` |
| `web/svelte/src/lib/api/ripper.ts` | Add `rename(index, choice)` method |
| `web/svelte/src/routes/+page.svelte` | Make disc name clickable; show derived name with arrow; handle naming modal open |
| `web/svelte/src/routes/+page.svelte` | Handle post-rip move message from SSE events |

## Where This Lives

| Concern | Layer |
|---|---|
| Auto-derive from disc name | `infra/src/makemkv/parser.rs` |
| Store derived/user-set name | `AppState` (in-memory, session-scoped) |
| User override via search | `web/svelte/src/lib/api/media-naming.ts` (reuse existing modal) |
| Start rip in correct folder | `web/src/routes/ripper.rs` (start_rip handler) |
| Post-rip move/cleanup | `web/src/routes/ripper.rs` (after `RipEvent::Complete`/`Error`) |
| UI display | `web/svelte/src/routes/+page.svelte` |

## Notes

- **In-memory storage:** Derived/user-set names are stored in `AppState` (RwLock). They survive drive polls but are lost on server restart. This is intentional — the user can re-derive or re-set after a restart.
- **No DB migration:** Since names are session-scoped, no database changes are needed.
- **Reuse existing modal:** The `openMediaNamingModal()` already handles search + selection. We just call it from the drive card instead of the naming flow.
- **Reuse existing search:** The `searchApi.search()` already queries TMDB/OMDB. No new API endpoint needed for search.
- **Post-rip move is a rip lifecycle concern:** The move/cleanup logic belongs in the rip event handler (where we see `Complete`/`Error`), not in the naming layer.
- **Collision handling:** If the final name already exists, append ` (Copy N)` — same logic as `unique_rip_destination()` used for the initial rip folder.
