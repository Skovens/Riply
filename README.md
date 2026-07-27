# Riply

Rip, manage, and move media files into a structured library. Scan directories for movies and TV shows, look up metadata from TMDb/OMDb, review matches, and organize them into your library — all from the CLI or a web dashboard.

## Features

- **Media scanning** — Walk directories, hash files, detect media types
- **Metadata matching** — Parse filenames, search TMDb and OMDb APIs
- **Review queue** — Confirm or reject matches before moving
- **Library organization** — Move confirmed files into structured `movies/` and `tv/` directories
- **Disc ripping** — MakeMKV integration with auto-detection (native install or Flatpak)
- **Job scheduler** — Background jobs with progress tracking and cancellation
- **Web dashboard** — Real-time UI with Server-Sent Events
- **REST API** — Full programmatic access to all operations

## Architecture

```
core (domain) → app (orchestration) → infra (I/O) → web/cli (interfaces)
```

| Crate | Role |
|-------|------|
| `core` | Domain types, traits, business rules. Zero infrastructure deps. |
| `app` | Services, job scheduler, orchestration. Depends only on `core`. |
| `infra` | SQLite database, HTTP clients, MakeMKV backend. |
| `web` | Axum HTTP API + embedded frontend (vanilla JS). |
| `cli` | CLI binary with clap subcommands. |

## Quick Start

```bash
git clone https://github.com/Skovens/Riply.git
cd Riply
cargo build --all
cargo run -p cli -- serve
```

Open `http://localhost:3000` in your browser.

## CLI Usage

```bash
# Scan a directory for media files
riply scan /path/to/downloads

# Analyze scanned files (metadata lookup)
riply analyze

# Review files that need manual confirmation
riply review

# Move confirmed files to your library
riply move

# Disc ripping
riply rip drives          # List available drives
riply rip start 0         # Start ripping from drive 0
riply rip cancel 0        # Cancel active rip

# Start the web server
riply serve
riply serve --port 8080

# Show pipeline status
riply status
```

## Configuration

Riply uses a TOML config file (`config.toml` by default):

```toml
database_path = "riply.db"
movies_directory = "movies"
tv_directory = "tv"
host = "0.0.0.0"
port = 3000

[ripping]
command = "makemkvcon"   # or "flatpak run com.github.riply"
```

Set API keys via the config file or environment variables for metadata lookup:
- `tmdb_api_key` — TMDb v3 API key
- `omdb_api_key` — OMDb API key

## Development

```bash
cargo fmt
cargo clippy --all
cargo test --all
```

Quick check: `cargo check`

The `core` crate is imported as `riply_core` (not `core`) to avoid conflicting with Rust's std `core` crate.

### Dev Workflow (auto-rebuild)

Run this in one terminal — it watches both Rust and frontend files, rebuilding
everything on any change:

```bash
cargo install cargo-watch   # one-time install
cargo-watch -w cli/src -w web/src -w web/svelte/src -- bash scripts/watch.sh
```

This rebuilds the frontend (~0.5s) and starts the server on every change.

**Alternative — separate processes** (frontend HMR on port 5173):

```bash
# Terminal 1 — Rust backend
cargo watch -x 'run -p cli -- serve'

# Terminal 2 — Svelte frontend (hot reload)
cd web/svelte
npm install                 # one-time install
npm run dev
```

Open `http://localhost:5173` for frontend HMR. When done, run `npm run build`
to bake the frontend into the Rust binary for production.

## Agent Instructions

See [AGENTS.md](AGENTS.md) for coding conventions, build commands, and agent task board.

## License

MIT
