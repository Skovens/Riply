# Design: CLI Interface

## Purpose
Command-line interface for Riply operations. Built with Clap derive macros. This document describes the *current* behavior of each CLI command.

---

## Entry Point

### `riply` — `cli/src/main.rs`

**Global options:**
| Option | Short | Default | Description |
|---|---|---|---|
| `--config` | `-c` | `config.toml` | Path to configuration file |
| `--verbose` | `-v` | `false` | Enable debug logging |
| `--log-format` | — | `compact` | Log format: `compact`, `pretty`, `json` |

**Execution flow:**
1. Parse CLI arguments
2. Initialize logging (tracing + EnvFilter)
3. Load or create default config
4. Initialize database (new + migrate)
5. Execute subcommand
6. Log "Riply finished"

**Logging:**
- `compact` — single-line, no colors (default)
- `pretty` — multi-line, colorized
- `json` — structured JSON output
- Log level: `info` (default) or `debug` (with `-v`)

---

## Commands

### `riply scan <directory>`

Scan a directory for media files, save to database.

**Arguments:**
- `directory` — Directory to scan
- `--json` — Output as JSON

**Behavior:**
1. Validates directory exists
2. Creates database connection + runs migrations
3. Initializes metadata searcher (if API keys configured)
4. Calls `ScannerService::scan_directory()`
5. Saves title groups to database
6. Saves file records to database
7. Shows progress bar (non-JSON mode)

**JSON output:**
```json
{
    "directory": "/path/to/scan",
    "title_groups": 1,
    "files": 5,
    "records": [
        { "id": "...", "filename": "movie.mkv", "status": "Scanned", "size": 1234567 }
    ]
}
```

---

### `riply analyze`

Analyze all scanned files for metadata matching.

**Arguments:**
- `--json` — Output as JSON

**Behavior:**
1. Loads all files with `status = Scanned` from database
2. If none: prints message and returns
3. Initializes metadata searcher (if API keys configured)
4. Falls back to filename-only analysis if no API keys
5. For each file:
   - Calls `AnalyzerService::analyze_file()`
   - Updates database with analyzed record
   - Increments counters (matched, needs_review, failed)
6. Prints summary (non-JSON) or JSON output

**JSON output:**
```json
{
    "files_analyzed": 10,
    "matched": 7,
    "needs_review": 2,
    "failed": 1,
    "results": [
        { "filename": "movie.mkv", "status": "Matched", "suggested_title": "Inception", "suggested_year": 2010, "confidence": 0.95 }
    ]
}
```

---

### `riply review`

Interactive review of files needing manual confirmation.

**Arguments:**
- `--yolo` — Confirm all files without review

**Behavior (interactive mode):**
1. Loads all files with `status = Needs Review` from database
2. If none: prints message and returns
3. Enters review loop:
   - Shows remaining files with suggested title, year, confidence
   - Prompts for comma-separated indices to confirm (e.g., `1,3,5`)
   - Commands:
     - `all` — Confirm all remaining
     - `skip` / `q` / `quit` — Stop reviewing
     - Empty input — Stop reviewing
   - Confirms selected files (sets `status = Confirmed`)
4. Prints summary of confirmed files

**YOLO mode:**
- Confirms all files in one batch without prompting
- Sets `status = Confirmed` for all `Needs Review` files

---

### `riply move`

Move confirmed files to organized library.

**Arguments:**
- `--json` — Output as JSON

