# cli

CLI binary entry point with clap subcommands. This is the `riply` binary.

## Subcommands

- `scan <dir>` — Scan directory for media files
- `analyze` — Analyze scanned files (metadata lookup)
- `review [--yolo]` — Review files that need manual confirmation
- `move` — Move confirmed files to organized library
- `rip drives|start|cancel` — Disc ripping with MakeMKV
- `serve [--port]` — Start the web API server
- `status` — Show current pipeline status

## Testing

```bash
cargo test -p cli
```

## Patterns

- Uses `clap` derive macros.
- Commands delegate to `app` services.
- Config loaded from `config.toml` (or `--config` flag).
