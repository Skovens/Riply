# Riply

A modern, type-safe media library organizer built in Rust with a SvelteKit frontend. Riply scans directories, identifies movies and TV shows using TMDb/OMDB, and organizes them into clean, standardized library structures.

> **Rebuild of [MediaAtlas](https://github.com/Skovens/MediaAtlas)** — prioritizing memory safety, concurrency, testability, and clean architecture over Python's GIL bottlenecks.

## Architecture

```
media-atlas/
├── core/                  # Pure domain logic (no I/O, fully testable)
│   ├── pipeline.rs        # State machine, status transitions
│   ├── confidence.rs      # Scoring algorithms, thresholds
│   ├── filename.rs        # Parsing, cleaning, TV/movie detection
│   ├── paths.rs           # Destination path generation, extras detection
│   ├── file_record.rs     # File metadata model
│   ├── media_type.rs      # Movie vs TV types
│   ├── metadata.rs        # Media metadata handling
│   └── ripping.rs         # DVD ripping state machine
├── app/                   # Application services (orchestration)
│   ├── scanner.rs         # File discovery, hashing, metadata extraction
│   ├── analyzer.rs        # TMDb/OMDB search, episode matching, review workflow
│   ├── mover.rs           # File organization, duplicate handling
│   └── jobs.rs            # Background task scheduler, progress channels
├── infra/                 # Infrastructure adapters (I/O heavy)
│   ├── db.rs              # SQLite + sqlx (migrations, queries, WAL mode)
│   ├── http.rs            # reqwest with retry/rate-limiting, SSE endpoints
│   ├── makemkv.rs         # Async CLI wrapper, progress parsing, error recovery
│   ├── fs.rs              # File operations, safe moves, temp cleanup
│   └── config.rs          # TOML settings, validation, path resolution
├── web/                   # Presentation layer
│   ├── api/               # Axum REST + SSE routes
│   └── svelte/            # SvelteKit frontend (Tailwind, responsive, accessible)
├── cli/                   # CLI entrypoint (clap subcommands)
└── tests/                 # Unit, integration, property-based, E2E
```

**Communication Flow:**
- CLI and Web UI both consume the same `app/` services — zero code duplication
- Background jobs emit progress/events via `tokio::sync::broadcast` channels
- Web frontend subscribes via Server-Sent Events (SSE) for real-time dashboard updates
- SQLite uses WAL mode for concurrent reads without blocking writes
- MakeMKV integration uses async process handling with structured stdout parsing

## Tech Stack

| Concern | Technology | Why |
|---------|-----------|-----|
| **Language** | Rust | Compile-time guarantees, `Result`/`Option` everywhere, zero-cost concurrency |
| **Runtime** | Tokio | Async runtime, zero-cost channels, no data races |
| **Database** | SQLite + sqlx | Version-controlled migrations, type-safe queries, WAL mode |
| **API** | Axum | Lightweight, ergonomic routing, built-in error handling |
| **Frontend** | SvelteKit | Reactive, lightweight, SPA with Tailwind CSS |
| **Media DB** | TMDb / OMDb | Rich metadata, episode matching, confidence scoring |
| **DVD Ripping** | MakeMKV | Async CLI wrapper, structured output parsing, error recovery |

## Features

- **Smart Scanning** — File discovery, SHA-256 hashing (parallel), FFprobe metadata extraction
- **Intelligent Matching** — TMDb/OMDB search with confidence scoring and episode matching logic
- **Review Workflow** — Accept, edit, or reject matches with keyboard shortcuts and bulk actions
- **File Organization** — Safe cross-device moves, duplicate detection, extras handling
- **DVD Ripping** — MakeMKV integration with drive detection, progress tracking, AutoRip
- **Real-time Dashboard** — SvelteKit frontend with SSE-powered live progress updates
- **CLI Interface** — Full pipeline control via `clap` subcommands
- **Type-safe Migrations** — sqlx migrations with schema version tracking

## Quick Start

### Prerequisites

- Rust 1.75+ (latest stable)
- Node.js 18+ (for SvelteKit frontend)
- SQLite 3 (usually pre-installed)
- FFprobe (from FFmpeg)
- MakeMKV (optional, for DVD ripping)
- TMDb API key (required for media analysis)
- OMDb API key (optional, fallback for TMDb misses)

### Build

```bash
# Build the entire workspace
cargo build --release

# Build only the CLI
cargo build --release -p riply_cli

# Build only the web server
cargo build --release -p riply_web
```

### Run the CLI

```bash
# Scan a directory
cargo run -- scan /path/to/media

# Analyze scanned files (requires TMDb API key)
cargo run -- analyze

# Review matches
cargo run -- review

# Move matched files to organized library
cargo run -- move --movies-dir /path/to/movies --tv-dir /path/to/tv-shows

# Check pipeline status
cargo run -- status

# Rip DVDs
cargo run -- rip
```

### Run the Web Server

```bash
# Start the Axum API + serve SvelteKit frontend
cargo run --bin riply-web

# Or build and run separately
cargo build --release -p riply_web
./target/release/riply-web
```

The web UI will be available at `http://localhost:3000` (or the port configured in your settings).

## Configuration

Riply uses a TOML configuration file (default: `config.toml`):

```toml
[scan]
input_dir = "/path/to/incoming"
movies_dir = "/path/to/movies"
tv_dir = "/path/to/tv-shows"
db_path = "riply.db"

[api]
tmdb_enabled = true
tmdb_api_key = "your-tmdb-api-key"
omdb_enabled = false
omdb_api_key = "your-omdb-api-key"

[settings]
verbose = false
autorip_enabled = false
```

## Pipeline States

```
NEW → SCANNED → MATCHED → CONFIRMED → MOVED
                 ↓
            NEEDS_REVIEW → CONFIRMED
                 ↓
               FAILED
```

- **NEW** — File discovered, not yet processed
- **SCANNED** — File hashed and metadata extracted
- **MATCHED** — TMDb/OMDB returned a match (confidence-based)
- **NEEDS_REVIEW** — Low-confidence match requires manual review
- **CONFIRMED** — Match accepted (auto or manual)
- **MOVED** — File organized into library
- **FAILED** — Processing error

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/files` | List files with pagination |
| GET | `/api/files/:id` | Get file details |
| PATCH | `/api/files/:id` | Update file status |
| GET | `/api/review` | Get files needing review |
| POST | `/api/review/:id/accept` | Accept a match |
| POST | `/api/review/:id/reject` | Reject a match |
| POST | `/api/move` | Start moving files |
| GET | `/api/move/status` | Get mover status |
| GET | `/api/ripper/drives` | List available drives |
| POST | `/api/ripper/:index/start` | Start ripping |
| GET | `/api/config` | Get current settings |
| PUT | `/api/config` | Update settings |
| GET | `/api/sse` | Server-Sent Events stream |

## Development

### Workspace Structure

```
Cargo.toml          # Workspace root, shared dependencies
core/               # Pure domain logic (no I/O)
app/                # Application services (orchestration)
infra/              # Infrastructure adapters (I/O)
web/                # Web server + SvelteKit frontend
cli/                # CLI entrypoint
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific crate tests
cargo test -p riply_core
```

### Code Quality

```bash
# Format code
cargo fmt

# Clippy checks
cargo clippy -- -D warnings

# Run pre-commit hooks manually
pre-commit run --all-files
```

### Pre-commit Hooks

The repository includes pre-commit hooks that run `cargo fmt` and `cargo clippy` on Rust files before each commit. Install them:

```bash
pip install pre-commit
pre-commit install
```

### Architecture Principles

1. **Core logic is 100% testable without a database, network, or filesystem**
2. **CLI and Web UI share the exact same application layer** (no code duplication)
3. **Progress tracking is event-driven** (SSE/broadcast channels instead of polling)
4. **Migrations are version-controlled** (no ad-hoc schema changes)
5. **All external dependencies are wrapped in swappable adapters**

## Roadmap

See [improvements.md](improvements.md) for the full project roadmap and [docs/improvements-details.md](docs/improvements-details.md) for detailed implementation notes.

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built as a rebuild of [MediaAtlas](https://github.com/Skovens/MediaAtlas)
- Media metadata powered by [TMDb](https://www.themoviedb.org/) and [OMDb](http://www.omdbapi.com/)
- DVD ripping powered by [MakeMKV](http://www.makemkv.com/)