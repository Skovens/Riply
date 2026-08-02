# ADR-008: Logging and Observability

## Status

Accepted

## Context

Riply needs logging for:
- Debugging scan/analyze/move operations
- Monitoring job progress and failures
- HTTP request/response tracking
- MakeMKV subprocess errors
- Configuration errors

The logging must be:
- Configurable at runtime (via environment variable)
- Structured enough for log aggregation (future)
- Minimal overhead in production

Options considered:
1. **tracing** + **tracing-subscriber** — Industry standard, structured, async-safe
2. **log** + **env_logger** — Simpler, but less structured
3. **slog** — Structured logging, but smaller ecosystem
4. **Custom logger** — Full control, high maintenance

## Decision

Use **tracing** for instrumentation with **tracing-subscriber** for output.

Configuration:
- `EnvFilter` for log level control via `RUST_LOG` environment variable
- Format layer for human-readable console output
- Default filter: `riply=info,tower_http=debug`

Usage pattern:
- `#[instrument]` attribute on public functions for automatic span creation
- Fields added to spans for context (job IDs, file paths, status codes)
- `tracing::info!`, `tracing::warn!`, `tracing::error!` macros for logging

HTTP request logging via `tower_http::TraceLayer` (recommended in improvements.md, not yet implemented).

## Consequences

### Positive
- Structured logs with contextual fields
- Environment-variable-based log level control
- `#[instrument]` reduces boilerplate
- Compatible with OpenTelemetry exporters (future)
- Zero-cost when tracing is disabled in release builds

### Negative
- `tracing` adds compile-time dependencies
- No built-in log rotation — would need `tracing-appender` or external log management
- HTTP request logging not yet implemented (per improvements.md)

### Future Considerations
Consider adding:
- Health/readiness endpoints (improvements.md)
- Metrics endpoint with Prometheus (improvements.md)
- Structured JSON log output for production deployments
