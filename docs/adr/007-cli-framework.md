# ADR-007: CLI Framework Selection

## Status

Accepted

## Context

Riply provides a command-line interface for:
- Scanning directories for media files
- Analyzing scanned files (metadata lookup)
- Reviewing and confirming matches
- Moving files to library structure
- Disc ripping (MakeMKV integration)
- Starting the web server
- Checking pipeline status

The CLI must be:
- Intuitive with subcommands
- Well-documented (`--help` output)
- Configurable via arguments and config file

Options considered:
1. **Clap** — Most popular Rust CLI library, derive API, auto-generated help
2. ** structopt** — Was superseded by clap 3.0 (now clap_derive)
3. **crossterm** + manual parsing — Maximum control, high effort
4. **arstruct** — Simpler, fewer features

## Decision

Use **Clap** (version 4) with derive macros.

Commands implemented:
- `riply scan <path>` — Scan directory for media
- `riply analyze` — Look up metadata for scanned files
- `riply review` — Review and confirm/reject matches
- `riply move` — Move confirmed files to library
- `riply rip drives/start/cancel` — Disc ripping commands
- `riply serve` — Start web server (with `--port` option)
- `riply status` — Show pipeline status

## Consequences

### Positive
- Derive API is clean and declarative
- Auto-generated `--help` and man pages
- Type-safe argument parsing (no manual validation)
- Subcommand nesting works naturally
- Large ecosystem and community

### Negative
- Clap 4 is a significant version jump from clap 3 — breaking changes
- Derive macros can be slow to compile for very complex CLIs (not an issue for Riply)
- Limited support for interactive CLI features (progress bars, confirmation prompts) — would need `indicatif` or `dialoguer`

### Future Considerations
Consider adding `--json` output flag to CLI commands (recommended in improvements.md) for machine-readable output and scripting support.
