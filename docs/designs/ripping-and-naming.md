# Feature: Ripping & Auto-Naming

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose

Riply automates DVD/Blu-ray ripping and naming. The core insight: **ripping and naming are independent**. Ripping is a time-consuming I/O operation. Naming is a metadata lookup that can happen in parallel. The software tries to do as much work as possible upfront, reducing user steps.

---

## Two Independent Workflows

### Workflow A: Ripping (I/O-bound)

```
Disc inserted → detected by MakeMKV
    ↓
User clicks "Rip" (or autorip triggers)
    ↓
MakeMKV rips to temp/{index}/
    ↓
Files stay in temp until user accepts naming
```

**Rules:**
- Ripping is its own thing, has nothing to do with renaming
- Ripping starts when user clicks Rip (or autorip triggers)
- Ripping destination is always `temp/{index}/` (configurable temp directory)
- Ripping does NOT wait for naming to resolve
- After rip completes, files remain in temp until user accepts naming

### Workflow B: Auto-Naming (metadata lookup)

```
Disc inserted → detected by MakeMKV
    ↓
Parse disc name → search TMDb/OMDb → return suggested name
    ↓
Show suggestion in naming list below drives
    ↓
User accepts/rejects/searches
    ↓
If accepted → move rip from temp to library
```

**Rules:**
- Auto-naming starts at **disc detection**, NOT at rip start
- Auto-naming runs in background, independent of rip
- Auto-naming has a 30-second timeout (non-blocking)
- If auto-naming succeeds → `suggested_name = Some(title, year)`
- If auto-naming fails → `suggested_name = None`, UI shows "naming failed"
- User can always manually rename via search popup

---

## Data Flow

```
disc → MakeMKV → temp/{index}/ → TMDb lookup → naming list (if not accepted)
                              ↘ user accepts → library/{jellyfin}/
```

Two staging areas:

| Area | Path | Lifespan | Purpose |
|------|------|----------|---------|
| **Temp** | `/tmp/riply/0/` | Until moved or age-cleanup | Raw rip output, stays here until user accepts a name |
| **Library** | `movies_dir` / `tv_dir` | Permanent | Final Jellyfin-compliant structure |

---

## State Machine

### Drive States

```
No disc → Ready → Ripping → Complete → Ready (ejected)
                      ↓
                    Error → Ready (retry)
```

### Naming States

```
No disc → Disc detected → Auto-naming in progress → Named / Naming failed
                                              ↓
                                    User accepts / rejects / searches
                                              ↓
                                    Moved to library / Left in temp
```

**Key invariant:** Naming state is independent of rip state. A drive can be:
- Ready (no rip) + Named (auto-named before rip)
- Ripping + Naming in progress (naming runs in parallel)
- Complete (rip done) + Naming accepted (auto-moved to library)
- Complete (rip done) + Naming failed (waiting for user)

---

## Implementation Details

### 1. Disc Detection (`list_drives`)

**Trigger:** MakeMKV returns a drive with `disc_name.is_some()`

**Action:** If no naming info exists for this drive (`!naming_completed.contains(index)`):
1. Parse disc name using `infra::makemkv::parser::parse_disc_name(disc_name)`
2. Search TMDb/OMDb using `searcher.search(title, year, MediaType)`
3. Store result in `AppState.naming_suggestions[index] = Some((title, year))` or `None`
4. Mark as completed: `AppState.naming_completed.insert(index)`
5. Send SSE event: `rip:named` (success) or `rip:naming_failed` (failure)
6. Send SSE event: `naming:list_updated` (naming list changed)

**Timeout:** 30 seconds. If exceeded, treat as failure.

**Important:** This runs in a `tokio::spawn` background task. It does NOT block the drive detection response.

### 2. Auto-Naming Logic (`resolve_rip_title_with_duration`)

```rust
async fn resolve_rip_title_with_duration(
    disc_name: &str,
    rip_folder: &str,
    searcher: Option<&dyn MetadataSearcher>,
) -> (String, Option<u16>)
```

