# infra

I/O adapters — database, HTTP clients, MakeMKV integration.

## Key Types

- `Database` — SQLite persistence (save, query, update file records)
- `HttpClient` — TMDb v3 and OMDb API client
- `Config` / `RippingConfig` — TOML configuration management
- `MakemkvBackend` / `MakemkvSession` — MakeMKV CLI integration
- `DryRunBackend` — Testing backend that simulates ripping

## Testing

```bash
cargo test -p infra
```

## Patterns

- Uses `anyhow::Result` for errors.
- Database uses `sqlx` with WAL journal mode and `create_if_missing`.
- HTTP client uses `reqwest` with `rustls-tls`.
- MakeMKV parses robot-mode output. Auto-detects native install, common paths, and Flatpak.
