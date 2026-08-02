# ADR-011: Real-Time Web Updates

## Status

Accepted

## Context

The Riply web dashboard needs to display real-time updates:
- Job progress bars updating live
- New files appearing in the file list
- Review queue changes
- Ripper status (drives reading/ripping)
- System events (errors, completions)

Options considered:
1. **Server-Sent Events (SSE)** — Simple, HTTP-native, one-way server→client
2. **WebSockets** — Full duplex, more complex, overkill for Riply's needs
3. **Long polling** — Simple but inefficient, high latency
4. **GraphQL subscriptions** — Powerful but adds GraphQL infrastructure

## Decision

Use **Server-Sent Events (SSE)** for real-time updates.

Key design:
- `/api/sse` endpoint streams events to connected clients
- `SseManager` in `web` crate manages subscriber lifecycle
- Events are typed: `event:status`, `event:progress`, `event:log`
- Frontend uses `EventSource` API to receive events
- SSE connection is managed by `connectSSE()` in frontend `api.js`
- Automatic cleanup via unsubscribe function
- Polling fallback for components that need frequent updates (jobs: 2s, ripper: 3s)

## Consequences

### Positive
- Simple implementation — single HTTP endpoint
- Native browser support — no libraries needed
- Automatic reconnection on connection loss
- One-way communication is sufficient (client sends requests via fetch)
- Lower overhead than WebSockets

### Negative
- One-way only (server→client) — client must use separate fetch calls for requests
- No built-in message acknowledgment (lost events on disconnect)
- Browser limitation: max 6 concurrent connections per origin (not an issue for Riply)
- SSE connection is lost on page navigation (re-established by frontend)

### Future Considerations
If Riply needs bidirectional communication (e.g., remote collaboration, admin controls), consider migrating to WebSockets. The current `SseManager` abstraction makes this transition feasible.
