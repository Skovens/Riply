# Feature: Event-Driven Drive Detection

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md) and [ripping-and-naming.md](./ripping-and-naming.md). This document describes the technical implementation details.

## Purpose

Replace continuous drive polling with event-driven detection. The system listens for disc insertion/ejection events from the operating system instead of polling drives repeatedly. This eliminates unnecessary drive spinning and LED activity.

---

## Architecture

### Adapter Pattern

Each detection source is wrapped in an adapter that converts its native event format into a standardized `DriveEvent`. The rest of the system never sees raw udev rules, inotify events, or D-Bus signals — only the standardized event type.

### Strategy Pattern

All adapters implement the same `DriveEventSource` trait, making them interchangeable. The system selects one adapter at startup based on availability.

### Chain of Responsibility

Sources are tried in priority order. First available source wins:
1. udev
2. inotify on `/sys/block/`
3. inotify on `/dev/sr*`
4. D-Bus

---

## Standard Event Type

```rust
pub enum DriveEvent {
    Inserted { 
        drive_index: usize, 
        device_path: String,
        disc_name: Option<String>,
    },
    Ejected { 
        drive_index: usize, 
        device_path: String,
    },
}
```

- `drive_index`: MakeMKV drive index (0, 1, 2, ...)
- `device_path`: OS device path (e.g., `/dev/sr0`)
- `disc_name`: Optional disc name from MakeMKV (populated on Inserted)

---

## Adapter Trait

```rust
pub trait DriveEventSource: Send + Sync + 'static {
    /// Human-readable name of this source (e.g., "udev", "inotify_sysfs")
    fn name(&self) -> &str;
    
    /// Check if this source is available on the current system
    fn is_available(&self) -> bool;
    
    /// Start listening for events. Sends events to the channel.
    /// This is a blocking call — should be called in a spawned task.
    fn listen(self: Arc<Self>, tx: mpsc::UnboundedSender<DriveEvent>);
}
```

**Implementation requirements:**
- `is_available()` must be non-blocking and fast (< 100ms)
- `listen()` should run until the channel is closed or the source becomes unavailable
- All adapters must be `Send + Sync + 'static`
- Errors during listening should be logged, not propagated (to avoid killing the listener task)

---

## Chain Logic

**Selection process (executed at server startup):**

1. Create instances of all available adapters
2. Test each adapter's `is_available()` in priority order
3. First adapter where `is_available()` returns `true` is selected
4. Store selected source in `AppState.drive_event_source`
5. Spawn the listener task: `source.listen(tx)`
6. Log which source was selected (or none)

**Selection log messages:**
```
"Event source selected: udev"
"Event source selected: inotify_sysfs"
"Event source selected: inotify_dev"
"Event source selected: dbus"
"No event source available — polling disabled"
```

**Fallback behavior:**
- If no source is available, `AppState.event_source_available = false`
- Autorip toggle is disabled in UI
- User must manually refresh drives

---

## Source Implementations

### 1. udev Adapter

**Source:** `infra/src/drive_events/udev.rs`

**How it works:**
- Uses `libudev` to monitor device events
- Listens for `change` and `remove` actions on block devices with `DEVTYPE="disk"` and `ID_BUS="ata"`
- Maps device path to MakeMKV drive index via `makemkvcon`
- Filters for optical drives (`ID_CDROM` property)

**Dependencies:**
- `libudev-sys` (system library, no Rust dependency)
- `libudev` crate (optional, wraps libudev-sys)

**Availability check:**
- Check if `libudev` is linked
- Check if user has permission to read udev database

**Advantages:**
- Most reliable source on Linux
- Handles both insertion and ejection
- Provides rich device metadata

**Disadvantages:**
- Linux-only
- Requires libudev to be installed

---

### 2. inotify on /sys/block/ Adapter

**Source:** `infra/src/drive_events/inotify_sysfs.rs`

**How it works:**
- Uses inotify to watch `/sys/block/` directory
- Monitors for file creation/deletion events (new block devices appear/disappear)
- When a change is detected, scans `/sys/block/` for optical drives
- Maps device path to MakeMKV drive index via `makemkvcon`

**Dependencies:**
- `nix` crate (already a dependency — provides inotify bindings)

**Availability check:**
- Check if `/sys/block/` exists and is readable
- Check if inotify is supported (Linux)

**Advantages:**
- Works on Linux
- No additional dependencies beyond nix
- Lightweight

**Disadvantages:**
- Linux-only
- May not detect all edge cases (e.g., USB drives)
- Requires scanning on each event

---

### 3. inotify on /dev/sr* Adapter

**Source:** `infra/src/drive_events/inotify_dev.rs`

