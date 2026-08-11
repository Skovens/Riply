# ADR-015: Typed Web Error Type

## Status

Accepted

## Context

ADR-004 defined the two-tier strategy (thiserror in `core`, anyhow
elsewhere) but left web error mapping open: handlers had no single way
to produce a correct HTTP status code. Each handler rolled its own
`(StatusCode, String)` tuples, which drifted.

## Decision

Introduce `WebError` in `web/src/error.rs`:

- Variants: `Internal`, `NotFound`, `BadRequest`, `ServiceUnavailable`
- Constructor helpers: `internal()`, `not_found()`, `bad_request()`,
  `service_unavailable()`
- Implements `IntoResponse`, mapping:
  - Internal -> 500, NotFound -> 404, BadRequest -> 400,
    ServiceUnavailable -> 503

Handlers return `Result<T, WebError>`; domain/anyhow errors are
converted at the layer boundary with a message.

## Consequences

- Consistent, correct status codes across all routes.
- One obvious place to extend (e.g. Unauthorized -> 401) for future auth.
- Conversion boilerplate is localized to each handler boundary.
