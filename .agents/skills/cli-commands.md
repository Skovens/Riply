---
name: cli-commands
description: >
  CLI interface for Riply using clap. Covers all subcommands (scan,
  analyze, review, move, rip, serve, status, setup), argument parsing,
  config loading, database initialization, and the CLI vs web distinction.
metadata:
  language: rust
  version: "1.0+"
  compatibility: clap, tokio, tracing, riply_core, infra
---

# CLI Commands

## Overview

The CLI binary (`riply`) is in the `cli` crate. It uses `clap` for argument
parsing and `tracing` for logging. The CLI runs operations **synchronously**
— it does NOT use `JobScheduler` (that's only for the web server).

```
cli (binary) -> main.rs -> clap parsing -> config load -> db init -> command dispatch
```

### Workspace Structure

```
cli/
  Cargo.toml           - Package: cli, Binary: riply
  src/
    main.rs            - Cli struct, Command enum, main()
    commands/
      mod.rs           - Re-exports all command functions
      scan.rs          - scan_directory()
      analyze.rs       - analyze_files()
      review.rs        - review_files() with interactive loop
      mover.rs         - move_files()
      rip.rs           - RipCommand enum, rip_command()
      serve.rs         - serve() (starts web server)
      setup.rs         - SetupArgs, run_setup()
      status.rs        - show_status()
```

## Binary Entry Point

**Binary name:** `riply` (defined in `cli/Cargo.toml`)

```rust
/// Riply - Rip, manage, move media files
#[derive(Parser, Debug)]
#[command(name = "riply", version, about)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    command: Command,
}
```

### Global Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--config` | `-c` | `config.toml` | Path to TOML config file |
| `--verbose` | `-v` | false | Enable DEBUG-level logging |

## Command Dispatch

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 1. Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if cli.verbose { Level::DEBUG } else { Level::INFO })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // 2. Load configuration (create default if missing)
    let config = match Config::load(&cli.config) {
        Ok(config) => config,
        Err(_) => {
            info!("Configuration file not found, creating default");
            Config::create_default(&cli.config)?;
            Config::load(&cli.config)?
        }
    };

    // 3. Initialize database
    let db = Database::new(&config.database_path).await?;
    db.migrate().await?;

    // 4. Execute command
    match cli.command {
        Command::Scan { directory } => scan_directory(&directory, &config).await?,
        Command::Analyze => analyze_files(&db, &config).await?,
        Command::Review { yolo } => review_files(yolo, &db).await?,
        Command::Move => move_files(&db, &config).await?,
        Command::Rip { rip_command } => rip_command(rip_command, &config).await?,
        Command::Serve { host, port } => serve(db, config, Some(cli.config), host, port).await?,
        Command::Status => show_status(&db).await?,
        Command::Setup(args) => run_setup(args)?,
    }

    info!("Riply finished");
    Ok(())
}
```

## Subcommands

### Scan

```bash
riply scan <directory>
```

Scans a directory recursively for media files (.mkv, .mp4, .avi, .mov, .m4v, .webm).

**Parameters:**
- `directory` (positional): Path to scan

**What it does:**
1. Validates directory exists
2. Initializes database (migrate)
3. Runs `ScannerService::scan_directory()`
4. For each file: hashes, saves to DB
5. Prints summary of files found

**Output:**
```
Found 42 media files:

  movie.mkv [abc123def456]
    Status: Scanned
    Size: 4294967296

  show.s01e01.mkv [def789abc012]
    Status: Scanned
    Size: 1073741824

42 records saved to database
```

### Analyze

```bash
riply analyze
```

Analyzes all `Scanned` files to identify media and assign pipeline status.

**What it does:**
1. Loads `Scanned` files from database
2. Creates `MetadataSearcher` if API keys are configured in config
3. Runs `AnalyzerService::analyze_file()` on each record
4. Updates database with results
5. Prints summary: matched, needs review, failed

**Output:**
```
Analyzing 42 files...

  movie.mkv -> The Movie (2023)
  show.s01e01.mkv -> Breaking Bad (S01E01) (needs review)
  extras.mkv (failed)

Analysis complete:
  Matched: 35
  Needs review: 5
  Failed: 2