**Steps:**
1. Parse disc name: `parse_disc_name(disc_name)` → `(title, year)`
2. If title is empty → return `(title, year)` (no search possible)
3. If `searcher` is `None` → return `(title, year)` (no API configured)
4. Clean title: `NameCleaner::clean_name_for_search(&title)`
5. Search TMDb: `searcher.search(clean_title, year, MediaType::Movie)`
6. If no movie match, search TV: `searcher.search(clean_title, year, MediaType::Tv)`
7. If TV match found, verify episode duration (non-blocking, non-fatal)
8. Return `(matched_title, matched_year)` or `(title, year)` if no match

**Search order:** Movies first, then TV. Takes first match from each.

### 3. Naming List

**What it is:** A global list below all drive cards, always visible.

**When populated:** When auto-naming completes (success or failure).

**Each entry shows:**
- **Named successfully:** `Original Disc Title → Auto-Detected Title (Year)` with thumbnail
- **Naming failed:** `Original Disc Title → Media naming failed, please select title`

**User actions:**
- Click entry → open search popup, manually name the disc
- Click "Accept" → move rip folder from temp to library using accepted name
- Click "Reject" → leave rip folder in temp (age-cleanup handles old rips)

**API endpoint:** `GET /api/ripper/naming-list`
```json
[
  {
    "drive_index": 0,
    "original_name": "INCEPTION_2010",
    "suggested_name": "Inception",
    "year": 2010,
    "status": "named", // or "naming_failed"
    "thumbnail_url": "/api/media/thumbnail/550"
  }
]
```

### 4. Accept/Reject Flow

**Accept naming:**
- **If rip NOT in progress:** Move rip folder immediately (see "Post-Rip Move" below)
- **If rip IS in progress:** Queue the accept. Store intent in `AppState.pending_accepts[drive_index]`. Return "queued" response to user. When rip completes, automatically execute the move.

**Reject naming:**
- Leave rip folder in temp
- Send SSE event: `naming:rejected`
- Remove from naming list

**Search for name:**
1. Open search popup with disc name pre-filled
2. User searches TMDb/OMDb
3. User picks result
4. Update `AppState.naming_suggestions[drive_index]` with new name
5. User can then accept

**Queue-based accept behavior:**
- User accepts → store in `AppState.pending_accepts[drive_index] = (suggested_name, year)`
- While rip is in progress → do nothing, just store the intent
- When rip completes → check if `pending_accepts[drive_index]` exists
- If yes → move rip folder to library with accepted name
- If no → leave rip in temp, add to naming list for later
- Accept endpoint returns "queued" response (not error) if rip in progress
- Naming list UI shows "Accept queued — will move when rip completes" for drives that are ripping

### 5. Post-Rip Move

**When:** User accepts naming (not automatic on rip completion)

**Immediate move (rip not in progress):**
1. Get rip folder: `temp/{drive_index}/`
2. Get library destination: `movies_dir/{accepted_name} ({year})/`
3. Move rip folder to library destination
4. If destination exists, append ` (Copy N)` suffix
5. Send SSE event: `naming:accepted`
6. Remove from naming list

**Queued move (rip in progress):**
1. Store intent in `AppState.pending_accepts[drive_index] = (accepted_name, year)`
2. Return "queued" response to user: "Naming accepted — will move when rip completes"
3. When rip completes (in `start_rip` completion handler):
   - Check `AppState.pending_accepts[drive_index]`
   - If present → execute move (steps 1-5 above)
   - Remove from `pending_accepts`
   - Send `naming:accepted` SSE event
   - Remove from naming list
   - If absent → leave rip in temp, add to naming list

**Important:** The move is triggered by user acceptance, NOT by rip completion alone. Rip completion only adds the entry to the naming list. If user accepted while rip was in progress, the move executes automatically when rip finishes.

### 6. Drive Detection

