# Feature: Drive Management Improvements

## Purpose
Improve the drive card experience by eliminating unnecessary polling, enabling real-time drive state updates, adding eject controls, and allowing users to reorder drives visually.

---

## Current State

| Concern | Current Behavior |
|---|---|
| **Drive loading** | `fetchDrives()` called once on `onMount()`. No polling, no SSE updates for drive changes. |
| **Disc insertion** | User must refresh the page, then wait for the next poll cycle. |
| **Eject** | Backend endpoint exists at `POST /v1/ripper/:index/eject` but no UI button. |
| **Reordering** | Drives are returned in index order from the backend. No UI reordering. |
| **SSE** | Used for rip progress, job events, review events. `drive:changed` events exist but just call `fetchDrives()`. |

---

## 1. Smart Polling

### Problem
On page refresh, drives are re-fetched even if nothing has changed. This is unnecessary and slow.

### Behavior

| Scenario | Action |
|---|---|
| **First load** | `fetchDrives()` on mount (unchanged) |
| **Page refresh** | If drives are already loaded and no rip is in progress, **skip the poll** — reuse in-memory state |
| **Rip completes** | Poll drives to update state (e.g., disc ejected, state changed to `Ready`) |
| **SSE `drive:changed`** | Update the affected drive card in-place (see §2) — no full re-fetch needed |
| **Manual refresh** | User can click a "refresh" button to force a re-poll |

### Implementation

**Backend:**
- No changes needed. The `listDrives()` endpoint is already fast — the optimization is frontend-only.

**Frontend:**
- Add `let lastDrives: DriveInfo[] = []` to track previously loaded drives
- Before calling `fetchDrives()`, compare new drives against `lastDrives`:
  - If identical (same count, same indices, same states), skip DOM update
  - If different, update `drives` and save to `lastDrives`
- On page refresh, if `drives` is non-empty and no rip is active, skip the initial `fetchDrives()` call — show cached state immediately
- Add a "↻" refresh button next to the "Drives" panel header that forces a re-poll

---

## 2. Live Drive State Updates via SSE

### Problem
When a disc is inserted, the drive card doesn't update until the user refreshes the page.

### Behavior

| Event | Backend Action | Frontend Action |
|---|---|---|
| **Disc inserted** | `autorip_poll_loop` detects new drive → broadcasts `drive:inserted:{index}` via SSE | Parse event → update `drives` array in-place → show drive card |
| **Disc ejected** | `autorip_poll_loop` detects ejected drive → broadcasts `drive:ejected:{index}` via SSE | Parse event → update `drives` array in-place → show "No disc" |
| **Rip completes** | Already emits `drive:changed` via SSE | Parse event → call `fetchDrives()` (full refresh needed to get updated state) |
| **Drive removed** | `autorip_poll_loop` detects removed drive → broadcasts `drive:removed:{index}` via SSE | Parse event → remove drive from `drives` array |

### SSE Event Format

Reuse the existing `kind:action:detail` format:

```
drive:inserted:{index}
drive:ejected:{index}
drive:removed:{index}
drive:changed:{index}   // existing — full state change, requires re-fetch
```

### Implementation

**Backend (`infra/src/autorip.rs` or wherever `autorip_poll_loop` lives):**
- After detecting a disc insertion/ejection/removal, broadcast the event via `SseManager`:
  ```rust
  sse.send_status(&format!("drive:inserted:{}", drive.index));
  ```
- The poll loop already calls `detect_drives()` — capture the delta (new/removed/ejected drives) and broadcast events

**Frontend (`web/svelte/src/routes/+page.svelte`):**
- Extend `handleSSEEvent()` to handle `drive:inserted`, `drive:ejected`, `drive:removed`:
  ```typescript
  if (kind === 'drive') {
    const index = parseInt(detail, 10);
    if (action === 'inserted' || action === 'ejected') {
      // Re-fetch only the affected drive (or do a full fetch for simplicity)
      fetchDrives();
    } else if (action === 'removed') {
      drives = drives.filter(d => d.index !== index);
    }
    return;
  }
  ```

