---
name: job-system
description: >
  Background job orchestration for Riply. Covers the JobScheduler with
  cancellation and progress tracking, the three core services
  (Scanner, Analyzer, Mover), SSE status broadcasting, and the Axum
  route handlers that wire everything together.
metadata:
  language: rust
  version: "1.0+"
  compatibility: tokio, axum, tracing, serde, atomic
---

# Job System

## Architecture Overview

Jobs are long-running background tasks managed by `JobScheduler`. Every
web operation that takes more than a few hundred milliseconds runs as a
job. The CLI runs the same operations synchronously (no scheduler).

```
web request -> JobScheduler -> tokio::spawn -> work closure -> SSE broadcast
```

### Key Types (from `app::jobs`)

| Type | Module | Purpose |
|------|--------|---------|
| `JobScheduler` | `app::jobs` | Registry + launcher for background jobs |
| `JobInfo` | `app::jobs` | Public read-only view of a job |
| `JobKind` | `app::jobs` | Discriminator: Scan, Analyze, Move, Custom |
| `JobStatus` | `app::jobs` | Queued -> Running -> Complete/Failed/Cancelled |
| `JobProgress` | `app::jobs` | `{ total, processed }` counters |
| `CancellationToken` | `app::jobs` | Cooperative cancellation via `Arc<AtomicBool>` |
| `ProgressHandle` | `app::jobs` | Shared handle to report progress (`set_total`, `increment`) |

### Module Location

```
app/src/jobs.rs          - JobScheduler, JobInfo, JobKind, JobStatus, CancellationToken, ProgressHandle
app/src/scanner.rs       - ScannerService (directory scanning + SHA-256 hashing)
app/src/analyzer.rs      - AnalyzerService (filename parsing + metadata search)
app/src/mover.rs         - MoverService (file relocation with cross-device fallback)
web/src/routes/jobs.rs   - REST endpoints for job management
web/src/routes/scan.rs   - Start scan job
web/src/routes/analyze.rs - Start analyze job + retry-failed
web/src/routes/move_.rs  - Start move job + preview + status
web/src/sse.rs           - SseManager (broadcast channel for status updates)
```

## Starting Jobs

Every job is started through a typed method on `JobScheduler`. The
scheduler lives in `AppState.scheduler` (an `Arc<JobScheduler>`).

### Pattern

```rust
let job_id = state
    .scheduler
    .start_<kind>_job("<param>", move |cancel, progress| {
        // Clone everything you need before the closure
        let db = db.clone();
        let sse = sse.clone();
        async move {
            // 1. Set total work units
            progress.set_total(count);

            // 2. Broadcast start event
            sse.send_status(&format!("scan:starting:{}", count));

            // 3. Loop over work, checking cancellation
            for (i, item) in items.iter().enumerate() {
                if cancel.is_cancelled() {
                    sse.send_status("scan:cancelled");
                    return Ok(());
                }

                // Do the actual work ...

                // 4. Report progress
                progress.increment();

                // 5. Periodic status updates (every 10 items or last)
                if (i + 1) % 10 == 0 || i + 1 == items.len() {
                    sse.send_status(&format!("scan:progress:{}:{}", i + 1, count));
                }
            }

            // 6. Final status
            sse.send_status(&format!("scan:complete:{}", processed));
            Ok(())
        }
    })
    .await?;
```

### Available Start Methods

| Method | Parameters | JobKind |
|--------|-----------|---------|
| `start_scan_job(dir, work)` | directory path | `JobKind::Scan { directory }` |
| `start_analyze_job(work)` | - | `JobKind::Analyze` |
| `start_move_job(movies_dir, tv_dir, work)` | two directory paths | `JobKind::Move { movies_dir, tv_dir }` |
| `start_custom_job(name, work)` | arbitrary name | `JobKind::Custom { name }` |

All methods return `Result<String, anyhow::Error>` - the `String` is the
job ID (format: `"job_N"` where N is an incrementing counter).

## Job Lifecycle

```
Queued --tokio::spawn--> Running --work returns Ok--> Complete
                                  |
                                  +-work returns Err--> Failed
                                  |
                                  +-cancel_job called--> Cancelled
```

### State Transitions

- **Queued -> Running**: Happens automatically inside the spawned task
  (first thing the closure does).
