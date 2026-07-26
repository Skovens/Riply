# MakeMKV Integration Implementation Plan

## Design Principles

1. **Graceful degradation**: Riply works fully when `makemkvcon` is not installed. Every
   user-facing path (CLI, web API) reports a clear "MakeMKV not available" message instead
   of crashing. The scan/analyze/review/move pipeline is never affected.

2. **Testable**: All output parsing is pure functions separated from subprocess management.
   Tests use fixture strings, not real binaries.

3. **Cancellation-safe**: Killing the child process is the primary cancel mechanism. The
   existing `JobScheduler` `CancellationToken` is checked between pipeline stages.

---

## 1. Config Changes (`infra/src/config.rs`)

Add one field:

```rust
/// Path to makemkvcon binary (auto-detected if not set)
#[serde(default)]
pub makemkvcon_path: Option<String>,
```

Default: `None` (auto-detect at runtime by checking `which makemkvcon` and
`/usr/bin/makemkvcon`). This means no config change is needed for existing users.

---

## 2. Output Parser (`infra/src/makemkv/parser.rs`)

Pure functions that parse `makemkvcon -r` output lines. No subprocess logic here.

### Line types to parse

| Prefix   | Meaning                        | Example |
|----------|--------------------------------|---------|
| `DRV:`   | Drive info                     | `DRV:0,1,1,24,"HL-DT-ST BD-RE BH16NS40","DISC_NAME"` |
| `PRGV:`  | Progress value                 | `PRGV:5000,10000,100000` |
| `PRGC:`  | Current operation title        | `PRGC:1234,0,"Processing titles"` |
| `PRGT:`  | Total operation title          | `PRGT:5678,0,"Overall progress"` |
| `MSG:`   | Message / error                | `MSG:3010,0,0,"No disc found.","There is no disc in drive."` |
| `TCOUT:` | Title count                    | `TCOUT:5` |
| `TINFO:` | Title info (attribute id, code, value) | `TINFO:0,1,"Title Name"` |
| `CINFO:` | Disc info                      | `CINFO:1,0,"disc label"` |

### Functions

```rust
/// Parsed drive info from a DRV: line
pub fn parse_drive_line(line: &str) -> Option<ParsedDrive>

/// Progress from a PRGV: line (current, total, max)
pub fn parse_progress_line(line: &str) -> Option<ParsedProgress>

/// Title count from TCOUT:
pub fn parse_title_count(line: &str) -> Option<usize>

/// Detect if a line indicates an error
pub fn parse_error_line(line: &str) -> Option<String>

/// Detect if a line indicates rip completion
pub fn is_completion_line(line: &str) -> bool
```

### Data types

```rust
pub struct ParsedDrive {
    pub index: usize,
    pub visible: bool,
    pub enabled: bool,
    pub flags: u32,
    pub drive_name: String,
    pub disc_name: Option<String>,
}

pub struct ParsedProgress {
    pub current: u64,
    pub total: u64,
    pub max: u64,
}
```

### Test strategy

- 15+ unit tests with fixture strings copied from real `makemkvcon -r` output
- Edge cases: empty disc name, unicode drive names, zero progress, max progress
- Parse malformed lines → return `None`, never panic

---

## 3. MakeMkvClient Rewrite (`infra/src/makemkv.rs`)

### Structure

```rust
pub struct MakeMkvClient {
    binary_path: PathBuf,
}

impl MakeMkvClient {
    /// Create client, auto-detecting makemkvcon if path is None.
    /// Returns Err if binary not found.
    pub fn new(makemkvcon_path: Option<&str>) -> Result<Self, MkvError>
}
```

### Availability check

On construction, verify the binary exists and is executable:
1. If `makemkvcon_path` is `Some(path)`, use that exact path
2. If `None`, try `which makemkvcon` (via `std::process::Command`)
3. Fall back to checking `/usr/bin/makemkvcon`
4. If nothing found → return `Err(MkvError::NotInstalled)`

### Error type

