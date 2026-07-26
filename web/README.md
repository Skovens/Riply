# web

Axum HTTP API + embedded frontend.

## Key Types

- `AppState` — Shared application state (db, config, backend, scheduler, SSE)
- `create_router` — Builds the Axum router with all endpoints
- `SseManager` — Server-Sent Events broadcast channel

## Testing

```bash
cargo test -p web
```

## Patterns

- API returns `Json<ApiResponse<T>>` with `success`, `data`, `error` fields.
- SSE events: `event:status`, `event:progress`, `event:log`.
- Frontend is embedded via `include_str!` (compiled into the binary).
- This crate is library-only — the binary is in `cli`.