```

**Metadata search behavior:**
- If `tmdb_api_key` or `omdb_api_key` is set: uses external metadata
- If neither is set: filename-only heuristic analysis

### Review

```bash
riply review [--yolo]
```

Interactive review of files that need manual confirmation (`NeedsReview` status).

**Flags:**
| Flag | Description |
|------|-------------|
| `--yolo` | Confirm all files without individual review |

**Interactive mode (no --yolo):**
1. Loads `Needs Review` files from database
2. Displays each file with suggested title, year, confidence
3. Prompts for comma-separated indices to confirm
4. Supports commands: `all`, `skip`, `q`, `quit`
5. Updates confirmed files to `Confirmed` status

**Prompt:**
```
5 file(s) remaining:
  [1] movie.mkv
      Suggested: The Movie (2023)
      Confidence: 72.0%
  [2] show.s01e01.mkv
      Suggested: Breaking Bad (2008)
      Confidence: 65.0%

  Enter numbers to confirm (e.g. 1,3,5)
  'all' to confirm remaining  |  'skip' to quit  |  'q' to quit
>
```

**YOLO mode:**
```
riply review --yolo
YOLO mode enabled - confirming all files without review
All 5 files confirmed
```

### Move

```bash
riply move
```

Moves `Confirmed` files to an organized library structure.

**What it does:**
1. Loads `Confirmed` files from database
2. Creates movie/TV directories if they don't exist
3. Runs `MoverService::move_file()` on each record
4. Updates database with new paths and `Moved` status
5. Prints summary: moved, skipped, errors

**Output:**
```
Moving 10 files to organized library:
  Movies: /mnt/media/movies
  TV: /mnt/media/tv

  ✓ movie.mkv -> "/mnt/media/movies/The Movie (2023)/movie.mkv"
  ✓ show.s01e01.mkv -> "/mnt/media/tv/Breaking Bad/Season 01/show.s01e01.mkv"

Move complete:
  Moved: 10
  Skipped: 0
  Errors: 0
```

**Destination structure:**
- Movies: `movies_dir/Title (Year)/filename`
- TV: `tv_dir/Series Name/Season XX/filename`

### Rip

```bash
riply rip <subcommand>
```

Manage disc ripping via MakeMKV.

**Subcommands:**

| Command | Description |
|---------|-------------|
| `riply rip list` | List available drives |
| `riply rip start <index> [-d <dest>]` | Start ripping on a drive |
| `riply rip eject <index>` | Eject disc from drive |

**Rip start:**
```bash
riply rip start 0 -d /mnt/media/rips
```

- `index` (positional): Drive index (0-based)
- `-d, --destination`: Optional destination directory (defaults to `rip_output_directory` from config)

**Behavior:**
1. Auto-detects MakeMKV backend
2. Looks up disc name, creates subdirectory if applicable
3. Starts rip, prints progress percentage
4. Press Ctrl+C to cancel
5. Reports complete or error

**Output:**
```
Ripping started on drive 0 → /mnt/media/rips/Title
Press Ctrl+C to cancel
Progress: 45.2%Progress: 78.9%
Progress: 100.0%
Rip complete!
```

**Rip list:**
```bash
riply rip list
```

```
Available drives:
  [0] PLDS DVD-RW DU-16A8 - READY
      Disc: TITLE.00.1.234.567
  [1] ATAPI DVD-ROM - EMPTY
