# ADR-018: Graceful Shutdown

## Status

Accepted

## Context

`riply serve` runs the HTTP server plus background tasks (autorip
monitor, healthchecks heartbeat). A bare Ctrl+C previously dropped
everything: SSE clients disconnected mid-event, the autorip monitor
left a dangling poll loop, and the exit could take time or leave tasks
running.

## Decision

Use Axum's `with_graceful_shutdown` on the server in
`cli/src/commands/serve.rs`:

- `shutdown_signal()` waits on `tokio::signal::ctrl_c()`
- The autorip monitor task is aborted explicitly on shutdown
- Background jobs finish their current step and the process exits

## Consequences

- Ctrl+C exits promptly and cleanly (verified in tests).
- No orphaned background tasks on shutdown.
- SSE clients see the connection close as part of a normal shutdown.
