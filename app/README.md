# app

Service orchestration and job scheduling. Depends only on `core`.

## Key Types

- `ScannerService` — Walk directories, hash files, produce `FileRecord`s
- `AnalyzerService` — Parse filenames, search metadata APIs
- `MoverService` — Calculate destination paths, move files
- `JobScheduler` — Background job management with progress tracking
- `JobInfo` / `JobStatus` / `JobKind` / `JobProgress` — Job state types
- `ProgressHandle` / `CancellationToken` — Job control primitives

## Testing

```bash
cargo test -p app
```

## Patterns

- Services are zero-sized structs with static methods.
- Use `anyhow::Result` for errors.
- Job closures use `tokio::spawn` and receive a `CancellationToken` + `ProgressHandle`.