- **Running -> Complete**: The work closure returned `Ok(())`.
- **Running -> Failed**: The work closure returned `Err(...)`. The error
  is logged via `tracing::error!`.
- **Running -> Cancelled**: `cancel_job()` was called. The `JoinHandle`
  is aborted and the cooperative cancellation flag is set.
- **Complete/Failed/Cancelled -> removed**: `cleanup_finished()` removes
  them from the registry.

### Idempotent Cancel

Calling `cancel_job()` on an already-terminal job is a no-op (returns
`Ok(())`, does not error). This makes callers safe to call cancel
without knowing the job's current state.

## SSE Status Broadcasting

The `SseManager` uses a `tokio::sync::broadcast` channel (default
capacity 256). All job status updates flow through it.

### Event Format

```
event:{name}\ndata:{payload}
```

Three event names:

| Event | Method | Example |
|-------|--------|---------|
| `status` | `send_status(msg)` | `event:status\ndata:scan:progress:5:10` |
| `progress` | `send_progress(msg)` | `event:progress\ndata:scan:progress:5:10` |
| `log` | `send_log(level, msg)` | `event:log\ndata:warn:Failed to save file.mkv` |

### Status Message Convention

All status messages follow `"{operation}:{event}:{details}"`:

| Message | Meaning |
|---------|---------|
| `scan:starting:{count}` | Scan job started, total files |
| `scan:progress:{i}:{total}` | After every ~10 files |
| `scan:complete:{count}` | Scan finished, files found |
| `scan:cancelled` | User cancelled the scan |
| `analyze:starting:{count}` | Analysis started |
| `analyze:progress:{i}:{total}` | Progress tick |
| `analyze:complete:matched={N},review={N}` | Analysis done with counts |
| `analyze:cancelled` | Cancelled |
| `move:starting:{count}` | Move started |
| `move:progress:{i}:{total}` | Progress tick |
| `move:complete:moved={N},errors={N}` | Move done with summary |
| `move:cancelled` | Cancelled |

The `SseManager::message_to_event()` helper parses these into Axum
SSE `Event` objects.

## Core Services

### ScannerService (`app::ScannerService`)

Scans a directory recursively for media files (.mkv, .mp4, .avi, .mov,
.m4v, .webm). Each file is:

1. **Hashed** with SHA-256 (via `tokio::task::spawn_blocking` to avoid
   blocking the async runtime)
2. **Identified** by ID = first 16 hex chars of the hash
3. **Recorded** as `FileRecord` with path, filename, hash, size

```rust
let scanner = ScannerService::new();
let records = scanner.scan_directory(&path).await?;
```

Use `ScannerService::hash_file_async(&path)` for standalone hashing.

### AnalyzerService (`app::AnalyzerService`)

Analyzes a `FileRecord` to identify the media and assign a pipeline
status.

```rust
let searcher: Option<Box<dyn MetadataSearcher>> = /* from config */;
let analyzer = AnalyzerService::new(searcher);
let updated = analyzer.analyze_file(&record).await?;
```

**Analysis flow:**
1. Extract ffprobe metadata (duration, content type) - skips gracefully
   if ffprobe unavailable
2. Parse filename for title, year, TV episode markers (SxxExx, NxNN)
3. If a `MetadataSearcher` is configured, try TMDb/OMDb lookup
   - Success -> `Matched`, confidence 0.95, source = "tmdb"
   - Failure -> fall through to filename-only
4. Filename-only heuristic scoring:
   - Title > 3 chars: +0.5
   - Year present: +0.3
   - Extension present: +0.2
   - TV episode from filename: minimum confidence 0.85
5. Status assignment: confidence >= 0.8 -> `Matched`, else `NeedsReview`

### MoverService (`app::MoverService`)

Moves confirmed files to an organized library structure.

```rust
let mover = MoverService::new();
let updated = mover.move_file(&record, &movies_dir, &tv_dir).await?;
```

**Move logic:**
- Movies -> `movies_dir/Title (Year)/filename`
- TV -> `tv_dir/Series Name/Season XX/filename`
- Creates parent directories automatically
- Cross-device fallback: copy + verify size + delete source
- Returns error if destination already exists
- Updates record status to `Moved`

Use `preview_move()` to compute the destination without moving.

## Axum Route Handlers

### Jobs Management (`web/src/routes/jobs.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/jobs` | GET | List all jobs |
| `/api/jobs/:id` | GET | Get single job info |
| `/api/jobs/:id/cancel` | POST | Cancel a job |

