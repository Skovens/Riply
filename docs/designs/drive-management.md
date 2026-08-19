# Design: Drive Management

## Purpose
Manage drive cards in the Rip window. Drives are polled on events (not constantly), updated via SSE, and provide eject controls.

## Reference
ADR-025 (Three-Window Architecture) — Rip window drive cards.

---

## Drive Card States

| State | Display |
|-------|---------|
| `Ready` | "Insert disc to rip" + Rip button active |
| `Ripping` | Progress bar + "Ripping..." + ETA |
| `Complete` | "Ripping successful" + disc cover + auto-detected name + type |
| `Error` | "Ripping failed: [reason]" + descriptive error |

---

## Drive Card Buttons

Buttons never disappear. Inactive buttons are greyed out with a tooltip.

| Button | Active | Inactive (greyed out) |
|--------|--------|----------------------|
| Rip | Disc inserted, not ripping | No disc → "No disc in drive" |
| Eject | Disc inserted, not ripping | No disc → "No disc to eject" |
| View in Organizer | Unidentified rip | Identified rip → "Already named" |

---

## Drive Polling

No constant polling. Drives are polled:
- On page load
- When SSE `drive:changed` event arrives
- When user clicks refresh button
- After rip completes (to check for ejected discs)

### SSE Drive Events

| Event | Format | When |
|-------|--------|------|
| `drive:inserted` | `drive:inserted:{index}` | Disc inserted |
| `drive:ejected` | `drive:ejected:{index}` | Disc ejected |
| `drive:removed` | `drive:removed:{index}` | Drive removed |
| `drive:changed` | `drive:changed:{index}` | Full state change |

### Implementation

**Backend (`infra/src/autorip.rs`):**
- After detecting a disc insertion/ejection/removal, broadcast the event via `SseManager`
- The poll loop already calls `detect_drives()` — capture the delta and broadcast events

**Frontend (`web/svelte/src/routes/+page.svelte`):**
- Extend `handleSSEEvent()` to handle `drive:inserted`, `drive:ejected`, `drive:removed`

---

## Eject Button

**Frontend:**
- Add an `eject(index)` handler
- Add API method in `web/svelte/src/lib/api/ripper.ts`
- Add button in the drive card template (always visible, greyed out when inactive)

**Backend:** No changes needed — `POST /v1/ripper/:index/eject` already exists.

---

## Acceptance Criteria

- [ ] AC1: Drive cards update via SSE without page refresh
- [ ] AC2: An eject button is always visible (greyed out when inactive)
- [ ] AC3: Hovering over an inactive button shows a tooltip explaining why
- [ ] AC4: Clicking eject calls the backend endpoint and updates the drive card
- [ ] AC5: `cargo clippy --all-targets -- -D warnings` passes with 0 warnings
- [ ] AC6: `cargo test` passes with no failures

---

## Where This Lives

| Concern | Layer |
|---|---|
| Drive state | `infra/src/autorip.rs` (backend SSE broadcast) |
| Drive card UI | `web/svelte/src/routes/+page.svelte` |
| Eject API | `web/svelte/src/lib/api/ripper.ts` |
