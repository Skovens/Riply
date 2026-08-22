# SSE Protocol Refactor Plan

## Overview

Replace the colon-delimited SSE event format (`"kind:action:idx:detail"`) with structured JSON.
This fixes bugs that exist in the current parsing logic and makes the protocol extensible without
index-shifting.

## Current Format (broken)

```
rip:progress:0:50:120      → kind=rip, action=progress, detail=0:50:120
rip:eta:0:120              → sent as separate event (NEVER ACTUALLY FIRES)
rip:named:0:Inception (2010) → frontend listens for "naming_complete" (WRONG NAME)
rip:message:0:Ripping title 1 → detail.split(':')[0] reads "Ripping" not "0"
```

### Bugs in current logic
1. **ETA never shows** — Backend sends ETA inside `rip:progress:0:50:120` but frontend looks for a separate `rip:eta:0:120` event that never fires.
2. **Auto-naming never shows** — Backend sends `rip:named:0:Title (Year)` but frontend listens for `rip:naming_complete` (wrong event name).
3. **Message text after colon drops** — `detail.split(':')[0]` reads the wrong segment if title contains `:`.
4. **Title with colon breaks parsing** — Same colon-fragility.

## Target Format (JSON)

### Event envelope (all events)

```json
{
  "kind": "rip",
  "action": "progress",
  "index": 0,
  "data": { ... }
}
```

### Per-action data shapes

| Action | data fields |
|--------|-------------|
| `starting` | `{}` |
| `progress` | `{ pct: number, eta: number | null, stage: string }` |
| `complete` | `{ dest: string }` |
| `error` | `{ message: string }` |
| `cancelled` | `{}` |
| `message` | `{ text: string }` |
| `named` | `{ title: string, year: number, type: "movie" | "tv" }` |
| `naming_failed` | `{}` |
| `drive:inserted` | `{}` |
| `drive:ejected` | `{}` |
| `drive:removed` | `{}` |
| `scan:starting` | `{ job_id: string }` |
| `scan:progress` | `{ job_id: string, processed: number, total: number }` |
| `scan:complete` | `{ job_id: string }` |
| `scan:cancelled` | `{ job_id: string }` |
| `scan:failed` | `{ job_id: string, message: string }` |
| `analyze:starting` | `{ job_id: string }` |
| `analyze:progress` | `{ job_id: string, processed: number, total: number }` |
| `analyze:complete` | `{ job_id: string }` |
| `analyze:cancelled` | `{ job_id: string }` |
| `analyze:failed` | `{ job_id: string, message: string }` |
| `move:starting` | `{ job_id: string }` |
| `move:progress` | `{ job_id: string, processed: number, total: number }` |
| `move:complete` | `{ job_id: string }` |
| `move:cancelled` | `{ job_id: string }` |
| `move:failed` | `{ job_id: string, message: string }` |
| `review:accepted` | `{}` |
| `review:rejected` | `{}` |
| `connected` | `{}` |

### SSE wire format

```
event:status
data:{"kind":"rip","action":"progress","index":0,"data":{"pct":50,"eta":120,"stage":"Ripping title 1"}}

event:status
data:{"kind":"rip","action":"complete","index":0,"data":{"dest":"/path/to/movie"}}
```

## Implementation Order

### Phase 1: Backend — SseManager (core infrastructure)

**File:** `web/src/sse.rs`

1. Add `serde_json::Value` import
2. Change `send_status` to accept JSON string directly (no event/data prefix wrapping — the SSE handler adds that)
3. Add a new `send_json` method that wraps the value in the envelope:
   ```rust
   pub fn send_json(&self, event: serde_json::Value) {
       let _ = self.tx.send(event.to_string());
   }
   ```
4. Keep `send_status` for backward compat during transition (or remove it immediately — prefer immediate removal)

### Phase 2: Backend — Ripper events (~20 call sites)

**File:** `web/src/routes/ripper.rs`

Replace all `sse.send_status(&format!("rip:..."))` calls with `sse.send_json(json!({...}))`:

