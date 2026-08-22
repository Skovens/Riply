# Design: SSE (Server-Sent Events) Manager

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Broadcast real-time updates to all connected web clients. Used for job progress, rip progress, drive state changes, and scan/review notifications.

---

## SseManager

### Structure

```rust
pub struct SseManager {
    tx: broadcast::Sender<String>,
}
```

- Uses `tokio::sync::broadcast` channel for fan-out to multiple subscribers
- Channel capacity: configurable (default 256)
- Old messages are dropped if the channel is full

### Construction

```rust
pub fn new(capacity: usize) -> Self
```

Creates a broadcast channel with the given capacity.

### Subscription

```rust
pub fn subscribe(&self) -> broadcast::Receiver<String>
```

Returns a new receiver for each SSE connection.

---

## Broadcasting Methods

### `send_json(event: Value)`

Broadcast a structured JSON event. This is the primary method.

**Event envelope format:**
```json
{
  "kind": "rip",
  "action": "progress",
  "index": 0,
  "data": { "pct": 50, "eta": 120, "stage": "Ripping title 1" }
}
```

### `send_status(status: &str)` (legacy)

Kept for backward compatibility. Broadcasts in the old colon-delimited format.

**Format:** `event:status\ndata:{status}`

### `send_progress(message: &str)` (legacy)

Kept for backward compatibility.

### `send_log(level: &str, message: &str)` (legacy)

Kept for backward compatibility.

---

## SSE Event Format

### JSON Envelope (primary)

All events use this structure:

```json
{
  "kind": "<event-domain>",
  "action": "<event-action>",
  "index": <drive-or-job-index>,
  "data": { <action-specific-fields> }
}
```

### Wire Format

The JSON string is sent as the SSE data payload:
```
event:status
data:{"kind":"rip","action":"progress","index":0,"data":{"pct":50,"eta":120,"stage":"Ripping title 1"}}
```

### Conversion to SSE Event

```rust
pub fn message_to_event(msg: &str) -> Event
```

**Parsing:**
1. If message contains `\n`:
   - Split on first `\n`
   - If header starts with `event:`: extract event name
   - If body starts with `data:`: strip prefix
   - Create SSE event with the extracted name and data
2. If message starts with `data:`: strip prefix, use as data
3. Otherwise: use entire message as data

**Example (legacy):**
```
Input:  "event:status\ndata:scan:complete:1"
Output: Event { event: "status", data: "scan:complete:1" }
```

**Example (JSON):**
```
Input:  {"kind":"scan","action":"complete","index":0,"data":{"job_id":"scan"}}
Output: Event { event: "status", data: '{"kind":"scan","action":"complete","index":0,"data":{"job_id":"scan"}}' }
```

---

## SSE Endpoint

### `GET /v1/sse`

**Handler:** `sse_handler::router()`

**Behavior:**
1. Creates a new subscription via `sse.subscribe()`
2. Streams events as SSE format:
   ```
   event:status
   data:{"kind":"rip","action":"progress","index":0,"data":{"pct":50,"eta":120,"stage":"Ripping title 1"}}

   event:status
   data:{"kind":"rip","action":"complete","index":0,"data":{"dest":"/path/to/movie"}}

   ...
   ```
3. Client reconnects on disconnect (standard SSE behavior)
4. If client falls behind, oldest messages are dropped (broadcast channel semantics)

**Why SSE is excluded from the 30s timeout layer:**
- SSE is a long-lived stream
- The 30s `TimeoutLayer` applies only to the API sub-router
- SSE handler is on a separate router merge

---

## SSE Event Taxonomy

### Job Events

| Event | data fields | Triggered By |
|---|---|---|
| `scan:starting` | `{ "total": N, "job_id": "scan" }` | Scan job starts |
| `scan:progress` | `{ "job_id": "scan", "processed": N, "total": N }` | Scan progress |
| `scan:complete` | `{ "job_id": "scan" }` | Scan succeeds |
| `scan:cancelled` | `{ "job_id": "scan" }` | Scan cancelled |
| `scan:failed` | `{ "job_id": "scan", "message": "error text" }` | Scan fails |

Same pattern for `analyze` and `move`.

### Rip Events

| Event | data fields | Triggered By |
|---|---|---|
| `rip:starting` | `{}` | Rip starts |
| `rip:progress` | `{ "pct": N, "eta": N \| null, "stage": "text" }` | Progress update |
| `rip:complete` | `{ "dest": "/path/to/output" }` | Rip succeeds |
| `rip:error` | `{ "message": "error text" }` | Rip fails |
| `rip:message` | `{ "text": "MakeMKV status message" }` | Status message |
| `rip:cancelled` | `{}` | Rip cancelled |
| `rip:named` | `{ "title": "Movie", "year": 2010, "type": "movie" }` | Auto-naming succeeded |
| `rip:naming_failed` | `{}` | Auto-naming failed |

### Drive Events

| Event | data fields | Triggered By |
|---|---|---|
| `drive:inserted` | `{}` | Disc inserted |
| `drive:ejected` | `{}` | Disc ejected |
| `drive:removed` | `{}` | Drive disappeared |

### Review Events

| Event | data fields | Triggered By |
|---|---|---|
| `review:accepted` | `{ "id": "file-id" }` | File accepted in review |
| `review:rejected` | `{ "id": "file-id" }` | File rejected in review |

### Auto-rip Events

| Event | data fields | Triggered By |
|---|---|---|
| `autorip:ejected` | `{}` | Auto-rip ejected disc |

---

## Frontend SSE Client

### `web/svelte/src/lib/api/sse.ts`

**Connection:**
```typescript
const eventSource = new EventSource('/v1/sse');
```

**Event handling (JSON format):**
```typescript
eventSource.addEventListener('status', (event: MessageEvent) => {
    const parsed = JSON.parse(event.data) as SseEvent;
    onMessage(parsed);
});
```

**SseEvent type:**
```typescript
type SseEvent = {
  kind: string;
  action: string;
  index: number;
  data: Record<string, unknown>;
};
```

**Event dispatch (type guards):**
```typescript
if (isRipProgress(event)) {
    const { pct, eta, stage } = ripProgressFields(event);
    // Update progress bar
}
if (isRipNamed(event)) {
    const { title, year } = ripNamedInfo(event);
    // Show auto-naming result
}
```

---

## Where This Lives

| Concern | File |
|---|---|
| SseManager | `web/src/sse.rs` |
| SSE endpoint | `web/src/routes/sse_handler.rs` |
| SSE client | `web/svelte/src/lib/api/sse.ts` |
| Event helpers | `web/svelte/src/lib/api/parse-event.ts` |
| Rip SSE events | `web/src/routes/ripper.rs` |
| Job SSE events | `web/src/routes/scan.rs`, `web/src/routes/analyze.rs`, `web/src/routes/move_.rs` |
| Drive SSE events | `web/src/routes/ripper.rs` |
| Review SSE events | `web/src/routes/review.rs` |

---

## Notes

- **Broadcast channel** — All connected clients receive all events
- **Capacity 256** — Oldest messages dropped if channel is full
- **JSON format** — Structured events with `kind`, `action`, `index`, `data` fields
- **Reconnection** — Client reconnects automatically on disconnect
- **No acknowledgment** — Events are fire-and-forget
- **SSE excluded from timeout** — Long-lived stream must not be cut by 30s timeout
- **Status event type** — All events use `event:status` type
- **Title-safe** — JSON format handles colons and special characters in text fields
- **Legacy support** — `send_status`, `send_progress`, `send_log` kept for backward compat
