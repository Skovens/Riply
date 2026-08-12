# ADR-019: XDG Data Home for Config and Database

## Status

Proposed

## Context

Riply currently defaults to cwd-relative files: `config.toml`
(`cli/src/main.rs`) and `riply.db` (`infra/src/config.rs`). The binary's
behavior therefore depends on where it is launched from. For a release
binary that will live in `~/bin` on a Linux desktop, this is wrong:
running `riply serve` from a different directory would silently create
or use a different config and database, splitting the user's library
across directories.

Riply is single-machine, lightweight, and portable (ADR-002). The XDG
Base Directory spec is the Linux standard for locating per-user files.

Note on scope: strict XDG would split config (`~/.config`) from data
(`~/.local/share`). Riply deliberately keeps both under one directory
(`~/.config/riply`) for simplicity: a single folder that is easy to
back up, inspect, and delete. The DB is ephemeral by design (AGENTS.md
gotcha #5), so the data-vs-config distinction carries little
operational weight here.

## Decision

Resolve default config and database paths from the XDG config home.

- Config: `<config_home>/riply/config.toml`
  (default: `~/.config/riply/config.toml`)
- Database: `<config_home>/riply/riply.db`
  (default: `~/.config/riply/riply.db`)

Details:

- Resolve the base directory with the `dirs` crate
  (`dirs::config_dir()`, which honors `XDG_CONFIG_HOME` and resolves to
  `~/.config` on Linux).
- The `config.toml` `database_path` field remains, but relative values
  now resolve against the config directory (so a portable config keeps
  the DB next to itself), not the cwd; absolute paths pass through.
  `database_path_buf()` in `infra/src/config.rs` is the single
  resolution point.
- The `--config` flag default in `cli/src/main.rs` changes from
  `"config.toml"` to the resolved config path; an explicit `--config`
  still overrides.
- First run: create `~/.config/riply` and write a default config
  (`Config::create_default`) instead of erroring.
- Backup default output moves alongside the DB
  (`<config_home>/riply/riply_backup_<timestamp>.db`).
- Legacy cwd files are not auto-migrated: log an info hint pointing at
  the new location on first run. Do not silently rewrite user state
  (same lesson as the ADR-016 migration gap).

## Consequences

- The binary behaves identically no matter where it is launched from.
- First run works with zero flags from any cwd.
- Old cwd `riply.db`/`config.toml` from earlier sessions are no longer
  picked up; relocation is manual (or a documented one-time step).
- Roughly eight tests in `infra/src/config.rs` assert the old defaults
  and must be updated.
- `--config <path>` and an absolute `database_path` remain the escape
  hatches.
