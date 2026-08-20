# Design: SSE (Server-Sent Events) Manager

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Broadcast real-time updates to all connected web clients. Used for job progress, rip progress, drive state changes, and scan/review notifications. This document describes the *current* behavior of the SSE manager.

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

### `send_status(status: &str)`

Broadcast a status update event.

**Format:** `event:status\ndata:{status}`

**Examples:**
- `"drive:inserted:0"`
- `"drive:ejected:0"`
- `"drive:removed:0"`
- `"scan:starting:1"`
- `"scan:complete:1"`
- `"rip:progress:0:50:120"`
- `"rip:complete:0"`

### `send_progress(message: &str)`

Broadcast a progress update.

**Format:** `event:progress\ndata:{message}`

### `send_log(level: &str, message: &str)`

Broadcast a log message.

**Format:** `event:log\ndata:{level}:{message}`

---

## SSE Event Format

### Raw Message Format

Messages are formatted as:
```
event:{name}\ndata:{payload}
```

Or simply:
```
data:{payload}
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

**Example:**
```
Input:  "event:status\ndata:scan:complete:1"
Output: Event { event: "status", data: "scan:complete:1" }
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
   data:scan:complete:1

   event:status
   data:rip:progress:0:50:120

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

| Event | Format | Triggered By |
|---|---|---|
| `scan:starting` | `scan:starting:{job_id}` | Scan job starts |
| `scan:progress` | `scan:progress:{job_id}:{processed}:{total}` | Scan progress |
| `scan:complete` | `scan:complete:{job_id}` | Scan succeeds |
| `scan:cancelled` | `scan:cancelled:{job_id}` | Scan cancelled |
| `scan:failed` | `scan:failed:{job_id}:{error}` | Scan fails |

Same pattern for `analyze` and `move`.

### Rip Events

| Event | Format | Triggered By |
|---|---|---|
| `rip:starting` | `rip:starting:{drive}` | Rip starts |
| `rip:progress` | `rip:progress:{drive}:{pct}` | Progress update |
| `rip:progress` | `rip:progress:{drive}:{pct}:{eta}` | Progress with ETA |
| `rip:complete` | `rip:complete:{drive}` | Rip succeeds |
| `rip:error` | `rip:error:{drive}:{msg}` | Rip fails |
| `rip:message` | `rip:message:{drive}:{msg}` | Status message |
| `rip:cancelled` | `rip:cancelled:{drive}` | Rip cancelled |

### Drive Events

| Event | Format | Triggered By |
|---|---|---|
| `drive:inserted` | `drive:inserted:{index}` | Disc inserted |
| `drive:ejected` | `drive:ejected:{index}` | Disc ejected |
| `drive:removed` | `drive:removed:{index}` | Drive disappeared |
| `drive:changed` | `drive:changed:{index}` | State changed (legacy) |

### Review/Scan Events

| Event | Format | Triggered By |
|---|---|---|
| `scan:complete` | `scan:complete` | Scan finishes |
| `scan:cancelled` | `scan:cancelled` | Scan cancelled |
| `review:accepted` | `review:accepted` | File accepted in review |
| `review:rejected` | `review:rejected` | File rejected in review |

---

## Frontend SSE Client

### `web/svelte/src/lib/api/sse.ts`

**Connection:**
```typescript
const eventSource = new EventSource('/v1/sse');
```

**Event handling:**
```typescript
eventSource.addEventListener('status', (event) => {
    const data = event.data;
    // Parse event: "kind:action:detail"
    const [kind, action, ...rest] = data.split(':');
    // Route to appropriate handler
    handleSSEEvent(kind, action, rest.join(':'));
});
```

**Event dispatch:**

| Kind | Action | Handler |
|---|---|---|
| `rip` | `starting` | Show rip progress bar |
| `rip` | `progress` | Update progress bar |
| `rip` | `progress` (with ETA) | Update progress bar + ETA |
| `rip` | `complete` | Show success toast, trigger auto-scan |
| `rip` | `error` | Show error toast |
| `rip` | `cancelled` | Hide rip progress bar |
| `rip` | `message` | Update rip stage text |
| `drive` | `inserted` | Re-fetch drives |
| `drive` | `ejected` | Re-fetch drives |
| `drive` | `removed` | Re-fetch drives |
| `drive` | `changed` | Re-fetch drives |
| `scan` | `starting` | Add job to list |
| `scan` | `progress` | Update job progress |
| `scan` | `complete` | Set job status to Complete |
| `scan` | `cancelled` | Set job status to Cancelled |
| `scan` | `failed` | Set job status to Failed |
| `analyze` | `starting` | Add job to list |
| `analyze` | `progress` | Update job progress |
| `analyze` | `complete` | Set job status to Complete |
| `analyze` | `cancelled` | Set job status to Cancelled |
| `analyze` | `failed` | Set job status to Failed |
| `move` | `starting` | Add job to list |
| `move` | `progress` | Update job progress |
| `move` | `complete` | Set job status to Complete |
| `move` | `cancelled` | Set job status to Cancelled |
| `move` | `failed` | Set job status to Failed |
| `review` | `accepted` | Refresh review queue + stats |
| `review` | `rejected` | Refresh review queue + stats |

---

## Where This Lives

| Concern | File |
|---|---|
| SseManager | `web/src/sse.rs` |
| SSE endpoint | `web/src/routes/sse_handler.rs` |
| SSE client | `web/svelte/src/lib/api/sse.ts` |
| SSE event handling | `web/svelte/src/routes/+page.svelte` |
| Rip SSE events | `web/src/routes/ripper.rs` |
| Job SSE events | `app/src/jobs.rs` |

---

## Notes

- **Broadcast channel** — All connected clients receive all events
- **Capacity 256** — Oldest messages dropped if channel is full
- **Event/data format** — Follows SSE specification
- **Reconnection** — Client reconnects automatically on disconnect
- **No acknowledgment** — Events are fire-and-forget
- **SSE excluded from timeout** — Long-lived stream must not be cut by 30s timeout
- **Status event type** — Most events use `event:status` type
- **Event parsing** — Client splits on `:` to extract kind, action, detail