| Line | Old | New |
|------|-----|-----|
| 266 | `rip:progress:{idx}:{pct}:{eta}` | `{"kind":"rip","action":"progress","index":{idx},"data":{"pct":{pct},"eta":{eta},"stage":""}}` |
| 269 | `rip:progress:{idx}:{pct}` | Same with eta=null |
| 274 | `rip:complete:{idx}` | `{"kind":"rip","action":"complete","index":{idx},"data":{}}` |
| 278 | `rip:error:{idx}:{msg}` | `{"kind":"rip","action":"error","index":{idx},"data":{"message":"{msg}"}}` |
| 282 | `rip:message:{idx}:{msg}` | `{"kind":"rip","action":"message","index":{idx},"data":{"text":"{msg}"}}` |
| 409 | `rip:starting:{idx}` | `{"kind":"rip","action":"starting","index":{idx},"data":{}}` |
| 425 | `rip:named:{idx}:{display}` | `{"kind":"rip","action":"named","index":{idx},"data":{"title":"{title}","year":{year},"type":"movie"}}` |
| 427 | `rip:naming_failed:{idx}` | `{"kind":"rip","action":"naming_failed","index":{idx},"data":{}}` |
| 454 | `rip:complete:{idx}:{dest}` | `{"kind":"rip","action":"complete","index":{idx},"data":{"dest":"{dest}"}}` |
| 458 | `rip:message:{idx}:Move failed: {e}` | `{"kind":"rip","action":"message","index":{idx},"data":{"text":"Move failed: {e}"}}` |
| 465 | `rip:message:{idx}:Rip failed, cleaned up` | `{"kind":"rip","action":"message","index":{idx},"data":{"text":"Rip failed, cleaned up"}}` |
| 484 | `rip:cancelled:{idx}` | `{"kind":"rip","action":"cancelled","index":{idx},"data":{}}` |
| 592 | `drive:inserted:{idx}` | `{"kind":"drive","action":"inserted","index":{idx},"data":{}}` |
| 595 | `drive:ejected:{idx}` | `{"kind":"drive","action":"ejected","index":{idx},"data":{}}` |
| 606 | `drive:removed:{idx}` | `{"kind":"drive","action":"removed","index":{idx},"data":{}}` |
| 694 | `rip:starting:{idx}` | Same as above |
| 711 | `rip:named:{idx}:{display}` | Same as above |
| 713 | `rip:naming_failed:{idx}` | Same as above |
| 739 | `rip:complete:{idx}:{dest}` | Same as above |
| 743 | `rip:message:{idx}:Move failed: {e}` | Same as above |
| 751 | `rip:message:{idx}:Rip failed, cleaned up` | Same as above |
| 767 | `autorip:ejected:{idx}` | `{"kind":"autorip","action":"ejected","index":{idx},"data":{}}` |

### Phase 3: Backend — Organizer/Job events (~15 call sites)

**Files:** `web/src/routes/organizer.rs`, `web/src/routes/scan.rs`, `web/src/routes/jobs.rs`, `web/src/routes/move.rs`, `web/src/routes/review.rs`, `web/src/routes/config.rs`

Replace all `sse.send_status(...)` calls with `sse.send_json(json!({...}))` using the shapes from the table above.

### Phase 4: Backend — SSE handler

**File:** `web/src/routes/sse_handler.rs`

No changes needed — it already passes the raw message through `SseManager::message_to_event()`. The JSON string is the payload, and the SSE event type remains `status`.

### Phase 5: Frontend — SSE connection

**File:** `web/svelte/src/lib/api/sse.ts`

1. Change `connectSSE` to parse JSON from `event.data` instead of passing raw string
2. Parse the JSON envelope: `{ kind, action, index, data }`
3. Call `onMessage` with the parsed object instead of raw string

```ts
export function connectSSE(onMessage: (event: ParsedSSEEvent) => void) {
  const eventSource = new EventSource('/v1/sse');
  eventSource.addEventListener('status', (event: MessageEvent) => {
    try {
      const parsed = JSON.parse(event.data) as ParsedSSEEvent;
      onMessage(parsed);
      lastEvent.set(parsed);
    } catch (e) {
      console.error('SSE parse error:', e, event.data);
    }
  });
  // ... same for onmessage
}
```

### Phase 6: Frontend — parse-event.ts

**File:** `web/svelte/src/lib/api/parse-event.ts`