```

**Rip eject:**
```bash
riply rip eject 1
```

```
Ejected disc from drive 1 (/dev/sr1)
```

### Serve

```bash
riply serve [--host <host>] [-p <port>]
```

Starts the web API server with embedded SvelteKit frontend.

**Flags:**
| Flag | Description |
|------|-------------|
| `--host` | Bind address (overrides config) |
| `-p, --port` | Bind port (overrides config) |

**What it does:**
1. Initializes database
2. Detects MakeMKV backend (nullable)
3. Builds `AppState` with all dependencies
4. Starts autorip if enabled in config
5. Starts healthchecks heartbeat if configured
6. Creates router and serves on the specified address

**Defaults from config:**
- Host: `0.0.0.0`
- Port: `3000`

### Status

```bash
riply status
```

Shows current pipeline status counts.

**Output:**
```
Riply Status:
  Total files: 42
  Scanned:     5
  Matched:     20
  Review:      8
  Confirmed:   4
  Moved:       5
  Failed:      0

  Drives:      (use `riply serve` + web UI for drive info)
  Jobs:        (use web UI at http://localhost:3000/#/jobs)
```

### Setup

```bash
riply setup [--dry-run]
```

Interactive setup for MakeMKV installation (Flatpak).

**Flags:**
| Flag | Description |
|------|-------------|
| `--dry-run` | Show what would be done without executing |

## Configuration

The CLI loads config from a TOML file. Default path is `config.toml`.

```bash
riply -c /etc/riply/config.toml scan /mnt/media
```

**Config file format:**
```toml
database_path = "riply.db"
scan_directory = "/mnt/media"
movies_directory = "/mnt/media/movies"
tv_directory = "/mnt/media/tv"
rip_output_directory = "/mnt/media/rips"
verbose = false
tmdb_api_key = "your-tmdb-key"
omdb_api_key = "your-omdb-key"
host = "0.0.0.0"
port = 3000

[ripping]
autorip_enabled = true
autorip_eject_after = true
autorip_excluded_drives = []

[healthchecks]
uuid = ""
ping_interval_minutes = 5
```

## CLI vs Web Distinction

| Aspect | CLI | Web (serve) |
|--------|-----|-------------|
| Job system | NOT used | Uses `JobScheduler` |
| Progress | Printed to stdout | Sent via SSE |
| Cancellation | Ctrl+C (OS-level) | `cancel_job()` API |
| Review | Interactive TUI | Web UI |
| Real-time updates | N/A | SSE streaming |
| Concurrency | Single-threaded per command | Concurrent jobs |

**Key rule:** CLI commands run services directly. They do NOT create jobs,
do NOT broadcast SSE events, and do NOT support cooperative cancellation.

## Error Handling

All CLI commands return `Result<(), anyhow::Error>`:

```rust
// Validation errors
if !path.exists() {
    return Err(anyhow::anyhow!("Directory does not exist: {}", directory));
}

// Database errors propagate with context
db.save_file(&record).await?;

// MakeMKV errors
Err(RipError::NotAvailable) => {
    println!("No ripping backend found.");
    println!("Run `riply setup` to install MakeMKV via Flatpak.");
    Err(anyhow::anyhow!("No ripping backend available"))
}
```

## Logging

Uses `tracing` with `tracing-subscriber`.

```rust
use tracing::{info, Level};

// In main():
let subscriber = FmtSubscriber::builder()
    .with_max_level(if cli.verbose { Level::DEBUG } else { Level::INFO })
    .finish();
tracing::subscriber::set_global_default(subscriber)?;

// In commands:
info!("Scanning directory: {}", directory);
info!("Analysis complete: {} matched, {} need review", matched, review);
```

**Log levels:**
- `INFO` (default): Operation start/end, key results
- `DEBUG` (`-v`): Detailed file-by-file info

## Testing Patterns

### CLI Command Testing

Use `assert_cmd` and `predicates` for integration tests:

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_scan_nonexistent_dir() {
    let mut cmd = Command::cargo_bin("riply").unwrap();
    cmd.arg("scan")
       .arg("/nonexistent/path")
       .assert()
       .failure()
       .stderr(predicate::str::contains("Directory does not exist"));
}
```

### Config Loading Tests

```rust
let dir = tempdir().unwrap();
let config_path = dir.path().join("config.toml");
Config::create_default(config_path.to_str().unwrap()).unwrap();
let config = Config::load(config_path.to_str().unwrap()).unwrap();
assert_eq!(config.database_path, "riply.db");
```

## Common Pitfalls

1. **CLI does NOT use JobScheduler** — The CLI runs services directly.
   Never try to use `JobScheduler` in a CLI command. Jobs are only for
   the web server.

2. **Config is created if missing** — If `config.toml` doesn't exist,
   the CLI creates a default one and loads it. Don't treat missing
   config as an error.

3. **Review requires stdin** — The review command reads from stdin
   interactively. It will hang if stdin is not a terminal (e.g., in
   CI). Use `--yolo` for non-interactive mode.

4. **Rip commands block** — `riply rip start` runs until completion
   or Ctrl+C. It does NOT return a job ID or background the operation.

5. **Database path comes from config, not CLI** — The CLI always uses
   the database path from the config file. There is no `--db` flag.

6. **Setup is synchronous** — `run_setup()` returns `Result<(), anyhow::Error>`
   and is NOT async. It runs MakeMKV installation synchronously.

7. **Verbose flag affects ALL commands** — `-v` sets the global tracing
   level to DEBUG. There is no per-command verbose flag.

8. **Serve passes config_path to AppState** — When starting the web
   server, the CLI passes `Some(cli.config)` as the config path so
   the web API can persist config changes to disk.
