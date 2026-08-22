# Design: MakeMKV Backend

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Wrap MakeMKV's `makemkvcon` CLI tool with a trait-based abstraction (`RipBackend`). Supports native install, Flatpak, and dry-run testing. This document describes the *current* behavior of the MakeMKV backend.

---

## Architecture

```
core (RipBackend trait) → infra (MakemkvBackend) → cli/web (consumers)
```

- `RipBackend` trait defined in `core/src/ripping.rs` — zero infrastructure dependencies
- `MakemkvBackend` implements the trait in `infra/src/makemkv/backend.rs`
- `DryRunBackend` implements the trait for testing in `infra/src/makemkv/dryrun.rs`
- `MakemkvSession` manages the child process in `infra/src/makemkv/session.rs`
- `parser` module parses CLI output in `infra/src/makemkv/parser.rs`

---

## RipBackend Trait

```rust
pub trait RipBackend: Send + Sync {
    fn detect_drives(&self) -> Result<Vec<DriveInfo>, RipError>;
    fn start_rip(&self, drive: usize, dest: &str) -> Result<RipHandle, RipError>;
}
```

---

## MakemkvBackend

### Structure

```rust
pub struct MakemkvBackend {
    command: Vec<String>,  // e.g., ["flatpak", "run", "--command=makemkvcon", "com.makemkv.MakeMKV"]
}
```

### Construction

| Method | Description |
|---|---|
| `new(command: Vec<String>)` | Create with explicit command |
| `from_config(command: &str)` | Create from config string (splits on whitespace) |
| `auto_detect()` | Auto-detect installation |
| `from_config_or_detect(command: Option<&str>)` | Use config or auto-detect |

### Auto-Detection Order

1. `makemkvcon` on PATH (via `which`)
2. `/usr/bin/makemkvcon`
3. `/usr/local/bin/makemkvcon`
4. Flatpak: checks `flatpak list` for `com.makemkv.MakeMKV`

### Command Execution

```rust
fn run_command(&self, args: &[&str]) -> Result<String, RipError>
```

**Behavior:**
1. Runs `self.command[0]` with `self.command[1..]` + `args`
2. Returns stdout on success
3. Returns `RipError::Backend(stderr)` on non-zero exit

---

## Drive Detection

### `detect_drives()`

**Command:** `makemkvcon -r --cache=1 info disc:9999`

**Why `disc:9999`:** Queries all drives without inserting a specific disc index.

**Output parsing:**
- Parses `DRV:` lines via `parser::parse_drive_line()`
- Filters out non-visible drives (status == 256 = not attached)
- Calculates state from `enabled` flag:
  - `enabled == true` + disc name → `Ready`
  - `enabled == false` → `Error`
  - No disc name → `Empty`
- Derives display name via `parser::derive_name()` (underscore-based disc names)

**Drive line format:**
```
DRV:index,status,enabled,flags,"drive_name","disc_name","device_path"
```

**Status values:**
| Status | Meaning |
|---|---|
| 0 | Empty |
| 1 | Open (no disc) |
| 2 | Closed (disc present) |
| 3 | Loading |
| 256 | Not attached |

---

## Ripping

### `start_rip(drive: usize, dest: &str) -> Result<RipHandle, RipError>`

**Command:**
```
makemkvcon -r --cache=1024 --noscan mkv disc:{drive} all {dest}
```

**Flags:**
- `-r` — Robot mode (machine-readable output)
- `--cache=1024` — 1024KB read cache
- `--noscan` — Don't scan for drives (drive index provided)
- `mkv` — Output format
- `disc:{drive}` — Source disc index
- `all` — All titles
- `{dest}` — Destination directory

**Process setup:**
1. Creates destination directory if it doesn't exist
2. Spawns child process with:
   - stdin: piped (for overwrite prompt handling)
   - stdout: piped (for progress parsing)
   - stderr: piped (for logging)
   - `kill_on_drop(true)` — Auto-terminate on process drop
3. Spawns task to write `"n\n"` to stdin (auto-answer "no" to overwrite prompts)
4. Wraps child in `MakemkvSession`
5. Returns `RipHandle` wrapping the session

### Why `"n\n"` to stdin?

MakeMKV may prompt to overwrite existing files. Since the destination is already uniquified by `unique_rip_destination()`, the `"n\n"` is a defensive safety net to prevent blocking on interactive prompts.

---

## MakemkvSession

### Structure

```rust
pub struct MakemkvSession {
    child: Option<Child>,
    rx: mpsc::UnboundedReceiver<RipEvent>,
}
```

### Event Parsing

**Reader tasks:**
1. **stdout reader** — Parses output lines into `RipEvent`:
   - `PRGV:current,total,max` → `RipEvent::Progress { current, total, max }`
   - `MSG:201003,...` → `RipEvent::Complete`
   - `MSG:3010xx,...` → `RipEvent::Error(message)`
   - Other `MSG:` lines → `RipEvent::Message(message)`
