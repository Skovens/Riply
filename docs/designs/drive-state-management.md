# Design: Drive State Management

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Manage optical drive discovery, state tracking, ripping, and auto-rip. This document describes the *current* behavior of the system.

---

## Drive State Machine

```
          ┌──────────────────────────────────────────────────┐
          │                                                    │
          ▼                                                    │
   ┌──────────┐    ┌───────┐    ┌────────┐    ┌──────────┐
   │  Empty   │───▶│Loading│───▶│  Ready │───▶│  Ripping │
   │ (no disc)│◀───│      │◀───│(disc)   │    │          │
   └──────────┘    └───────┘    └────────┘    └──────────┘
                                         │
                                         ▼
                                    ┌──────────┐
                                    │  Complete│
                                    │  (done)  │
                                    └──────────┘
```

### States

| State | Meaning | When |
|---|---|---|
| `Empty` | No disc in drive | `makemkvcon discinfo 0` reports no disc |
| `Loading` | Disc detected, metadata being read | Brief transition (milliseconds) |
| `Ready` | Disc is readable, rip can start | After `Loading` completes |
| `Ripping` | Rip is in progress | Set by `list_drives()` when `active_rips` contains the index |
| `Complete` | Rip finished successfully | Internal state from MakeMKV |
| `Error` | Rip encountered an error | Internal state from MakeMKV |

### State Transitions

| Transition | Triggered By |
|---|---|
| `Empty → Loading` | Disc inserted (detected by `detect_drives()`) |
| `Loading → Ready` | Disc metadata fully read |
| `Ready → Ripping` | `start_rip()` called, handle registered in `active_rips` |
| `Ripping → Complete` | Rip succeeds (MakeMKV reports `Complete`) |
| `Ripping → Error` | Rip fails (MakeMKV reports error) |
| `Any → Empty` | Disc ejected (manually or via `eject_disc()`) |

---

## Drive Discovery

### `list_drives()` — `GET /v1/ripper/drives`

**Behavior:**
1. Calls `backend.detect_drives()` (blocking, via `spawn_blocking`)
2. For each drive, checks `active_rips` map — if the index is present, overrides state to `Ripping`
3. Enriches each drive with `derived_name` from `disc_names` state (user-set names)
4. Returns `Vec<DriveInfo>` as JSON

**Error handling:**
- No backend: returns `[]` (empty list)
- Detection failure: returns 500 with error message

### `DriveInfo` fields

```rust
pub struct DriveInfo {
    pub index: usize,
    pub name: String,        // e.g., "HL-DT-ST DVDRAM GH24NS95"
    pub device_path: Option<String>,  // e.g., "/dev/sr0"
    pub state: DriveState,   // Empty, Loading, Ready, Ripping, Complete, Error
    pub disc_name: Option<String>,     // e.g., "INCEPTION_2010"
    pub derived_name: Option<String>,  // User-set final name (from disc_names state)
}
```

---

## Active Rips Tracking

### `AppState.active_rips: RwLock<HashMap<usize, JoinHandle<()>>>`

- Maps drive index → tokio task handle
- Populated when `start_rip()` spawns the rip task
- Cleared when the rip task completes (success or failure)
- Used by `list_drives()` to override drive state to `Ripping`
- Used by `autorip_poll_loop()` to pause detection while a rip is active

### `start_rip()` — `POST /v1/ripper/:index/start`

**Behavior:**
1. Validates backend is available
2. Resolves destination directory:
   - If `body.destination` is provided: use as-is (with uniqueness check via `unique_rip_destination()`)
   - Otherwise: use `config.rip_output_directory` + auto-generated subdirectory
3. Auto-generates subdirectory from disc name:
   - Calls `resolve_rip_title()` which searches TMDb/OMDb using the parsed disc name
   - If a match is found: uses matched title + year (e.g., "Inception (2010)")
   - If no match: uses raw parsed disc name
4. Calls `unique_rip_destination()` to append `(Copy N)` if directory exists
5. Starts rip via `backend.start_rip(index, dest)`
6. Spawns a task that runs `process_rip_events()` loop
7. Registers task in `active_rips`

