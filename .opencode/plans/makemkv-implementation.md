# MakeMKV Backend Abstraction — Implementation Plan

## Status: IN PROGRESS

Phase 5: Abstract ripping behind traits. Separate `RipBackend` (sync, dyn-compatible)
from `RipSession` (async, Pin<Box>). No extra dependencies.

---

## Design Decisions

1. **Separate traits (option 3)**: `RipBackend` has sync methods → fully dyn-compatible.
   `RipSession` has async methods with `Pin<Box<dyn Future>>` → wrapped in concrete `RipHandle`.
2. **Config**: new `ripping.command` field replaces `makemkvcon_path`. Legacy field still works.
3. **Auto-detection**: native → `/usr/bin/makemkvcon` → Flatpak
4. **DryRunBackend**: yes, for testing and CI

---

## Files

### Create
| File | Purpose |
|------|---------|
| `core/src/ripping.rs` | Traits + domain types |
| `infra/src/makemkv/backend.rs` | `MakemkvBackend` impl |
| `infra/src/makemkv/session.rs` | `MakemkvSession` impl |
| `infra/src/makemkv/dryrun.rs` | `DryRunBackend` for testing |

### Modify
| File | Changes |
|------|---------|
| `core/src/lib.rs` | Add `pub mod ripping;` + re-exports |
| `core/Cargo.toml` | Add `futures` (for `Pin`) |
| `infra/src/config.rs` | Add `RippingConfig` struct |
| `infra/src/makemkv/mod.rs` | Remove moved types, add submodules |
| `infra/src/lib.rs` | Update exports |
| `web/src/api.rs` | `AppState.backend: Option<Arc<dyn RipBackend>>` |
| `cli/src/commands/rip.rs` | Use `get_backend(&config)` |
| `cli/src/commands/serve.rs` | Construct backend from config |

---

## Implementation Steps

### Step 1: Core types and traits
- `DriveInfo`, `DriveState`, `RipEvent`, `RipError` (domain types)
- `RipBackend` trait: `detect_drives()` → sync, `start_rip()` → sync, returns `RipHandle`
- `RipSession` trait: `next_event()` → `Pin<Box<dyn Future>>`, `cancel()` → `Pin<Box<dyn Future>>`
- `RipHandle` struct: wraps `Box<dyn RipSession>`, delegates async methods

### Step 2: Config
- `RippingConfig { command: Option<String>, makemkvcon_path: Option<String> }`
- Resolution: command → makemkvcon_path → auto-detect

### Step 3: MakemkvBackend
- Stores `command: Vec<String>`
- `detect_drives()`: `std::process::Command` (blocking), parse via `parser.rs`
- `start_rip()`: `tokio::process::Command`, spawn reader task, return `RipHandle`
- Auto-detection: which → common paths → Flatpak

### Step 4: MakemkvSession
- Wraps `Child` + `mpsc::UnboundedReceiver<RipEvent>`
- `next_event()`: channel recv
- `cancel()`: kill child

### Step 5: DryRunBackend
- `detect_drives()`: returns 2 fake drives
- `start_rip()`: spawns progress events on interval

### Step 6: Wire everything
- `AppState.backend: Option<Arc<dyn RipBackend>>`
- CLI `get_backend()` with install instructions
- serve.rs constructs backend from config

---

## Verification
```bash
cargo clippy --all
cargo test --all
```

## Commit
```
refactor: trait-based ripping backend abstraction

- Move DriveInfo, DriveState, RipEvent to core/src/ripping.rs
- Add RipBackend (sync) + RipSession (async) traits
- Add RipHandle concrete wrapper
- MakemkvBackend implements RipBackend with Flatpak auto-detect
- Add DryRunBackend for testing
- Config: command-based ripping with legacy makemkvcon_path support
- Update AppState, CLI, and web endpoints to use trait objects
```