2. **stderr reader** — Logs lines at debug level (no events)

### Cancellation

```rust
fn cancel(&mut self) -> Pin<Box<dyn Future<Output = Result<(), RipError>>>>
```

**Behavior:**
1. Calls `child.kill()` on the child process
2. Sets `child = None`
3. Returns `Ok(())` or `RipError::Io`

### Drop

Safety fallback: calls `child.start_kill()` if child is still present.

---

## Parser Module

### Input Format

MakeMKV robot-mode output uses prefix-based lines:

| Prefix | Format | Parsed As |
|---|---|---|
| `DRV:` | `DRV:index,status,enabled,flags,"name","disc","path"` | `ParsedDrive` |
| `PRGV:` | `PRGV:current,total,max` | `ParsedProgress` |
| `MSG:` | `MSG:code,flags,count,"message","format",params...` | `ParsedMessage` |
| `TCOUT:` | `TCOUT:count` | Title count |

### CSV Parsing

`parse_csv_fields()` — Handles quoted fields containing commas:
```
1,2,"hello, world",4 → ["1", "2", "\"hello, world\"", "4"]
```

### Unquote

`unquote()` — Removes surrounding quotes and unescapes `\"` → `"` and `\\` → `\`.

### Error Detection

`is_error_message()` — Checks if message code is in range `301000` to `301100` (MakeMKV error codes).

### Completion Detection

`is_completion_line()` — Checks if line starts with `MSG:201003,` (MakeMKV completion message).

---

## Disc Name Parsing

### `parse_disc_name(name: &str) -> (String, Option<u16>)`

**Patterns:**
- `INCEPTION_2010` → `("Inception", Some(2010))`
- `THE_DARK_KNIGHT` → `("The Dark Knight", None)`
- `BRAVEHEART` → `("Braveheart", None)`
- `2001_A_SPACE_ODYSSEY` → `("2001 A Space Odyssey", None)` (year in title, not trailing)

**Algorithm:**
1. Replace underscores with spaces
2. Trim
3. Check for trailing 4-digit year (1900–2100) preceded by space
4. If found: extract year, rest is title
5. Apply `NameCleaner::to_title_case()` for proper casing

### `derive_name(name: &str) -> Option<(String, Option<u16>)>`

Like `parse_disc_name` but returns `None` if:
- Title is empty
- Name doesn't contain underscores (user-made labels)

---

## DryRunBackend

### Purpose

Testing and CI — returns fake drives and simulates rip progress.

### Drives

| Index | Name | State | Disc |
|---|---|---|---|
| 0 | Fake DVD Drive | Ready | "Test Movie (2024)" |
| 1 | Fake Blu-ray Drive | Empty | None |

### Rip Simulation

Events (5 steps):
1. `Message("Starting dry-run rip on drive {index}")`
2. `Progress(25, 100, 100)`
3. `Progress(50, 100, 100)`
4. `Progress(75, 100, 100)`
5. `Progress(100, 100, 100)`
6. `Complete`

Cancellation: Sets `cancelled = true`, subsequent `next_event()` returns `None`.

---

## Installation

### `check_installation() -> InstallationStatus`

**Checks:**
1. MakeMKV auto-detection
2. Flatpak availability
3. Flathub remote configured
4. Flatpak device permissions
5. Cdrom group membership

**Status struct:**
```rust
pub struct InstallationStatus {
    pub installed: bool,
    pub method: DetectionMethod,     // Native(path) | Flatpak | None
    pub flatpak_available: bool,
    pub flathub_configured: bool,
    pub flatpak_permissions_ok: bool,
    pub drive_access_ok: bool,
    pub missing: Vec<MissingComponent>, // Flatpak | Flathub | MakeMKV | CdromGroup
}
```

### `install_makemkv(dry_run: bool) -> anyhow::Result<InstallationStatus>`

Interactive installation with prompts:
1. Install Flatpak (if missing) — detects apt/dnf/pacman
2. Add Flathub remote (if missing)
3. Install MakeMKV from Flathub (if missing)
4. Add user to cdrom group (if missing) — requires sudo
5. Verify installation

---

## Eject

### `infra::eject_disc(device_path: &str) -> Result<(), anyhow::Error>`

Uses Linux `eject` command.

**Error handling:**
- "No medium found" → `Ok(())` (drive already empty)
- "not found" / "No such" → `Err` (device doesn't exist)
- "eject" command not found → `Err` (install `eject` package)
- Other errors → `Err` with stderr message

---

## Where This Lives

| Concern | File |
|---|---|
| RipBackend trait | `core/src/ripping.rs` |
| MakemkvBackend | `infra/src/makemkv/backend.rs` |
| MakemkvSession | `infra/src/makemkv/session.rs` |
| Parser | `infra/src/makemkv/parser.rs` |
| Installation | `infra/src/makemkv/setup.rs` |
| DryRunBackend | `infra/src/makemkv/dryrun.rs` |
| Module exports | `infra/src/makemkv/mod.rs` |
| Eject function | `infra/src/makemkv/mod.rs` |

---

## Rip Flow (ADR-025)

### Overview

Per ADR-025 (Three-Window Architecture), the Rip window handles disc ripping with concurrent autodetection.

### Flow

```
User clicks "Rip" → MakemkvBackend.start_rip()
                    ↓
              Files rip to tmp folder (configurable, default /tmp/riply)
                    ↓
              Concurrent: TMDb lookup runs on disc name
              (independent of rip, no overlap)
                    ↓
              SSE: rip:named or rip:naming_failed (live feedback)
                    ↓
              Rip completes → SSE: rip:complete
                    ↓
              Post-rip move (files moved to final destination):
              - Movie: biggest file → {Title} ({Year})/
                rest → {Title} ({Year})/Extras/
              - TV: user confirms show/season → {Show} ({Year})/Season {NN}/
                files → {Show} - S{NN}E{NN}.{ext}
                extras → Season {NN}/Extras/
              - Unidentified: files stay in tmp with disc name
                    ↓
              SSE: rip:renamed (if moved)
              Drive card: Complete / Error / Ready
