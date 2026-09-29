# ADR-029: Event-Driven Drive Detection

## Status

Accepted

## Date

2024

## Context

The Riply application needs to detect optical disc insertion and ejection events to enable AutoRip functionality. The initial approach was to continuously poll drives every 5 seconds using the autorip poll loop. This approach has several drawbacks:

1. **Unnecessary drive spinning**: Polling causes the drive to spin up repeatedly, wasting power and generating noise
2. **LED activity**: The drive LED flashes constantly during polling, confusing users
3. **Latency**: Disc insertion may not be detected for up to 5 seconds
4. **Resource waste**: Continuous polling consumes CPU and I/O resources even when no discs are present

## Decision

We will implement an event-driven drive detection system using a **Chain of Responsibility pattern** with multiple platform-specific adapters:

1. **udev** (Linux) — Primary source via libudev
2. **inotify on /sys/block/** (Linux) — Fallback via nix crate
3. **inotify on /dev/sr\*** (Linux) — Alternative inotify approach
4. **D-Bus** (Linux/Unix) — Desktop environment integration

The chain tries sources in priority order. The first available source is selected at startup and used for the lifetime of the server.

### Key Design Points

- **Adapter Pattern**: Each source implements `DriveEventSource` trait, converting native events to standardized `DriveEvent` enum
- **Chain of Responsibility**: Sources tried in priority order; first available wins
- **One-time selection**: Source selected at server startup, not per-request
- **SSE integration**: Events broadcast via Server-Sent Events for real-time UI updates
- **AutoRip integration**: Insertion events trigger AutoRip when enabled for that drive
- **Event source availability**: Frontend gates AutoRip toggle behind availability check
- **Fallback**: If no event source available, system continues with existing state; user can manually refresh

## Consequences

### Positive

- **No unnecessary drive spinning**: Drives only accessed on actual events
- **Instant detection**: Disc insertion detected within milliseconds
- **Lower resource usage**: No continuous polling overhead
- **Better UX**: AutoRip works seamlessly when event source available
- **Clear visibility**: Health endpoint and `/v1/ripper/event-source` report event source status
- **Graceful degradation**: System continues to work without event source (manual refresh only)

### Negative

- **Platform-specific**: Most sources are Linux-specific; Windows/macOS need platform-specific adapters
- **Added complexity**: New module (`infra/src/drive_events/`) with multiple adapters
- **Startup dependency**: Event source must be available at startup; if it dies, system continues but loses event-driven detection
- **Testing complexity**: Event sources require actual hardware or mocked adapters for testing

### Mitigations

- **Fallback polling exists**: `autorip_poll_loop()` still runs when no event source (though disabled in current implementation)
- **Manual refresh always available**: Users can refresh drives at any time
- **Clear status reporting**: Health endpoint and frontend UI clearly show event source status
- **Event source selection is one-time**: No runtime complexity from switching sources

## Implementation Details

### Files Added

| File | Purpose |
|------|---------|
| `infra/src/drive_events/mod.rs` | Module exports, event types, adapter trait, chain logic |
| `infra/src/drive_events/event.rs` | `DriveEvent` enum definition |
| `infra/src/drive_events/adapter.rs` | `DriveEventSource` trait definition |
| `infra/src/drive_events/chain.rs` | Chain of responsibility logic |
| `infra/src/drive_events/udev.rs` | udev adapter implementation |
| `infra/src/drive_events/inotify_sysfs.rs` | inotify /sys/block adapter |
| `infra/src/drive_events/inotify_dev.rs` | inotify /dev adapter |
| `infra/src/drive_events/dbus.rs` | D-Bus adapter |
| `web/src/routes/drive_events.rs` | Event handler and loop |

### Files Modified

| File | Changes |
|------|---------|
| `web/src/api.rs` | Added `drive_event_source`, `drive_event_rx`, `event_source_available` to `AppState`; added `EventSourceHealth` to health endpoint |
| `cli/src/commands/serve.rs` | Build event chain at startup, spawn listener and event loop |
| `web/src/routes/ripper.rs` | Added `/v1/ripper/event-source` endpoint; extracted `run_rip_task()` from `start_rip()` |
| `web/svelte/src/lib/components/DrivePanel.svelte` | SSE subscription, auto-refresh, per-drive refresh button, event source gating |
| `web/svelte/src/lib/api/ripper.ts` | Added `getEventSourceStatus()`, `getNamingList()` |
| `web/svelte/src/lib/api/sse.ts` | Added `subscribe()` reactive API |

### New API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/v1/ripper/event-source` | Event source availability status |
| GET | `/health/detailed` | Now includes `event_source` field |

### SSE Events

| Event | Format | When |
|-------|--------|------|
| `drive_inserted` | `{"type":"drive_inserted","drive_index":N,"device_path":"...","disc_name":"..."}` | Disc inserted |
| `drive_ejected` | `{"type":"drive_ejected","drive_index":N,"device_path":"..."}` | Disc ejected |

## References

- [drive-event-sources.md](../designs/drive-event-sources.md) — Full design spec
- [drive-management.md](../designs/drive-management.md) — Drive management design (updated)
- Chain of Responsibility pattern — GoF design patterns
- Adapter pattern — GoF design patterns