```rust
#[derive(Debug, thiserror::Error)]
pub enum MkvError {
    #[error("MakeMKV is not installed. Install from https://www.makemkv.com")]
    NotInstalled,

    #[error("makemkvcon failed: {0}")]
    CommandFailed(String),

    #[error("Drive {0} not found")]
    DriveNotFound(usize),

    #[error("No disc in drive {0}")]
    NoDisc(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Methods

#### `detect_drives(&self) -> Result<Vec<DriveInfo>, MkvError>`

1. Spawn `makemkvcon -r --cache=1 info disc:9999`
   - `disc:9999` is the special "list all drives" address
2. Read stdout line by line
3. Parse each `DRV:` line via `parser::parse_drive_line()`
4. Filter to `visible == true` drives
5. Map `DriveState` from flags (empty if disc_name is None, ready otherwise)
6. Return `Vec<DriveInfo>`

#### `start_rip(&self, drive_index: usize, destination: &str) -> Result<RipHandle, MkvError>`

1. Verify destination directory exists (create if needed)
2. Spawn `makemkvcon -r --cache=1024 --noscan mkv disc:{index} all {destination}`
   - `--noscan`: don't rescan drives, just use current state
   - `--cache=1024`: 1GB cache for Blu-ray
3. Return a `RipHandle` containing the child process and a progress channel

#### `cancel_rip(&self, handle: &mut RipHandle) -> Result<(), MkvError>`

1. Kill the child process via `handle.child.kill()`
2. Wait for process to exit
3. Return Ok

### RipHandle (replaces old RipSession)

```rust
pub struct RipHandle {
    child: tokio::process::Child,
    rx: tokio::sync::mpsc::UnboundedReceiver<RipEvent>,
}

pub enum RipEvent {
    Progress { current: u64, total: u64, max: u64 },
    Message(String),
    Complete,
    Error(String),
}
```

The reader task (spawned alongside the child process) reads stdout line by line and
sends parsed events through the channel. This keeps parsing async and non-blocking.

#### `RipHandle` methods

```rust
pub async fn next_event(&mut self) -> Option<RipEvent>
pub fn cancel(&mut self) -> Result<(), MkvError>
```

### DriveInfo / DriveState (keep existing, minor update)

```rust
pub struct DriveInfo {
    pub index: usize,
    pub name: String,
    pub disc_name: Option<String>,
    pub state: DriveState,
}

