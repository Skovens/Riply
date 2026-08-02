# ADR-010: Background Job System

## Status

Accepted

## Context

Riply operations (scanning, analyzing, moving, ripping) can take minutes to hours. The application must:
- Run long operations in the background
- Track job progress and status
- Allow job cancellation
- Support multiple concurrent jobs
- Persist job state across server restarts

Options considered:
1. **Custom job scheduler** — Full control, tailored to Riply's needs
2. **Async-channel based** — Simple, but no persistence
3. **External queue (Redis/Bull)** — Robust, but adds infrastructure dependency

## Decision

Use a **custom in-memory job scheduler** with SQLite persistence.

Key design:
- `JobScheduler` service in `app` crate manages job lifecycle
- Each job has: `JobInfo` (metadata), `JobStatus` (enum), `ProgressHandle`
- Jobs are stored in SQLite for persistence across restarts
- Cancellation via `tokio::sync::watch` channel (sender/receiver pattern)
- Progress reported through the same channel
- Background polling by web frontend (jobs: 2s, ripper: 3s)

## Consequences

### Positive
- No external dependencies (no Redis, no message queue)
- Simple architecture — single service manages everything
- SQLite persistence ensures jobs survive restarts
- Cancellation is cooperative (jobs check the watch channel)
- Progress tracking is built-in via `ProgressHandle`

### Negative
- Jobs are lost if the process crashes (no dead letter queue)
- No job prioritization or scheduling windows
- Memory usage grows with active jobs (bounded by design)
- Polling-based UI updates (not event-driven)

### Future Considerations
If Riply needs:
- Distributed job processing → migrate to Redis + async queue
- Job scheduling (run at midnight) → add cron-like scheduler
- Retry logic for failed jobs → add retry queue