1. Update `ParsedSSEEvent` interface to match the JSON structure
2. Remove `parseEvent()` — no longer needed, parsing happens in `sse.ts`
3. Update `ripEventIndex()` to read from `event.index`
4. Update `ripProgressFields()` to read from `event.data`
5. Remove `ripMessageText()` — text is now in `event.data.text`
6. Add new helpers:
   ```ts
   export function isRipProgress(event: ParsedSSEEvent): boolean {
     return event.kind === 'rip' && event.action === 'progress';
   }
   export function isRipComplete(event: ParsedSSEEvent): boolean {
     return event.kind === 'rip' && event.action === 'complete';
   }
   export function isRipError(event: ParsedSSEEvent): boolean {
     return event.kind === 'rip' && event.action === 'error';
   }
   export function isRipMessage(event: ParsedSSEEvent): boolean {
     return event.kind === 'rip' && event.action === 'message';
   }
   export function isRipNamed(event: ParsedSSEEvent): boolean {
     return event.kind === 'rip' && event.action === 'named';
   }
   export function isRipNamingFailed(event: ParsedSSEEvent): boolean {
     return event.kind === 'rip' && event.action === 'naming_failed';
   }
   export function isDriveEvent(event: ParsedSSEEvent): boolean {
     return event.kind === 'drive';
   }
   export function isScanEvent(event: ParsedSSEEvent): boolean {
     return event.kind === 'scan';
   }
   export function isAnalyzeEvent(event: ParsedSSEEvent): boolean {
     return event.kind === 'analyze';
   }
   export function isMoveEvent(event: ParsedSSEEvent): boolean {
     return event.kind === 'move';
   }
   ```

### Phase 7: Frontend — ripper/+page.svelte

**File:** `web/svelte/src/routes/ripper/+page.svelte`

1. Update `handleSSEEvent` to accept `ParsedSSEEvent` object instead of raw string
2. Replace `data.split(':')` logic with direct field access:
   ```ts
   if (event.kind === 'rip' && event.action === 'progress') {
     const { pct, eta, stage } = event.data;
     ripProgress = { ...ripProgress, [event.index]: { pct, eta, stage } };
   }
   if (event.kind === 'rip' && event.action === 'starting') {
     // ...
   }
   if (event.kind === 'rip' && event.action === 'complete') {
     // ...
   }
   if (event.kind === 'rip' && event.action === 'error') {
     // ...
   }
   if (event.kind === 'rip' && event.action === 'named') {
     // Auto-naming succeeded
     ripNaming = { ...ripNaming, [event.index]: { discName: drive.disc_name || '', foundName: event.data.title + (event.data.year ? ` (${event.data.year})` : ''), status: 'complete' } };
   }
   if (event.kind === 'rip' && event.action === 'naming_failed') {
     // Auto-naming failed
     ripNaming = { ...ripNaming, [event.index]: { discName: drive.disc_name || '', foundName: null, status: 'failed' } };
   }
   if (event.kind === 'rip' && event.action === 'message') {
     // Status message (MakeMKV stage text)
     ripStatus = { ...ripStatus, [event.index]: event.data.text };
   }
   if (event.kind === 'drive' && event.action === 'inserted') {
     fetchDrives();
   }
   if (event.kind === 'drive' && event.action === 'ejected') {
     fetchDrives();
   }
   if (event.kind === 'drive' && event.action === 'removed') {
     fetchDrives();
   }
   ```
3. Add `ripStatus` state for MakeMKV stage text
4. Update template to show:
   - Progress bar with percentage
   - ETA (minutes until <45s, then seconds)
   - Status line (MakeMKV stage text)
   - Auto-naming status (Searching... → Found: Title (Year) / Not found)
   - Poster image (left side, gray silhouette placeholder)
5. Make card taller/wider for poster layout

### Phase 8: Frontend — +layout.svelte

**File:** `web/svelte/src/routes/+layout.svelte`

1. Update `handleSSEEvent` to accept `ParsedSSEvent` object
2. Replace `data.split(':')` with direct field access:
   ```ts
   if (event.action === 'starting' || event.action === 'progress') {
     if (event.kind === 'scan') activeJob = 'scan';
     else if (event.kind === 'analyze') activeJob = 'analyze';
     else if (event.kind === 'move') activeJob = 'move';
   }
   if (event.action === 'complete' || event.action === 'cancelled') {
     if (event.kind === 'scan' || event.kind === 'analyze') activeJob = null;
     if (event.kind === 'move') activeJob = null;
   }
   ```

### Phase 9: Frontend — vitest tests

**File:** `web/svelte/src/lib/api/sse.test.ts` (update existing)