**How it works:**
- Uses inotify to watch `/dev/` directory for changes to `sr*` devices
- Monitors for device node creation/deletion
- When a change is detected, runs `makemkvcon -r list` to update drive state
- Simpler than sysfs approach but less granular

**Dependencies:**
- `nix` crate (already a dependency)

**Availability check:**
- Check if `/dev/sr*` devices exist
- Check if inotify is supported

**Advantages:**
- Works on Linux
- Simpler implementation
- Directly targets optical drives

**Disadvantages:**
- Linux-only
- Less reliable than sysfs approach
- May miss some events

---

### 4. D-Bus Adapter

**Source:** `infra/src/drive_events/dbus.rs`

**How it works:**
- Connects to system D-Bus
- Subscribes to signals from `org.freedesktop.UDisks2` or `org.freedesktop.login1`
- Listens for media insertion/ejection signals
- Maps device path to MakeMKV drive index via `makemkvcon`

**Dependencies:**
- `zbus` crate (optional dependency)

**Availability check:**
- Check if D-Bus system bus is accessible
- Check if zbus is compiled in

**Advantages:**
- Works on any system with D-Bus
- Standardized interface across desktop environments
- Handles USB and optical drives

**Disadvantages:**
- Requires D-Bus daemon
- May not be available on headless servers
- Additional dependency

---

## AppState Integration

**New fields:**
```rust
pub struct AppState {
    // Existing fields...
    
    /// Active drive event source (if available)
    pub drive_event_source: Option<Arc<dyn DriveEventSource>>,
    
    /// Receiver for drive events from the listener task
    pub drive_event_rx: RwLock<Option<mpsc::UnboundedReceiver<DriveEvent>>>,
    
    /// Whether an event source is available (gates autorip toggle)
    pub event_source_available: bool,
}
```

**Initialization (in `serve.rs`):**
```rust
// Build event chain
let chain = DriveEventChain::new();
let selected = chain.select();

state.drive_event_source = selected.source;
state.event_source_available = selected.is_available;

if let Some(source) = selected.source {
    let (tx, rx) = mpsc::unbounded_channel();
    *state.drive_event_rx.write().await = Some(rx);
    
    // Spawn listener task
    tokio::spawn(async move {
        source.listen(tx);
    });
    
    info!("Event source selected: {}", source.name());
} else {
    info!("No event source available — polling disabled");
}
```

---

## Event Handler

**Module:** `web/src/routes/drive_events.rs`

**Handler function:**
```rust
pub async fn drive_event_loop(
    state: Arc<AppState>,
    mut rx: mpsc::UnboundedReceiver<DriveEvent>,
)
```

**Behavior:**
1. Wait for events from channel
2. On `Inserted`:
   - Broadcast SSE event with `kind: "drive_inserted"`, drive_index, device_path, disc_name
   - If AutoRip is enabled for this drive → spawn `run_rip_task()` (extracted from HTTP handler)
3. On `Ejected`:
   - Broadcast SSE event with `kind: "drive_ejected"`, drive_index, device_path
4. Continue listening

