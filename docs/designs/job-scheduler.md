# Design: Job Scheduler

> ⚠️ **Note:** Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This document describes the technical implementation details.

## Purpose
Track long-running background operations (scan, analyze, move) with progress reporting and cancellation. This document describes the *current* behavior of the job scheduler system.

---

## Architecture

```
core (domain) → app (JobScheduler) → web (API endpoints)
```

- `JobScheduler` defined in `app/src/jobs.rs` — the orchestration layer
- Used by `web/src/routes/scan.rs`, `web/src/routes/analyze.rs`, `web/src/routes/move.rs`
- Exposed to the frontend via SSE events

---

## Core Types

### JobStatus

```rust
pub enum JobStatus {
    Queued,    // Scheduled but not yet started
    Running,   // Currently executing
    Complete,  // Finished successfully
    Failed,    // Finished with error
    Cancelled, // User requested cancellation
}
```

### JobKind

```rust
pub enum JobKind {
    Scan { directory: String },
    Analyze,
    Move { movies_dir: String, tv_dir: String },
    Custom { name: String },
}
```

### JobProgress

```rust
pub struct JobProgress {
    pub total: usize,     // Total work units (0 if unknown)
    pub processed: usize, // Completed work units
}
```

### JobInfo (public view)

```rust
pub struct JobInfo {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub progress: JobProgress,
}
```

---

## CancellationToken

### Implementation

```rust
pub struct CancellationToken(Arc<AtomicBool>);
```

- **Cooperative cancellation** — the spawned task must check `is_cancelled()` periodically
- **Atomic** — lock-free, safe to share across tasks
- **Cloneable** — multiple references to the same cancellation state

### Methods

```rust
impl CancellationToken {
    pub fn is_cancelled(&self) -> bool;  // Check cancellation flag
    fn cancel(&self);                     // Set cancellation flag
}
```

### Usage Pattern

```rust
scheduler.start_custom_job("my-job", |cancel, _progress| async move {
    loop {
        if cancel.is_cancelled() {
            return Ok(());  // Exit cleanly
        }
        // ... do work ...
    }
}).await
```

---

## ProgressHandle

### Implementation

```rust
pub struct ProgressHandle {
    total: Arc<AtomicUsize>,
    processed: Arc<AtomicUsize>,
}
```

- **Atomic counters** — lock-free, safe to share across tasks
- **Cloneable** — multiple references to the same progress state

### Methods

```rust
impl ProgressHandle {
    pub fn set_total(&self, total: usize);  // Set total work units
    pub fn increment(&self);                 // Increment processed by 1
    fn snapshot(&self) -> JobProgress;       // Get current progress
}
```

### Usage Pattern

```rust
scheduler.start_analyze_job(|_cancel, progress| async move {
    let files = get_files_to_analyze().await;
    progress.set_total(files.len());
    for file in files {
        analyze_file(&file).await;
        progress.increment();
    }
    Ok(())
}).await
```

---

## JobScheduler

### Internal Structure

```rust
pub struct JobScheduler {
    jobs: Arc<RwLock<HashMap<String, JobHandle>>>,
    next_id: AtomicUsize,
}

struct JobHandle {
    kind: JobKind,
    status: JobStatus,
    progress_snapshot: JobProgress,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}
```

### ID Generation

Job IDs are sequential integers starting from 1, converted to strings. Each call to `start_*` increments `next_id` atomically.

### Public API

```rust
impl JobScheduler {
    pub fn new() -> Self;
    pub async fn start_scan_job<F, Fut>(&self, directory: &str, work: F) -> Result<String, anyhow::Error>;
    pub async fn start_analyze_job<F, Fut>(&self, work: F) -> Result<String, anyhow::Error>;
    pub async fn start_move_job<F, Fut>(&self, movies_dir: &str, tv_dir: &str, work: F) -> Result<String, anyhow::Error>;
    pub async fn start_custom_job<F, Fut>(&self, name: &str, work: F) -> Result<String, anyhow::Error>;
    pub async fn cancel_job(&self, job_id: &str) -> Result<(), anyhow::Error>;
    pub async fn get_job_status(&self, job_id: &str) -> Result<JobInfo, anyhow::Error>;
    pub async fn list_jobs(&self) -> Vec<JobInfo>;
    pub async fn cleanup_finished(&self) -> usize;
}
```