1. Rewrite tests for JSON format
2. Test envelope parsing
3. Test per-action data extraction
4. Test edge cases (null eta, missing fields)

**File:** `web/svelte/src/lib/api/parse-event.test.ts` (update existing)

1. Update tests for new helper functions
2. Test type guards (isRipProgress, isRipComplete, etc.)

### Phase 10: Ripper UI improvements (on top of JSON refactor)

**File:** `web/svelte/src/routes/ripper/+page.svelte`

1. **Progress bar** — render when `ripProgress[idx]` exists, show percentage
2. **ETA display** — minutes until <45s, then seconds (no flapping)
3. **Status line** — show `ripStatus[idx]` (MakeMKV stage text from `rip:message` events)
4. **Auto-naming** — show "Searching..." while active, then "Found: Title (Year)" or "Not found"
5. **Poster image** — gray silhouette placeholder; show TMDb poster when available (need to fetch from TMDb API using tmdb_id from `rip:named` event)
6. **Card layout** — horizontal: poster on left (120px wide), info on right

### Phase 11: Update design docs

**Files to update:**
- `docs/designs/sse.md` — replace colon-format examples with JSON format
- `docs/designs/makemkv.md` — update SSE event format section
- `docs/PRODUCT.md` — update API endpoint docs if needed

## Testing Strategy

### Frontend (available)
- `npm test` (vitest) — unit tests for parse-event helpers, SSE parsing
- `npm run build` (SvelteKit) — compile check

### Backend (NOT available — needs Rust toolchain)
- `cargo test` — needs Rust toolchain
- `cargo build` — compile check
- Manual testing with real DVD drive (user will set up)

## Files Changed (summary)

### Backend (Rust)
- `web/src/sse.rs` — add `send_json` method
- `web/src/routes/ripper.rs` — ~22 `send_status` → `send_json`
- `web/src/routes/organizer.rs` — `send_status` → `send_json`
- `web/src/routes/scan.rs` — `send_status` → `send_json`
- `web/src/routes/move.rs` — `send_status` → `send_json`
- `web/src/routes/review.rs` — `send_status` → `send_json`
- `web/src/routes/config.rs` — `send_status` → `send_json`
- `web/src/routes/jobs.rs` — `send_status` → `send_json`

### Frontend (TypeScript/Svelte)
- `web/svelte/src/lib/api/sse.ts` — JSON parsing in `connectSSE`
- `web/svelte/src/lib/api/parse-event.ts` — new helpers, type guards
- `web/svelte/src/lib/api/parse-event.test.ts` — updated tests
- `web/svelte/src/lib/api/sse.test.ts` — updated tests
- `web/svelte/src/routes/ripper/+page.svelte` — JSON event handling + UI improvements
- `web/svelte/src/routes/+layout.svelte` — JSON event handling

### Documentation
- `docs/designs/sse.md` — JSON format examples
- `docs/designs/makemkv.md` — updated SSE event format
- `docs/PRODUCT.md` — API endpoint docs if needed

## Risks & Mitigations

1. **No Rust toolchain** — Can't compile-check backend changes. Mitigation: careful manual review, rely on frontend tests + user's hardware test rig.
2. **Blast radius** — ~58 call sites change simultaneously. Mitigation: all events switch at once (no backward compat layer), reducing partial-break risk.
3. **SSE reconnection** — If JSON format has a bug, all consumers break. Mitigation: add error handling in `sse.ts` to gracefully handle parse failures.
4. **MakeMKV message escaping** — JSON strings need proper escaping. Mitigation: use `serde_json::json!()` macro which handles escaping automatically.

## Completion Checklist

- [ ] Phase 1: Backend SseManager
- [ ] Phase 2: Backend Ripper events
- [ ] Phase 3: Backend Organizer/Job events
- [ ] Phase 4: Backend SSE handler (no changes needed)
- [ ] Phase 5: Frontend SSE connection
- [ ] Phase 6: Frontend parse-event.ts
- [ ] Phase 7: Frontend ripper/+page.svelte
- [ ] Phase 8: Frontend +layout.svelte
- [ ] Phase 9: Frontend vitest tests
- [ ] Phase 10: Ripper UI improvements
- [ ] Phase 11: Update design docs
- [ ] Frontend tests pass (vitest)
- [ ] Frontend builds (SvelteKit)
- [ ] AGENTS.md updated
- [ ] refactor.md deleted
- [ ] Commit and push