**Event-driven detection replaces polling.** Sources tried in priority order:
1. udev (Linux kernel device manager events)
2. inotify on `/sys/block/` (filesystem event notifications)
3. inotify on `/dev/sr*` (device node events)
4. D-Bus (systemd-logind / udisks2 signals)

First available source wins. Selected at server startup. Stored in AppState.

**Autorip gating:**
- Event source available → autorip toggle enabled
- No event source → autorip toggle disabled with tooltip: "Event-driven detection unavailable — requires udev, inotify, or D-Bus"

**Manual refresh:**
- Refresh button always present per-drive card
- Clicking refresh calls `list_drives()` for THAT drive only (not all drives)
- Button disabled during refresh (prevents double-click)
- Shows loading state per-drive
- Initial page load auto-refreshes all drives once (on mount)
- No continuous polling between events

**Error handling:**
| Scenario | Behavior |
|----------|----------|
| MakeMKV fails (disc scratch, hardware) | Descriptive error message. User can retry. |
| Drive removed mid-rip | Rip aborts, progress is lost. |
| TMDb down / rate limited | Rip completes, falls back to basic/manual naming. |
| Destination path not writable | Folder stays in temp. User is notified. |
| Disk full | Basic information displayed. |
| No TMDb/OMDb configured | Manual name entry mode. Rip still works, just no auto-naming. |
| Auto-naming timeout (30s) | Treat as naming failure. Entry appears in naming list with "naming failed". |
| Event fires but list_drives fails | Show "Detection failed — click Refresh" on drive card. Log warning. |
| Event source dies mid-session | Log warning. Continue with existing state. User can click Refresh. |

---

## Data Structures

### AppState Fields

```rust
pub struct AppState {
    // Existing fields...
    
    // Naming suggestions: drive_index → (title, year) or None if failed
    pub naming_suggestions: RwLock<HashMap<usize, Option<(String, u16)>>>,
    
    // Drives that have been auto-named (to avoid re-naming)
    pub naming_completed: RwLock<HashSet<usize>>,
    
    // Drives with completed rips (for fallback when MakeMKV loses drive access)
    pub completed_rips: RwLock<HashSet<usize>>,
}
```

### DriveInfo

```rust
pub struct DriveInfo {
    pub index: usize,
    pub name: String,
    pub disc_name: Option<String>,
    pub derived_name: Option<String>, // Auto-detected name (from parsing)
    pub suggested_name: Option<String>, // Auto-detected name (from TMDb)
    pub device_path: Option<String>,
    pub state: DriveState,
}
```

### SSE Events

| Event | Payload | When |
|-------|---------|------|
| `rip:starting` | `{"index": 0}` | Rip starts |
| `rip:progress` | `{"index": 0, "progress": 50, "stage": "title_select"}` | Rip progress |
| `rip:complete` | `{"index": 0}` | Rip completes |
| `rip:failed` | `{"index": 0, "message": "Disc unreadable"}` | Rip fails |
| `rip:named` | `{"index": 0, "data": {"title": "Inception", "year": 2010, "type": "movie"}}` | Auto-naming succeeds |
| `rip:naming_failed` | `{"index": 0, "data": {}}` | Auto-naming fails |
| `drive:updated` | `{"index": 0, "data": {}}` | Drive state changed |
| `naming:list_updated` | `{"data": [...]}` | Naming list changed |
| `naming:accepted` | `{"index": 0}` | User accepted naming |
| `naming:rejected` | `{"index": 0}` | User rejected naming |

---

## API Endpoints

### Existing Endpoints (Unchanged)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/ripper/drives` | List available drives |
| POST | `/api/ripper/:index/start` | Start ripping |
| POST | `/api/ripper/:index/cancel` | Cancel ripping |
| POST | `/api/ripper/:index/rename` | User manually sets name |
| POST | `/api/ripper/:index/eject` | Eject disc |

### New Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/ripper/naming-list` | Get naming list |
| POST | `/api/ripper/:index/accept` | Accept naming and move to library |
| POST | `/api/ripper/:index/reject` | Reject naming (leave in temp) |

