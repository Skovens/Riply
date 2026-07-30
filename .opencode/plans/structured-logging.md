# Structured Logging for AI-Parseable Output

## Problem

Logs are human-readable but AI-unfriendly:
- All values are string-interpolated into messages — AI must regex-extract
- No JSON output option for programmatic ingestion
- Log level hardcoded via `--verbose` flag, no `RUST_LOG` support
- HTTP handler spans missing (no request-scoped context)
- `#[instrument]` spans `skip(self)` losing key context
- Inconsistent error levels (errors logged as `info!`)

## Goal

Every log line emitted as structured key=value pairs, optionally as JSON,
so an AI agent (or any tool) can parse and understand the system state
without human interpretation.

## Changes

### 1. Subscriber: `RUST_LOG` + `--log-format`

**File:** `cli/src/main.rs`

```rust
// Replace current subscriber init with:
let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| if cli.verbose { EnvFilter::new("debug") } else { EnvFilter::new("info") });

match cli.log_format {
    LogFormat::Json => fmt().json().with_env_filter(env_filter).init(),
    LogFormat::Pretty => fmt().pretty().with_env_filter(env_filter).init(),
    LogFormat::Compact => fmt().compact().with_env_filter(env_filter).init(),
}
```

New CLI flag: `--log-format <json|pretty|compact>` (default: compact).
Uses `tracing-subscriber`'s `env-filter` feature (already a dep).

### 2. Structured fields on all tracing calls

Every `info!`, `debug!`, `warn!`, `error!` call converts to structured fields:

```rust
// Before
info!("Moving file: {}", path);
// After
info!(file = %path, "Moving file");
```

This applies to ~85 calls across:
- `cli/src/commands/` (scan, analyze, review, mover, rip, status, serve)
- `app/src/` (scanner, analyzer, jobs, mover, ffprobe)
- `infra/src/` (db, config, http, healthchecks, makemkv)
- `web/src/` (api, sse, routes/*)

### 3. `#[instrument]` field capture

Add explicit fields to instrument spans so key args appear in every child log:

```rust
// Before
#[instrument(skip(self))]
// After
#[instrument(skip(self), fields(path = %path))]
```

### 4. HTTP TraceLayer fields

Add request ID, method, URI to every HTTP span so web logs carry request context.

### 5. Consistent log levels

| Current | Target | Pattern |
|---------|--------|---------|
| `info!(..., "Cannot read dir")` | `warn!` | Expected failures |
| `info!(..., "Job failed")` | `error!` | Unexpected failures |
| `info!(..., "Destination exists")` | `warn!` | Expected conflict |
| `warn!(..., "Preview move failed")` | `error!` | Operation failure |
| No `trace!` usage | Add `trace!` | Per-line diagnostics |

## Scope

~85 tracing call sites + 20 `#[instrument]` sites + subscriber init.
All in the Rust backend — no frontend changes.

## Non-goals

- Changing user-facing `println!` output (scan results, status, etc.)
- Changing the logging crate (staying with `tracing`)
- Adding application-level request IDs (future work)