**Optimization:** Instead of a full `fetchDrives()` on insert/eject, we could add a `GET /v1/ripper/drive/{index}` endpoint to fetch a single drive. But for now, a full fetch is acceptable since drive detection is fast.

---

## 3. Eject Button Per Drive

### Problem
The eject endpoint exists but has no UI affordance.

### Behavior

| State | Button Display |
|---|---|
| **Disc inserted (`Ready`)** | Show eject button (🗑️ or "Eject" icon) |
| **Ripping** | Hide eject button (can't eject during rip) |
| **No disc** | Hide eject button |

### Implementation

**Frontend (`web/svelte/src/routes/+page.svelte`):**
- Add an `eject(index)` handler:
  ```typescript
  async function handleEject(index: number) {
    try {
      await ripperApi.eject(index);
      showToast('Disc ejected', 'success');
      fetchDrives(); // Refresh to update drive state
    } catch (e) {
      showToast('Failed to eject', 'error');
    }
  }
  ```
- Add API method in `web/svelte/src/lib/api/ripper.ts`:
  ```typescript
  eject(index: number) {
    return request(`/v1/ripper/${index}/eject`, { method: 'POST' });
  }
  ```
- Add button in the drive card template (in the `drive-card__actions` section):
  ```svelte
  {#if drive.state === 'Ready' && drive.disc_name}
    <button
      class="btn btn--secondary btn--sm"
      onclick={() => handleEject(drive.index)}
      title="Eject disc"
    >
      Eject
    </button>
  {/if}
  ```

**Backend:** No changes needed — `POST /v1/ripper/:index/eject` already exists.

---

## 4. Drag-and-Drop Drive Reordering

### Problem
Users with multiple drives have no way to reorder them to match their physical setup.

### Behavior

| Feature | Detail |
|---|---|
| **Drag handle** | Each drive card gets a drag handle (⠿ or ⋮⋮) on the left side |
| **Drag & drop** | User drags a drive card to a new position within the list |
| **Persistence** | Order is stored in `localStorage` under `riply:driveOrder` as an array of indices |
| **Visual feedback** | Dragged card highlights; drop target shows a dashed border |
| **Reset** | A "Reset order" button in the drives panel header restores default (backend) order |
| **Server restart** | Order persists across server restarts (stored in browser localStorage) |

### Implementation

**Frontend (`web/svelte/src/routes/+page.svelte`):**
- Add state for drag-and-drop:
  ```typescript
  let driveOrder = $state<number[]>(() => {
    const saved = localStorage.getItem('riply:driveOrder');
    return saved ? JSON.parse(saved) : [];
  });

  let dragIndex: number | null = $state(null);
  let dropIndex: number | null = $state(null);
  ```

- Add a `saveDriveOrder()` function:
  ```typescript
  function saveDriveOrder() {
    localStorage.setItem('riply:driveOrder', JSON.stringify(driveOrder));
  }
  ```

- Add drag handlers to the drive card:
  ```svelte
  <div
    class="drive-card"
    draggable="true"
    ondragstart={() => { dragIndex = drive.index; }}
    ondragover={(e) => { e.preventDefault(); dropIndex = drive.index; }}
    ondragend={() => {
      if (dragIndex !== null && dropIndex !== null && dragIndex !== dropIndex) {
        // Reorder the drives array
        const reordered = [...drives];
        const [dragged] = reordered.splice(dragIndex, 1);
        reordered.splice(dropIndex, 0, dragged);
        drives = reordered;
        driveOrder = drives.map(d => d.index);
        saveDriveOrder();
      }
      dragIndex = null;
      dropIndex = null;
    }}
  >
  ```

- Add a drag handle icon at the start of each drive card:
  ```svelte
  <div class="drive-card__drag-handle" title="Drag to reorder">⠿</div>
  ```

- Add CSS for drag-and-drop visual feedback:
  ```css
  .drive-card[draggable="true"] {
    cursor: grab;
  }
  .drive-card[draggable="true"]:active {
    cursor: grabbing;
  }
  .drive-card--drag-over {
    border: 2px dashed var(--color-primary);
  }
  .drive-card__drag-handle {
    cursor: grab;
    color: var(--color-muted);
    user-select: none;
  }
  ```

- Add a "Reset order" button:
  ```svelte
  <button class="btn btn--ghost btn--xs" onclick={() => {
    driveOrder = [];
    localStorage.removeItem('riply:driveOrder');
    // Re-sort drives by index
    drives = [...drives].sort((a, b) => a.index - b.index);
  }}>Reset order</button>
  ```

**Backend:** No changes needed. Drive order is a client-side concern.

---

## Acceptance Criteria

- [ ] AC1: On page refresh with no active rips, drives are loaded from cache (no API call)
- [ ] AC2: A manual "refresh" button forces a drive re-poll
- [ ] AC3: When a disc is inserted, the drive card updates via SSE without a page refresh
- [ ] AC4: When a disc is ejected (via auto-detection or eject button), the drive card updates via SSE
- [ ] AC5: An eject button appears on drive cards when a disc is inserted and the drive is ready
- [ ] AC6: Clicking eject calls the backend endpoint and updates the drive card
- [ ] AC7: Drive cards are draggable within the sidebar panel
- [ ] AC8: Dragged drive order is persisted in localStorage
- [ ] AC9: A "Reset order" button restores the default (backend) drive order
- [ ] AC10: `cargo clippy --all-targets -- -D warnings` passes with 0 warnings
- [ ] AC11: `cargo test` passes with no failures

---

## Files Likely Affected

### Backend (Rust)

| File | What changes |
|---|---|
| `infra/src/autorip.rs` (or equivalent) | Broadcast `drive:inserted`, `drive:ejected`, `drive:removed` SSE events in the poll loop |
| `web/src/sse.rs` | No changes — existing `send_status()` is sufficient |

### Frontend (TypeScript/Svelte)

| File | What changes |
|---|---|
| `web/svelte/src/routes/+page.svelte` | Smart polling logic, SSE event handlers for drive state, eject handler, drag-and-drop |
| `web/svelte/src/lib/api/ripper.ts` | Add `eject(index)` method |
| `web/svelte/src/routes/+page.svelte` (CSS) | Add drag-and-drop visual styles |

---

## Where This Lives

| Concern | Layer |
|---|---|
| Smart polling | `web/svelte/src/routes/+page.svelte` (frontend cache) |
| Live drive updates | `infra/src/autorip.rs` (backend SSE broadcast) + `web/svelte/src/routes/+page.svelte` (frontend SSE handler) |
| Eject button | `web/svelte/src/lib/api/ripper.ts` (API method) + `web/svelte/src/routes/+page.svelte` (UI) |
| Drag-and-drop | `web/svelte/src/routes/+page.svelte` (frontend only, localStorage) |

---

## Notes

- **Smart polling is a UX optimization:** The backend `listDrives()` is already fast, but eliminating redundant API calls reduces latency and server load.
- **SSE is the right transport for drive state:** The SSE connection is already open and used for rip progress. Broadcasting drive events over the same channel is efficient.
- **Eject is a simple wire-up:** The backend endpoint already exists — this is primarily a frontend task.
- **Drag-and-drop is client-side only:** No backend persistence needed. localStorage is sufficient for a single-user app.
- **Drive order doesn't affect functionality:** It's purely cosmetic — the backend always returns drives in index order. The frontend reorders the display.
- **Existing `drive:changed` SSE event:** Currently handled by calling `fetchDrives()`. This spec extends the SSE system with more granular events (`inserted`, `ejected`, `removed`) for in-place updates.