**Key constraints:**
- Only processes events for drives that exist
- Never polls continuously
- Never spins drives unnecessarily
- Handles errors gracefully (log + SSE message, don't crash)
- AutoRip triggered only when `event_source_available = true` and drive is in autorip list

## AppState Initialization

**Module:** `cli/src/commands/serve.rs`

**Implementation:**
```rust
// Build event chain
let chain = DriveEventChain::new();
let selected = chain.select();

let (drive_event_source, event_source_available) = if let Some(source) = selected.source {
    (Some(source), true)
} else {
    (None, false)
};

let state = Arc::new(AppState {
    // ... existing fields ...
    drive_event_source,
    drive_event_rx: RwLock::new(None),
    event_source_available,
});

// Spawn the drive event loop if an event source is available
if event_source_available {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let source = state.drive_event_source.as_ref().unwrap().clone();
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move { source.listen(tx); });
    tokio::spawn(async move {
        web::routes::drive_events::drive_event_loop(state_clone, rx).await;
    });
    info!("Drive event loop started");
}
```

---

## UI Behavior

### Drive Panel (DrivePanel.svelte)

**Auto-refresh on SSE events:**
- Subscribes to SSE on mount via `sseApi.subscribe()`
- On `kind: "drive", action: "updated"` → calls `fetchDrives()`
- On `kind: "rip", action: "complete" | "message"` → calls `fetchDrives()`
- No polling interval — refreshes only when events arrive or user clicks Refresh

**Per-drive refresh button (↻):**
- Always present on each drive card
- Clicking refresh calls `fetchDrives()` (full drive list refresh)
- Button always enabled (no loading state needed for quick refresh)

**Autorip toggle:**
- Fetched from `GET /v1/ripper/event-source` on mount
- Enabled if `event_source_available = true`
- Disabled (greyed out) if `event_source_available = false`
- Tooltip when disabled: "Auto-rip requires event source (udev/inotify)"
- Toggle is a no-op if disabled (early return in `toggleAutorip()`)

### Event Source Status Endpoint

**`GET /v1/ripper/event-source`:**
```json
{
  "ok": true,
  "data": { "available": true }
}
```

### Naming List

- Fetch from `GET /v1/ripper/naming-list` on mount
- Type: `NamingListEntry[]` with `drive_index`, `disc_name`, `derived_name`, `rip_status`, `rip_progress`

---

## Health Endpoint

**`/health/detailed` response includes:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "database": "ok",
  "drives": { "count": 1, "status": "ok", "error": null },
  "makemkv": { "available": true, "status": "ok", "error": null },
  "tmdb": { "configured": true, "status": "ok", "error": null },
  "event_source": {
    "available": true,
    "status": "ok",
    "error": null
  }
}
```

**If no event source:**
```json
{
  "event_source": {
    "available": false,
    "status": "unavailable",
    "error": "No event source available — polling disabled"
  }
}
```

---

## CLI Health Check

**Note:** CLI health check (`riply check`) is a separate concern from the web server health endpoint. Event source status is reported via `/health/detailed` and `/v1/ripper/event-source` API endpoints only.

---

## Acceptance Criteria

- [x] AC1: Event sources are tried in priority order (udev → inotify_sysfs → inotify_dev → dbus)
- [x] AC2: First available source is selected at startup
- [x] AC3: Selected source is logged on startup
- [x] AC4: No event source → autorip toggle disabled with tooltip
- [x] AC5: Refresh button per-drive triggers full drive list refresh
- [x] AC6: Auto-refresh on SSE events (drive:updated, rip:complete, rip:message)
- [x] AC7: Initial page load auto-refreshes all drives once
- [x] AC8: Event handler processes events without continuous polling
- [x] AC9: Event handler errors are logged and sent via SSE (don't crash)
- [x] AC10: Health endpoint reports event source status
- [x] AC11: Event source status available via `/v1/ripper/event-source`
- [x] AC12: No drive spinning between events (event-driven only)
- [x] AC13: `cargo build` passes for all crates
- [x] AC14: AutoRip triggered on disc insertion when enabled

---

## Files Affected

### Backend (Rust)

| File | Changes |
|------|---------|
| `infra/src/drive_events/mod.rs` | New module: exports, event types, adapter trait, chain logic |
| `infra/src/drive_events/event.rs` | DriveEvent enum definition |
| `infra/src/drive_events/adapter.rs` | DriveEventSource trait definition |
| `infra/src/drive_events/chain.rs` | Chain of responsibility logic |
| `infra/src/drive_events/udev.rs` | udev adapter implementation |
| `infra/src/drive_events/inotify_sysfs.rs` | inotify /sys/block adapter |
| `infra/src/drive_events/inotify_dev.rs` | inotify /dev adapter |
| `infra/src/drive_events/dbus.rs` | D-Bus adapter |
| `web/src/api.rs` | Add drive_event_source, drive_event_rx, event_source_available to AppState |
| `cli/src/commands/serve.rs` | Build event chain at startup, spawn listener |
| `web/src/routes/drive_events.rs` | Event handler module |
| `web/src/routes/ripper.rs` | Update router, add event source to health endpoint |

### Frontend (TypeScript/Svelte)

| File | Changes |
|------|---------|
| `web/svelte/src/routes/+page.svelte` | Auto-refresh on mount, per-drive refresh button, naming list |
| `web/svelte/src/lib/api/ripper.ts` | Add per-drive refresh, naming list methods |
| `web/svelte/src/lib/types.ts` | Add event source status to health response |

### Documentation

| File | Changes |
|------|---------|
| `docs/designs/drive-event-sources.md` | New spec (this file) |
| `docs/designs/ripping-and-naming.md` | Update: add event source section |
| `docs/designs/drive-management.md` | Update: reflect event-driven detection |
| `README.md` | Document event source requirements |

---

## Notes

- **No new Rust dependencies** — udev via system lib, inotify via nix (already a dep), D-Bus via zbus (optional dep)
- **No breaking changes** — existing API endpoints unchanged, new features are additive
- **Linux-first** — most sources are Linux-specific; Windows/macOS would need platform-specific adapters
- **Event source selection is one-time** — happens at startup, not per-page-load
- **Refresh is always available** — even with event source, user can manually refresh
- **No continuous polling** — drives are only accessed on event or manual refresh
- **Error resilience** — if event source dies, system continues with existing state; user can refresh