```

### Auto-Naming (Concurrent with Rip)

TMDb lookup begins at rip start (uses disc name from drive info):
1. Parse disc name for title + year (e.g., `INCEPTION_2010` → `Inception`, 2010)
2. Search TMDb with parsed title
3. If match found → send `rip:named:{index}:{type}:{title} ({year})` via SSE (`type` = movie or tv)
4. If no match → send `rip:naming_failed:{index}` via SSE
5. Drive card shows live: `"Disc: Inception → Found: Inception (2010)"` or `"Disc: Inception → Not found"`

### Post-Rip Move

At rip completion (after `rip:complete`):
1. Detect biggest file = main title
2. Move files based on media type (movie or TV show):
   - **Movie:** `{Title} ({Year})/` with main file `{Title}.{ext}`, extras in `Extras/`
   - **TV Show:** `{Show} ({Year})/Season {NN}/` with files `{Show} - S{NN}E{NN}.{ext}`, extras in `Season {NN}/Extras/`
   - **Unidentified:** files stay in tmp with disc name
3. Apply "(Copy N)" if destination already exists
4. Send `rip:renamed:{index}:{new_name}` via SSE
5. Drive card ends in terminal state: `Complete`, `Error`, or `Ready` (if unidentified)

### Tmp Folder

- Ripped content written to configurable tmp folder (default `/tmp/riply`)
- See ADR-027 (Tmp Folder Lifecycle) for cleanup policy
- On server restart, content older than N days (configurable, default 7) is deleted

### Jellyfin Naming Convention

Ripped content is restructured into Jellyfin-compatible format:

- **Movies:** `{Title} ({Year})/` with main file `{Title}.{ext}`
- **TV Shows:** `{Series} ({Year})/Season {NN}/` with files `{Series} - S{NN}E{NN}.{ext}`
- **Extras:** Moved to `Extras/` subfolder (movies) or `Season {NN}/Extras/` (TV)

### Movie vs TV Show Detection

See ADR-028 (Movie vs TV Show Detection) for full details.

Detection priority:
1. **TMDb match** — Most accurate (check if result is movie or tv_show)
2. **File-size heuristic** — Fallback when TMDb fails:
   - Movie: 1 large file (>3x median) + small extras
   - TV: Multiple similarly-sized files (std dev < 30% of mean)
3. **User confirmation** — Always required, one-click flip Movie ↔ TV Show

### SSE Events (JSON format)

| Event | data fields | When |
|---|---|---|
| `rip:starting` | `{}` | Rip begins |
| `rip:progress` | `{ "pct": N, "eta": N \| null, "stage": "text" }` | Every progress update |
| `rip:named` | `{ "title": "Movie", "year": 2010, "type": "movie" }` | Auto-naming succeeded |
| `rip:naming_failed` | `{}` | Auto-naming failed |
| `rip:complete` | `{ "dest": "/path/to/output" }` | Rip finished, files moved |
| `rip:error` | `{ "message": "error text" }` | Rip failed |
| `rip:message` | `{ "text": "MakeMKV status" }` | Status messages |
| `rip:cancelled` | `{}` | Rip cancelled |

---

## Notes

- **Robot mode (`-r`)** — All CLI interaction uses machine-readable output
- **Flatpak command** — `flatpak run --command=makemkvcon com.makemkv.MakeMKV`
- **Process isolation** — Each rip spawns a new child process
- **Kill on drop** — Processes are auto-terminated when session is dropped
- **Stdin safety net** — `"n\n"` prevents blocking on overwrite prompts
- **Parser is strict** — Malformed output is silently ignored
- **Error codes** — MakeMKV errors start with code 3010
- **Completion code** — `MSG:201003` signals rip completion
- **DryRunBackend** — Returns 2 fake drives, simulates 5 progress steps
