# ADR-006: HTTP Framework Selection

## Status

Accepted

## Context

Riply's web server needs to serve:
- REST API endpoints for all operations (scan, analyze, review, move, rip, jobs, config)
- Static files (HTML, CSS, JS) embedded in the binary
- Server-Sent Events (SSE) for real-time job progress
- File upload/download capabilities

Options considered:
1. **Axum** — Built on Tower, ergonomic routing, integrates with Tokio ecosystem
2. **Actix-web** — High performance, larger ecosystem, more complex
3. **Warp** — Composable filters, being superseded by Axum
4. **Hyper** — Low-level, maximum control but boilerplate-heavy

## Decision

Use **Axum** (version 0.7+) as the HTTP framework.

Key features used:
- Router with `.route()` and `.merge()` for composable route definitions
- Extractors for request parsing (JSON, form data, path params)
- `State` layer for sharing application state across handlers
- SSE support via `axum::response::sse`
- Static file serving via `ServeDir` or manual `include_str!` embedding

## Consequences

### Positive
- Ergonomic, type-safe routing with compile-time extractor validation
- Built on Tower — access to middleware ecosystem
- Excellent integration with Tokio and async Rust patterns
- `State` layer provides clean dependency injection
- Axum is actively maintained by Tokio team

### Negative
- Axum 0.7 uses `:param` syntax (not `{param}`) — known gotcha documented in AGENTS.md
- Single monolithic router (`api.rs`) — recommended to split into domain modules (per improvements.md)
- No built-in API versioning — must be added manually

### Future Considerations
Per improvements.md, the router should be split into per-domain modules (files, review, scan, etc.) for better maintainability. Consider API versioning (`/api/v1/...`) before reaching a stable release.