**Destination resolution logic:**
```
explicit_dest provided?
  └─ Yes → use as-is (uniquify if needed)
  └─ No → config.rip_output_directory
           └─ Disc name exists?
              └─ Yes → resolve_rip_title(disc_name) → TMDb/OMDb search → matched title
              └─ No → no subdirectory
```

### `cancel_rip()` — `POST /v1/ripper/:index/cancel`

**Behavior:**
1. Removes the handle from `active_rips`
2. Calls `handle.abort()` to cancel the tokio task
3. Sends `rip:cancelled:{index}` via SSE
4. Returns success message (even if no rip was active)

---

## Rip Progress Events

### `process_rip_events()` — Shared event loop

Called by both `start_rip()` and `execute_rip()` (auto-rip path).

**Event types:**

| Event | SSE Output |
|---|---|
| `RipEvent::Progress { current, total, max }` | `rip:progress:{index}:{pct}:{eta}` or `rip:progress:{index}:{pct}` |
| `RipEvent::Complete` | `rip:complete:{index}` |
| `RipEvent::Error(e)` | `rip:error:{index}:{error_message}` |
| `RipEvent::Message(msg)` | `rip:message:{index}:{message}` |

### ETA Calculation

`calculate_eta()` uses exponential smoothing:
1. First update: records start time, no ETA
2. Subsequent updates: calculates rate as `(pct_delta / time_delta)`
3. ETA = `(100 - current_pct) / rate`
4. Only reports ETA if rate > 0 and at least 3 seconds have elapsed
5. ETA is capped at 100% (no progress shown)

---

## Autorip

### `autorip_status()` — `GET /v1/ripper/autorip`

Returns sorted list of drive indices with autorip enabled.

### `autorip_toggle()` — `POST /v1/ripper/autorip/:index/toggle`

Toggles autorip for a drive index. Returns updated sorted list.

### `autorip_poll_loop()` — Background loop

**Polling interval:** 5 seconds

**Conditions to skip polling:**
- No backend available
- Active rips exist (pauses detection — current rip is never interrupted)
- No drives have autorip enabled

**Per-poll behavior:**
1. Detects drives (blocking)
2. Broadcasts SSE events for drive state changes:
   - `drive:inserted:{index}` — Empty → Ready or first sighting
   - `drive:ejected:{index}` — Ready → Empty
   - `drive:removed:{index}` — Drive disappeared from system
3. Drops tracking for drives where autorip was disabled
4. For each autorip-enabled drive:
   - If state changed to Ready (new disc):
     - Skip if already ripping on this drive
     - Spawn `execute_rip()` task
     - Register in `active_rips`
     - **Only one rip started per poll cycle** (break after first)

**Key design decisions:**
- Only one rip at a time (prevents drive contention)
- Detection pauses during active rips (other drives picked up after completion)
- State tracking persists across polls (enables transition detection)

---

## Eject

### `eject_disc()` — `POST /v1/ripper/:index/eject`

**Behavior:**
1. Validates backend is available
2. Detects drives to find the device path for the index
3. Calls `infra::eject_disc(&device_path)` (blocking)
4. Returns success message

**Error handling:**
- No backend: 503 Service Unavailable
- Drive not found: 404
- Device path missing: 404
- Ejection failure: 500

---

## Disc Naming

### `rename_disc()` — `POST /v1/ripper/drives/:index/rename`

**Behavior:**
1. Validates backend is available
2. Detects drives to verify the drive exists and has a disc
3. Constructs `final_name` from the user's selection:
   - With year: `"Title (Year)"`
   - Without year: `"Title"`
4. Stores in `AppState.disc_names` (in-memory, session-scoped)
5. The name is returned by `list_drives()` as `derived_name`

### `AppState.disc_names: RwLock<HashMap<usize, DiscNameInfo>>`

- Maps drive index → `{ disc_name, final_name }`
- In-memory only (lost on server restart)
- Used by `list_drives()` to enrich drive info
- Used by `execute_rip()` to resolve destination directory

### `resolve_rip_title()` — Disc name → metadata match