pub enum DriveState {
    Empty,     // disc_name is None
    Loading,   // flags indicate loading
    Ready,     // disc present, not ripping
    Ripping,   // actively ripping
    Complete,  // rip finished
    Error,     // flags indicate error
}
```

Map from `ParsedDrive`:
- `disc_name == None` → `Empty`
- `enabled && disc_name.is_some()` → `Ready`
- Other combinations → `Error`

---

## 4. CLI Commands (`cli/src/commands/rip.rs`)

### `list_drives`

```rust
async fn list_drives() -> Result<(), anyhow::Error> {
    let client = match MakeMkvClient::new(None) {
        Ok(c) => c,
        Err(MkvError::NotInstalled) => {
            println!("MakeMKV is not installed.");
            println!("Install from: https://www.makemkv.com/forum/viewtopic.php?t=224");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    // ... existing print logic
}
```

### `start_rip`

```rust
async fn start_rip(index: usize) -> Result<(), anyhow::Error> {
    let client = MakeMkvClient::new(None)?;
    let mut handle = client.start_rip(index, ".").await?;

    println!("Ripping started on drive {}", index);

    while let Some(event) = handle.next_event().await {
        match event {
            RipEvent::Progress { current, total, max } => {
                let pct = if max > 0 { (current as f64 / total as f64) * 100.0 } else { 0.0 };
                print!("\rProgress: {:.1}%", pct);
            }
            RipEvent::Complete => break,
            RipEvent::Error(e) => return Err(anyhow::anyhow!(e)),
            _ => {}
        }
    }
    println!("\nRip complete");
    Ok(())
}
```

### `cancel_rip`

```rust
async fn cancel_rip(index: usize) -> Result<(), anyhow::Error> {
    let client = MakeMkvClient::new(None)?;
    // For cancel, we need the existing RipHandle from a running session.
    // Since the CLI is single-threaded, we'll need to store the handle.
    // Approach: use a static Mutex<Option<RipHandle>> or restructure.
    // See "Cancel Strategy" below.
}
```

### `eject_disc`

```rust
async fn eject_disc(index: usize) -> Result<(), anyhow::Error> {
    // Use system eject command: `eject /dev/sr0` or similar
    // makemkvcon doesn't have an eject command
    // Alternative: parse drive device path from detect_drives
    let output = std::process::Command::new("eject")
        .arg(format!("/dev/sr{}", index))
        .output()?;
    // Handle error if eject not available
}
```

### Cancel Strategy

The CLI needs to hold a `RipHandle` across the progress loop. Two options:

**Option A (simple, CLI-only)**: Store `Option<RipHandle>` in a `static Mutex` or `OnceCell`.
Cancel sets it to `None` after killing. Simple but couples to global state.

**Option B (cleaner)**: The `start_rip` function owns the handle and runs the loop.
Cancel is handled by Ctrl+C (SIGINT) which tokio handles natively. The child process
gets killed by dropping the `Child`. This requires no explicit cancel command.

**Recommendation**: Option B. Use `tokio::signal::ctrl_c()` in the progress loop.
Drop the `RipHandle` to kill the process. The `cancel` CLI subcommand becomes
unnecessary or just prints "Press Ctrl+C to cancel". This matches how most rip
tools work.

---

## 5. Web API Changes (`web/src/api.rs`)

### AppState additions

```rust
pub struct AppState {
    pub db: Arc<Database>,
    pub config: Arc<RwLock<Config>>,
    pub sse: SseManager,
    pub mkv_client: Option<Arc<MakeMkvClient>>,  // None if not installed
}
```

The `mkv_client` is constructed once at startup. If MakeMKV isn't installed, it's `None`
and all ripper endpoints return 503 with a helpful message.

### Updated endpoints

#### `GET /api/ripper/drives`

```rust
async fn list_drives(State(state): State<Arc<AppState>>) -> Json<ApiResponse<Vec<DriveInfo>>> {
    let client = match &state.mkv_client {
        Some(c) => c,
        None => return Json(ApiResponse::ok(vec![])),
    };
    match client.detect_drives().await {
        Ok(drives) => Json(ApiResponse::ok(drives)),
        Err(e) => {
            // Return the error info in the response
            Json(ApiResponse::ok(vec![]))
        }
    }
}
```

#### `POST /api/ripper/:index/start`

Spawn the rip as a background task using `JobScheduler`:

```rust
async fn start_rip(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, String)> {
    let client = state.mkv_client.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "MakeMKV not installed".into()))?;

    let dest = state.config.read().await.scan_directory.clone()
        .unwrap_or_else(|| ".".to_string());

    let job_id = state.jobs.start_rip_job(client, index, &dest).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ApiResponse::ok(job_id)))
}
```

This integrates with the `JobScheduler` — the rip runs as a tracked job with
progress events broadcast via SSE.

#### `POST /api/ripper/:index/cancel`

Cancel the running job via `JobScheduler::cancel_job(job_id)`.

### New endpoint: `GET /api/ripper/status`

Returns active rip jobs and their progress, so the frontend can show status
without SSE.

---

## 6. JobScheduler Integration (`app/src/jobs.rs`)

Add a new convenience method:

```rust
pub async fn start_rip_job<F, Fut>(
    &self,
    drive_index: usize,
    destination: String,
    work: F,
) -> Result<String, anyhow::Error>
```

This is just a wrapper around `start_custom_job("Rip")` that the web/CLI layer
provides the actual work closure for. The scheduler doesn't need to know about
MakeMKV directly — it stays in the infra layer.

---

## 7. Files to Create / Modify

### New files
| File | Purpose |
|------|---------|
| `infra/src/makemkv/parser.rs` | Pure parsing functions for makemkvcon output |
| `infra/src/makemkv/parser_tests.rs` | (or inline `#[cfg(test)]` module) |

