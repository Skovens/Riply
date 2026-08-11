# ADR-014: Router Splitting (Per-Domain Modules)

## Status

Accepted

## Context

ADR-006 noted the monolithic router in `web/src/api.rs` as a negative
consequence of choosing Axum. As the API grew (files, review, scan, rip,
jobs, config, search, SSE), a single file became hard to navigate and
test. Domain modules were needed.

## Decision

Split the router into per-domain modules under `web/src/routes/`, each
exposing a `router()` function that is merged into the root router in
`web/src/api.rs`.

Modules: `analyze`, `config`, `files`, `jobs`, `move_`, `review`,
`ripper`, `scan`, `search`, `sse_handler` - composed with 11 `.merge()`
calls in `api.rs`. Shared concerns (`AppState`, `WebError`) live at the
crate root.

## Consequences

- Each domain owns its routes, extractors, and tests in one place.
- New endpoints are added by touching one module, not the root router.
- `api.rs` is now only composition, layers, and health/readiness routes.
- The `move_` module is named with a trailing underscore because `move`
  is a Rust keyword.