### Endpoint Details

#### `GET /api/ripper/naming-list`

Returns all naming suggestions (named or failed) that haven't been accepted/rejected yet.

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "drive_index": 0,
      "original_name": "INCEPTION_2010",
      "suggested_name": "Inception",
      "year": 2010,
      "status": "named",
      "thumbnail_url": "/api/media/thumbnail/550"
    },
    {
      "drive_index": 1,
      "original_name": "MY_HOME_VIDEO",
      "suggested_name": null,
      "year": null,
      "status": "naming_failed",
      "thumbnail_url": null
    }
  ]
}
```

#### `POST /api/ripper/:index/accept`

Accept the naming suggestion and move rip folder to library.

**Request:** (empty body)

**Response:**
```json
{
  "success": true,
  "data": {
    "message": "Moved to Inception (2010)"
  }
}
```

**Error responses:**
- `404` — Drive not found or no rip completed
- `400` — No naming suggestion available

#### `POST /api/ripper/:index/reject`

Reject the naming suggestion. Rip folder stays in temp.

**Request:** (empty body)

**Response:**
```json
{
  "success": true,
  "data": {
    "message": "Rip left in temp directory"
  }
}
```

**Error responses:**
- `404` — Drive not found or no rip completed

---

## Acceptance Criteria

- [ ] AC1: When a disc is inserted, auto-naming starts immediately (not after rip)
- [ ] AC2: Auto-naming runs in background, does NOT block rip
- [ ] AC3: Auto-naming has 30-second timeout, treated as failure if exceeded
- [ ] AC4: Naming list appears below drive cards, always visible
- [ ] AC5: Naming list shows both successful and failed naming attempts
- [ ] AC6: User can accept naming → rip moves to library
- [ ] AC7: User can reject naming → rip stays in temp
- [ ] AC8: User can search for name → update suggestion → accept
- [ ] AC9: Ripping works independently of naming (no waiting for naming)
- [ ] AC10: `cargo clippy --all-targets -- -D warnings` passes with 0 warnings
- [ ] AC11: `cargo test` passes with no failures

---

## Files Affected

### Backend (Rust)

| File | Changes |
|------|---------|
| `web/src/routes/ripper.rs` | Trigger auto-naming on disc detection (not rip completion); add naming list endpoint; add accept/reject endpoints |
| `web/src/api.rs` | Add naming list data structure to AppState |
| `web/svelte/src/lib/types.ts` | Add `suggested_name` to DriveInfo |
| `web/svelte/src/lib/api/ripper.ts` | Add naming list methods |
| `web/svelte/src/routes/+page.svelte` | Add naming list component below drive cards |

### Frontend (TypeScript/Svelte)

| File | Changes |
|------|---------|
| `web/svelte/src/routes/+page.svelte` | Add naming list component; handle `naming:list_updated` SSE event |
| `web/svelte/src/lib/types.ts` | Add naming list entry type |
| `web/svelte/src/lib/api/ripper.ts` | Add `getNamingList()`, `acceptNaming(index)`, `rejectNaming(index)` |

---

## Notes

- **In-memory storage:** Naming suggestions are stored in `AppState` (RwLock). They survive drive polls but are lost on server restart. This is intentional — the user can re-detect and re-name after a restart.
- **No DB migration:** Since naming suggestions are session-scoped, no database changes are needed.
- **Reuse existing search:** The `searchApi.search()` already queries TMDb/OMDB. No new API endpoint needed for search.
- **Reuse existing modal:** The `openMediaNamingModal()` already handles search + selection. We just call it from the naming list entries.
- **Post-rip move is a naming concern:** The move/cleanup logic belongs in the naming accept/reject handlers, not in the rip event handler.
- **Collision handling:** If the final name already exists, append ` (Copy N)` — same logic as `unique_rip_destination()` used for the initial rip folder.
- **Age cleanup:** Rips left in temp are cleaned up by age (configurable, default 7 days) via `cleanup_temp_rips()`.
