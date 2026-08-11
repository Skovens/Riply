# ADR-017: API Versioning

## Status

Accepted

## Context

improvements.md §4.6 identified missing API versioning: breaking changes
would force a major version bump. The API was growing, and the frontend
consumed every endpoint.

## Decision

Prefix all API routes with `/v1/`:

`/v1/files`, `/v1/config`, `/v1/jobs`, `/v1/review`, `/v1/ripper`,
`/v1/scan`, `/v1/search`, `/v1/analyze`, `/v1/move`, `/v1/sse`

The frontend `api` clients target the `/v1/` paths.

## Consequences

- Future breaking changes can land under `/v2/` without breaking
  existing clients.
- The version is explicit in every client call.
- Version prefix is applied per-route rather than via a `nest` - a
  slightly more verbose but grep-able approach.