Response shape:
```json
{
  "success": true,
  "data": {
    "id": "job_3",
    "kind": "Scan (/mnt/media)",
    "status": "Running",
    "progress_total": 150,
    "progress_processed": 42
  }
}
```

### Scan (`web/src/routes/scan.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/scan` | POST | Start scan job |

Request body: `{ "directory": "/path/to/scan" }`
Response: `{ "job_id": "job_4", "status": "Queued" }`

Validates directory existence before queuing.

### Analyze (`web/src/routes/analyze.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/analyze` | POST | Start analyze job |
| `/api/analyze/retry-failed` | POST | Reset failed -> scanned, then analyze |

Both endpoints read API keys from config to decide whether to use
external metadata searchers.

### Move (`web/src/routes/move_.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/move` | POST | Start move job |
| `/api/move/status` | GET | Count confirmed vs moved |
| `/api/move/preview` | GET | Show where files would go |

Request body (POST): `{ "movies_dir": "...", "tv_dir": "..." }` - both
optional, defaults to config values.

## AppState Integration

The `JobScheduler` is created once in `AppState` (in
`cli/src/commands/serve.rs`):

```rust
let state = Arc::new(AppState {
    // ...
    scheduler: JobScheduler::new(),
    sse: SseManager::new(256),
    // ...
});
```

All route handlers receive `State(Arc<AppState>)` and access the
scheduler via `state.scheduler`.

## Testing Patterns

### JobScheduler Tests

Use `#[tokio::test]` with a brief `tokio::time::sleep` to let the
spawned task complete:

```rust
#[tokio::test]
async fn test_start_and_complete_job() {
    let scheduler = JobScheduler::new();
    let id = scheduler
        .start_custom_job("test", |_cancel, _progress| async { Ok(()) })
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let info = scheduler.get_job_status(&id).await.unwrap();
    assert_eq!(info.status, JobStatus::Complete);
}
```

### Service Tests

Use `tempfile::tempdir()` for temporary directories and inline mock
implementations for traits:

```rust
struct MockSearcher;
impl riply_core::MetadataSearcher for MockSearcher {
    fn search_movies(&self, _query: &str, _year: Option<u32>)
        -> Pin<Box<dyn Future<Output = Result<Vec<riply_core::MetadataMatch>, anyhow::Error>> + Send + '_>>
    {
        Box::pin(async move { Ok(vec![/* mock data */]) })
    }
    fn search_tv(&self, _query: &str)
        -> Pin<Box<dyn Future<Output = Result<Vec<riply_core::MetadataMatch>, anyhow::Error>> + Send + '_>>
    {
        Box::pin(async move { Ok(vec![]) })
    }
}
```

### Cancelling a Running Job

The test spins up a job that loops checking `cancel.is_cancelled()`:

```rust
let id = scheduler
    .start_custom_job("test", |cancel, _progress| async move {
        loop {
            if cancel.is_cancelled() { return Ok(()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

tokio::time::sleep(Duration::from_millis(20)).await;
scheduler.cancel_job(&id).await.unwrap();

let info = scheduler.get_job_status(&id).await.unwrap();
assert_eq!(info.status, JobStatus::Cancelled);
```

## Common Pitfalls

1. **Clone before moving into closure** - The closure takes `+ 'static`
   bound. Clone `db`, `sse`, `config`, etc. before the `move`.

2. **Don't forget cancellation checks** - Every loop iteration in a
   job closure should check `cancel.is_cancelled()` and return early.

3. **Set total before incrementing** - `ProgressHandle::set_total()`
   must be called before `increment()` calls, otherwise progress shows
   0 total.

4. **`JobScheduler` is `Clone`** - It's built on `Arc<RwLock<...>>`,
   so cloning is cheap. Pass it around freely.

5. **`cleanup_finished()` removes Complete/Failed/Cancelled** - If you
   need to inspect finished jobs, query them before cleanup.

6. **CLI does NOT use JobScheduler** - The CLI commands in
   `cli/src/commands/` run services directly (no scheduler, no SSE).
   Only the web server (`serve` command) uses the job system.

7. **Progress updates are best-effort** - SSE broadcast capacity is
   finite (256). If the channel is full, messages are silently dropped.
   Don't rely on every single progress tick arriving.