### Job Lifecycle

```
start_*() → Queued → (tokio::spawn) → Running → Complete/Failed
                                               ↓
                                            (cancel_job)
                                               ↓
                                            Cancelled
```

### Job Execution

Each `start_*` method:
1. Creates a `CancellationToken` and `ProgressHandle`
2. Creates a `JobHandle` with status `Queued`
3. Spawns a tokio task that:
   a. Sets status to `Running`
   b. Executes the work closure
   c. Sets status to `Complete` (success) or `Failed` (error)
4. Returns the job ID as a string

### Cancellation

`cancel_job(job_id)`:
1. Looks up the job by ID
2. If already terminal (Complete/Failed/Cancelled), returns OK (no-op)
3. Sets the cancellation flag
4. Aborts the tokio task handle
5. Sets status to `Cancelled`
6. Resets progress to `{ total: 0, processed: 0 }`

### Cleanup

`cleanup_finished()`:
1. Removes all jobs with terminal status (Complete/Failed/Cancelled)
2. Returns the number of jobs removed
3. Prevents unbounded memory growth from old job history

---

## API Endpoints

### Scan

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/scan` | `scan_directory` | Start scan job |

**Request:**
```json
{ "directory": "/path/to/scan" }
```

**Response:**
```json
{
    "ok": true,
    "data": { "job_id": "1" }
}
```

### Analyze

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/analyze` | `analyze_files` | Start analyze job |

**Response:**
```json
{
    "ok": true,
    "data": { "job_id": "2" }
}
```

### Move

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/move/execute` | `execute_move` | Start move job |

**Response:**
```json
{
    "ok": true,
    "data": { "job_id": "3" }
}
```

### Jobs

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/jobs` | `list_jobs` | List all jobs |
| GET | `/v1/jobs/:id` | `get_job` | Get job by ID |
| POST | `/v1/jobs/:id/cancel` | `cancel_job` | Cancel a job |

---

## SSE Events

### Job Events

| Event | Format | Triggered By |
|---|---|---|
| `scan:starting` | `scan:starting:{job_id}` | Job starts |
| `scan:progress` | `scan:progress:{job_id}:{processed}:{total}` | Progress update |
| `scan:complete` | `scan:complete:{job_id}` | Job succeeds |
| `scan:cancelled` | `scan:cancelled:{job_id}` | Job cancelled |
| `scan:failed` | `scan:failed:{job_id}:{error}` | Job fails |

Same pattern for `analyze` and `move` events.

### Event Prefixes

| Prefix | Used By |
|---|---|
| `scan:` | `start_scan_job()` |
| `analyze:` | `start_analyze_job()` |
| `move:` | `start_move_job()` |
| `custom:` | `start_custom_job()` |

### SSE Broadcast Pattern

```rust
// In the job closure:
sse.send_status(&format!("scan:starting:{}", job_id));
sse.send_status(&format!("scan:progress:{}:{}", job_id, processed));
// On success:
sse.send_status(&format!("scan:complete:{}", job_id));
// On error:
sse.send_status(&format!("scan:failed:{}:{}", job_id, error));
// On cancel:
sse.send_status(&format!("scan:cancelled:{}", job_id));
```

---

## Integration Points

### Scan Job

```rust
// web/src/routes/scan.rs
let job_id = scheduler.start_scan_job(&config.scan_directory, move |cancel, progress| {
    let scanner = ScannerService::new();
    let searcher = state.searcher.as_ref().map(|s| s.as_ref());
    async move {
        sse.send_status(&format!("scan:starting:{}", job_id));

        let scan_result = scanner.scan_directory(&path, searcher).await
            .map_err(|e| anyhow::anyhow!("Scan failed: {}", e))?;

        // Save records to database
        for record in scan_result.records {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("Scan cancelled"));
            }
            db.upsert_file(&record).await
                .map_err(|e| anyhow::anyhow!("DB error: {}", e))?;
            progress.increment();
        }

        sse.send_status(&format!("scan:complete:{}", job_id));
        Ok(())
    }
}).await?;
```