**Behavior:**
1. Parses disc name using `infra::makemkv::parser::parse_disc_name()`
2. If backend has a `MetadataSearcher` (TMDb/OMDb):
   - Cleans title for search
   - Searches TMDb first, then OMDb on failure
   - Returns matched title + year
3. Falls back to raw parsed disc name if no searcher or no match

---

## SSE Event System

### `SseManager` — `web/src/sse.rs`

- Maintains a buffer of 256 events
- Clients connect via `GET /v1/sse`
- Events are sent as `data: {message}\n\n`

### Event format

```
kind:action:detail
```

Examples:
- `rip:progress:0:50:120` — drive 0, 50%, 120s ETA
- `rip:complete:0` — drive 0 rip complete
- `rip:message:0:Rip complete` — drive 0 message
- `drive:inserted:0` — drive 0 disc inserted
- `drive:ejected:0` — drive 0 disc ejected
- `drive:removed:0` — drive 0 removed
- `drive:changed:0` — drive 0 state changed (legacy, triggers full re-fetch)

### SSE event consumers

| Event | Handled By | Action |
|---|---|---|
| `rip:starting:{idx}` | Frontend | Show rip progress bar, set stage |
| `rip:progress:{idx}:{pct}` | Frontend | Update progress bar |
| `rip:progress:{idx}:{pct}:{eta}` | Frontend | Update progress bar + ETA |
| `rip:message:{idx}:{msg}` | Frontend | Update rip stage text |
| `rip:complete:{idx}` | Frontend | Show success toast, trigger auto-scan |
| `rip:error:{idx}:{msg}` | Frontend | Show error toast |
| `drive:inserted:{idx}` | Frontend | Re-fetch drives |
| `drive:ejected:{idx}` | Frontend | Re-fetch drives |
| `drive:removed:{idx}` | Frontend | Re-fetch drives |
| `drive:changed:{idx}` | Frontend | Re-fetch drives |
| `scan:progress/{starting}` | Frontend | Refresh jobs |
| `scan:complete/cancelled` | Frontend | Refresh jobs + files |
| `review:accepted/rejected` | Frontend | Refresh review queue + stats |

---

## Frontend Drive Display

### `+page.svelte` — Drive panel

**Data loading:**
- `fetchDrives()` called once on `onMount()`
- Caches drives in `localStorage` under `riply:drives`
- On page refresh: if cached drives exist and no active rips, skips API call
- Manual refresh button (↻) forces re-fetch

**Drive card display:**
- Shows drive name, disc name, state badge
- If disc name is parseable: shows derived name with arrow (e.g., "Inception DVD → Inception (2010)")
- Clicking disc name opens media naming modal
- Shows rip progress bar if ripping
- "Rip" button when Ready (disabled if no output dir)
- "Cancel" button when Ripping
- "Eject" button when Ready with disc
- Autorip toggle checkbox

**Drive reordering:**
- Draggable cards with handle (⠿)
- Order persisted in `localStorage` under `riply:driveOrder`
- Reset button restores backend order

---

## Where This Lives

| Concern | Layer |
|---|---|
| Drive detection | `infra/src/makemkv/backend.rs` |
| Disc name parsing | `infra/src/makemkv/parser.rs` |
| Drive state enum | `core/src/ripping.rs` |
| Drive API endpoints | `web/src/routes/ripper.rs` |
| Autorip loop | `web/src/routes/ripper.rs` |
| Rip event processing | `web/src/routes/ripper.rs` |
| SSE broadcasting | `web/src/sse.rs` |
| Frontend display | `web/svelte/src/routes/+page.svelte` |
| Frontend API client | `web/svelte/src/lib/api/ripper.ts` |
| Frontend SSE client | `web/svelte/src/lib/api/sse.ts` |

---

## Notes

- **In-memory state:** `active_rips` and `disc_names` are session-scoped (lost on restart)
- **No DB:** Drive state is ephemeral — always re-detected on demand
- **Single rip constraint:** Only one rip at a time to prevent drive contention
- **Detection pauses during rips:** autorip_poll_loop skips detection while rips are active
- **Eject is physical:** Uses platform-specific disc ejection (Linux `cdrom.h`, macOS `eject`, Windows `mciSendString`)
