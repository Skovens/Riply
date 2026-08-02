# ADR-004: Error Handling Strategy

## Status

Accepted

## Context

Riply operates across multiple layers with different error handling needs:
- **Domain layer** (`core`): Errors are part of the type system (invalid states, validation failures)
- **Application layer** (`app`): Errors represent operational failures (scan failures, API errors)
- **Infrastructure layer** (`infra`): Errors are I/O failures (database, HTTP, process execution)
- **Interface layers** (`web`, `cli`): Errors need user-facing messages and HTTP status codes

## Decision

Use a **two-tier error handling strategy**:

### `core` — `thiserror` for domain errors
- Define specific error enums with `thiserror` for each module
- Errors represent invalid domain states or validation failures
- Implement `std::error::Error`, `Display`, and `Debug`
- Example: `FileValidationError`, `MetadataLookupError`

### `app`, `infra`, `cli` — `anyhow` for operational errors
- Use `anyhow::Result<T>` for functions that can fail in many ways
- Context chaining with `?` and `.context("description")`
- Convert domain errors to anyhow at layer boundaries
- Example: `scan_directory().context("failed to scan {path}")`

### `web` — Typed HTTP errors
- Custom `WebError` enum (recommended in improvements.md, not yet implemented)
- Implements `IntoResponse` for automatic status code mapping
- Maps internal errors to 500, not-found to 404, validation to 400

## Consequences

### Positive
- Clear separation between domain errors (typed) and operational errors (anyhow)
- `thiserror` keeps dependency footprint minimal in `core`
- `anyhow` reduces boilerplate in application code
- Context messages aid debugging without explicit error types

### Negative
- `anyhow` erases error types — debugging requires context inspection
- No unified error type across layers (intentional, but adds complexity)
- Web error handling not yet fully implemented (see improvements.md)

### Future Considerations
Consider adding a top-level `RiplyError` enum that wraps all error types for consistent error reporting and serialization, especially if external API consumers are added.