### Analyze Job

```rust
// web/src/routes/analyze.rs
let job_id = scheduler.start_analyze_job(move |cancel, progress| {
    let analyzer = AnalyzerService::new(state.searcher.clone());
    async move {
        sse.send_status(&format!("analyze:starting:{}", job_id));

        let files = db.get_files_by_status("Scanned").await?;
        progress.set_total(files.len());

        for file in files {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("Analyze cancelled"));
            }

            let analyzed = analyzer.analyze_file(&file).await
                .map_err(|e| anyhow::anyhow!("Analysis failed: {}", e))?;

            db.update_file(&analyzed).await?;
            progress.increment();
        }

        sse.send_status(&format!("analyze:complete:{}", job_id));
        Ok(())
    }
}).await?;
```

### Move Job

```rust
// web/src/routes/move.rs
let job_id = scheduler.start_move_job(&movies_dir, &tv_dir, move |cancel, progress| {
    let mover = MoverService::new();
    async move {
        sse.send_status(&format!("move:starting:{}", job_id));

        let confirmed = db.get_files_by_status("Confirmed").await?;
        progress.set_total(confirmed.len());

        for file in confirmed {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("Move cancelled"));
            }

            mover.move_file(&file, &movies_dir, &tv_dir).await?;
            db.update_file(&updated).await?;
            progress.increment();
        }

        sse.send_status(&format!("move:complete:{}", job_id));
        Ok(())
    }
}).await?;
```

---

## Frontend Display

### Jobs Panel (`+page.svelte`)

**Purpose:** Show active and recent jobs with progress and cancellation.

**Display:**
- Job kind (Scan, Analyze, Move) with details
- Status badge (Queued, Running, Complete, Failed, Cancelled)
- Progress bar (`processed / total`)
- Cancel button (visible when Running)

**SSE handling:**
| Event | Action |
|---|---|
| `scan:starting` | Add job to list, set status to Running |
| `scan:progress` | Update progress bar |
| `scan:complete` | Set status to Complete, hide cancel button |
| `scan:cancelled` | Set status to Cancelled |
| `scan:failed` | Set status to Failed, show error message |

---

## Error Handling

| Scenario | Behavior |
|---|---|
| Job closure returns `Err` | Status set to `Failed`, error logged |
| Job closure panics | Task aborted, status not updated (JoinError) |
| Cancel already-complete job | Returns `Ok(())` — no-op |
| Cancel nonexistent job | Returns `Err` — job not found |
| Cancel running job | Sets flag, aborts task, sets status to `Cancelled` |

---

## Where This Lives

| Concern | Layer |
|---|---|
| CancellationToken | `app/src/jobs.rs` |
| ProgressHandle | `app/src/jobs.rs` |
| JobScheduler | `app/src/jobs.rs` |
| JobInfo / JobKind / JobStatus | `app/src/jobs.rs` |
| Scan API | `web/src/routes/scan.rs` |
| Analyze API | `web/src/routes/analyze.rs` |
| Move API | `web/src/routes/move.rs` |
| Jobs API | `web/src/routes/jobs.rs` |
| Dashboard UI | `web/svelte/src/routes/+page.svelte` |

---

## Notes

- **Cooperative cancellation** — the task must check `is_cancelled()` periodically; the scheduler cannot forcibly stop arbitrary code
- **Task abort is best-effort** — `handle.abort()` cancels the tokio task but the closure may not exit immediately
- **Progress is atomic** — no locks needed for progress updates
- **Jobs are in-memory only** — lost on server restart (no persistence)
- **Cleanup is manual** — `cleanup_finished()` must be called periodically to remove completed jobs
- **IDs are sequential** — start at 1, increment per job (not UUIDs)
- **SSE events are fire-and-forget** — no acknowledgment from clients