**Behavior:**
1. Loads all files with `status = Confirmed` from database
2. If none: prints message and returns
3. Creates movies and TV directories (if they don't exist)
4. For each file:
   - Checks `is_ready_to_move()` (status must be Confirmed)
   - Calls `MoverService::move_file()`
   - Updates database with moved record
   - Counts: moved, skipped, errors
5. Prints summary (non-JSON) or JSON output

**JSON output:**
```json
{
    "movies_dir": "/path/to/movies",
    "tv_dir": "/path/to/tv",
    "files_processed": 10,
    "moved": 8,
    "skipped": 1,
    "errors": 1,
    "results": [
        { "filename": "movie.mkv", "status": "moved", "path": "/path/to/movies/Inception (2010)/movie.mkv" }
    ]
}
```

---

### `riply rip <subcommand>`

Subcommands for disc ripping operations.

**Subcommands:**

#### `riply rip list [--json]`

List available MakeMKV drives.

**Output:**
```
Available drives:
  [0] HL-DT-ST BD-RE BH16NS40 - Ready
      Disc: INCEPTION_2010
  [1] TSSTcorp CDDVDW SH-224DB - Empty
```

#### `riply rip start <index> [-d <destination>]`

Start ripping on a drive.

**Arguments:**
- `index` — Drive index to rip
- `-d, --destination` — Destination directory (default: `rip_output_directory` from config)

**Behavior:**
1. Validates backend is available (shows setup hint if not)
2. Resolves destination:
   - If `-d` provided: use as-is
   - Otherwise: validate config, use `rip_output_directory`
3. Detects drives, resolves disc name (TMDb/OMDb search if API keys available)
4. Creates unique destination path (appends ` (Copy N)` if needed)
5. Starts rip via backend
6. Enters event loop:
   - `Progress` — Shows percentage in place
   - `Complete` — Prints "Rip complete!" and exits
   - `Error` — Prints error and exits
   - `Message` — Prints message
   - `None` — No more events, exits
7. Ctrl+C handling:
   - Spawns task to listen for Ctrl+C
   - Sends cancel signal to rip handle
   - Prints "Cancelling rip..."

#### `riply rip eject <index>`

Eject disc from a drive.

**Arguments:**
- `index` — Drive index to eject

**Behavior:**
1. Validates backend is available
2. Detects drives, finds device path for index
3. Calls `infra::eject_disc()`
4. Prints confirmation message

---

### `riply serve [OPTIONS]`

Start the web API server.

**Arguments:**
- `--host` — Bind address (overrides config, default: `0.0.0.0`)
- `-p, --port` — Bind port (overrides config, default: `3000`)

**Behavior:**
1. Loads config + database
2. Initializes MakeMKV backend (from config or auto-detect)
3. Initializes metadata searcher (if API keys configured)
4. Creates `AppState` with:
   - Database, config, SSE manager (256 events)
   - Backend, searcher, job scheduler
   - Active rips map, autorip drives set
   - Metrics collector, disc names map
5. Starts autorip poll loop (background task)
6. Starts healthchecks.io heartbeat (if configured)
7. Creates Axum router with all route modules
8. Binds TCP listener, starts serving
9. Graceful shutdown (Ctrl+C or SIGTERM)
10. Aborts autorip handle on exit

**Route modules:**
- `files` — CRUD for file records
- `review` — Review queue (accept/reject)
- `scan` — Scan directory
- `analyze` — Analyze scanned files
- `mover` — Preview and execute moves
- `jobs` — Job management
- `ripper` — Drive management, rip control, autorip
- `config` — Configuration management
- `search` — TMDb/OMDb search
- `sse_handler` — SSE endpoint
- Health check (`/health`), readiness (`/ready`), metrics (`/metrics`)

---

### `riply status [--json]`

Show pipeline status counts.

**Output (non-JSON):**
```
Riply Status:
  Total files: 42
  Scanned:     5
  Matched:     10
  Review:      3
  Confirmed:   15
  Moved:       9
  Failed:      0

  Drives:      (use `riply serve` + web UI for drive info)
  Jobs:        (use web UI at http://localhost:3000/#/jobs)
```

**JSON output:**
```json
{
    "total_files": 42,
    "scanned": 5,
    "matched": 10,
    "needs_review": 3,
    "confirmed": 15,
    "moved": 9,
    "failed": 0
}
```

---

### `riply duplicate [--json]`

Find duplicate files by hash.

**Behavior:**
1. Loads all files from database
2. Groups files by hash
3. Filters groups with more than one file
4. Reports duplicate groups

**Output (non-JSON):**
```
Found 2 duplicate group(s) with 3 total duplicate(s):

Group 1 (3 files, hash: abc123def456...):
  [original] movie1.mkv ("/path/to/movie1.mkv")
  [duplicate] movie2.mkv ("/path/to/movie2.mkv")
  [duplicate] movie3.mkv ("/path/to/movie3.mkv")
```

**JSON output:**
```json
{
    "duplicate_groups": 2,
    "total_duplicates": 3,
    "files_scanned": 50,
    "groups": [
        { "hash": "abc123...", "files": [{"id": "...", "filename": "movie1.mkv", "path": "...", "size": 1234567}] }
    ]
}
```

---

### `riply library <directory> [--media-type <type>] [--json]`

Scan an organized media library and import files.

**Arguments:**
- `directory` — Directory to scan
- `--media-type` — `movie` or `tv` (default: `movie`)
- `--json` — Output as JSON

**Behavior:**
1. Validates directory exists
2. Creates database connection
3. Initializes metadata searcher (if API keys configured)
4. Scans directory (folder-first mode)
5. Saves title groups to database
6. For each file record:
   - Checks if file exists by hash
   - If exists: updates record
   - If new: inserts record
7. Tracks new vs updated counts
8. Prints summary or JSON output

---

### `riply watch <directory> [--media-type <type>]`

Watch a directory for new files and auto-scan them.

**Arguments:**
- `directory` — Directory to watch
- `--media-type` — `movie` or `tv` (default: `movie`)

**Behavior:**
1. Validates directory exists
2. Creates database connection
3. Initializes metadata searcher
4. Populates `seen_files` set with existing media files
5. Enters polling loop (5-second interval):
   - Reads directory entries
   - For each new media file:
     - Waits 2 seconds (file write completion)
     - Checks file still exists
     - Scans directory
     - Saves title groups and file records
     - Tracks new file count
   - Logs errors for failures
6. Ctrl+C to stop

---

### `riply backup [-o <output>]`

Backup the database to a file.

**Arguments:**
- `-o, --output` — Destination backup file path (default: `riply_backup_<timestamp>.db`)

**Behavior:**
1. Calls `Database::backup()` which uses SQLite's `sqlite3_backup` API
2. Prints confirmation message

---

### `riply restore <source>`

Restore the database from a backup file.

**Arguments:**
- `source` — Source backup file

**Behavior:**
1. Validates source file exists
2. Calls `Database::restore()` which:
   - Attaches backup database
   - Drops existing tables
   - Recreates tables from backup DDL
   - Copies data from backup
   - Recreates indexes
   - Commits transaction
   - Detaches backup
3. Prints confirmation message

---

### `riply setup [--dry-run]`

Interactive MakeMKV installation setup.

**Arguments:**
- `--dry-run` — Show what would be done without executing

**Behavior:**
1. Checks Flatpak installation
2. If missing: prompts to install via detected package manager (apt/dnf/pacman)
3. Checks Flathub remote
4. If missing: prompts to add Flathub
5. Checks MakeMKV installation
6. If missing: prompts to install from Flathub
7. Checks cdrom group membership
8. If missing: prompts to add user to cdrom group (requires sudo)
9. Verifies installation
10. Reports status

---

## Where This Lives

| Concern | File |
|---|---|
| CLI entry point | `cli/src/main.rs` |
| Command definitions | `cli/src/commands/mod.rs` |
| Scan command | `cli/src/commands/scan.rs` |
| Analyze command | `cli/src/commands/analyze.rs` |
| Review command | `cli/src/commands/review.rs` |
| Move command | `cli/src/commands/mover.rs` |
| Rip command | `cli/src/commands/rip.rs` |
| Serve command | `cli/src/commands/serve.rs` |
| Status command | `cli/src/commands/status.rs` |
| Duplicate command | `cli/src/commands/duplicate.rs` |
| Library command | `cli/src/commands/library.rs` |
| Watch command | `cli/src/commands/watch.rs` |
| Backup/Restore | `cli/src/commands/backup.rs` |
| Setup command | `cli/src/commands/setup.rs` |

---

## Notes

- **All commands load config first** — `config.toml` by default
- **Database is initialized per-command** — Each command creates its own connection
- **Progress bars** — Used in non-JSON mode for scan, analyze, move
- **JSON output** — Structured output for scripting/automation
- **Metadata searcher** — Only initialized if TMDb/OMDb API keys are configured
- **MakeMKV backend** — Only initialized for rip commands
- **Watch command** — Long-running, polls every 5 seconds
- **Setup command** — Interactive, requires TTY, uses sudo for group changes