### Modified files
| File | Changes |
|------|---------|
| `infra/src/makemkv.rs` | Full rewrite: subprocess spawning, RipHandle, error types |
| `infra/src/lib.rs` | Export `MkvError` |
| `infra/src/config.rs` | Add `makemkvcon_path` field |
| `cli/src/commands/rip.rs` | Wire to real client, Ctrl+C cancel, eject |
| `web/src/api.rs` | Add `mkv_client` to AppState, wire endpoints |
| `web/src/lib.rs` | No change (AppState already exported) |
| `app/src/jobs.rs` | Add `start_rip_job` convenience method |

---

## 8. Implementation Order

### Phase 1: Parser (pure, testable, no dependencies)
1. Create `infra/src/makemkv/parser.rs` with all parse functions
2. Write 15+ unit tests with fixture data
3. Verify: `cargo test -p infra`

### Phase 2: MakeMkvClient rewrite
1. Add `MkvError` enum
2. Implement `MakeMkvClient::new()` with availability detection
3. Implement `detect_drives()` using subprocess + parser
4. Implement `start_rip()` returning `RipHandle`
5. Implement progress reading via spawned reader task
6. Implement `cancel_rip()` via child process kill
7. Write tests: parser integration, error paths, mock binary
8. Verify: `cargo clippy --all && cargo test`

### Phase 3: CLI wiring
1. Update `rip.rs` to use `MakeMkvClient::new(None)`
2. Implement graceful "not installed" messages
3. Replace polling loop with `RipHandle::next_event()`
4. Add Ctrl+C cancellation
5. Implement eject (system command)
6. Verify: manual testing with `riply rip list`

### Phase 4: Config + Web wiring
1. Add `makemkvcon_path` to Config
2. Add `mkv_client: Option<Arc<MakeMkvClient>>` to AppState
3. Wire web endpoints to real client
4. Add job scheduler integration for rip jobs
5. Verify: `cargo test --all`

### Phase 5: Polish
1. SSE events for rip progress
2. Error messages are user-friendly
3. Documentation in code comments
4. Final clippy + test pass

---

## 9. Testing Strategy

### Unit tests (no subprocess)
- Parser: parse every line type, edge cases, malformed input
- DriveState mapping from ParsedDrive
- MkvError Display formatting

### Integration tests (mockable)
- `MakeMkvClient` can accept a custom binary path → point at a test script
- Test script outputs fixture data, verify parsed results
- Test cancel kills the child process
- Test error when binary doesn't exist

### Manual tests
- `riply rip list` with real drive (if available)
- `riply rip start 0` with real disc
- Web UI drive listing and rip start

---

## 10. Key MakeMKV CLI Reference

### Essential commands
```bash
# List all drives (disc:9999 = enumerate all)
makemkvcon -r --cache=1 info disc:9999

# Rip all titles from drive 0
makemkvcon -r --cache=1024 --noscan mkv disc:0 all /destination/folder

# Backup with decryption
makemkvcon -r backup --decrypt --cache=16 --noscan disc:0 /destination
```

### Robot mode (`-r`) output format
- Line-based, flushed per line
- All strings quoted, control chars backslash-escaped
- `DRV:index,visible,enabled,flags,"drive_name","disc_name"`
- `PRGV:current,total,max`
- `MSG:code,flags,count,"message","format",param0,...`
- `TCOUT:count` (title count)
- `TINFO:id,code,"value"` (title attributes)

### Build dependencies (Ubuntu/Debian)
```bash
sudo apt-get install build-essential pkg-config libc6-dev libssl-dev \
    libexpat1-dev libavcodec-dev libgl1-mesa-dev qtbase5-dev zlib1g-dev
```

### Binary location
- Default install: `/usr/bin/makemkvcon` (Linux)
- Can be overridden via config `makemkvcon_path`
