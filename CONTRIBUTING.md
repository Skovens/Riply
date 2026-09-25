# Contributing to Riply

Thanks for your interest in contributing! This guide covers everything from setting up your development environment to submitting a pull request.

> **Note:** Riply is alpha software (`0.x.0-alpha.N`). APIs, CLI commands, and file organization may change between releases.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Issue Templates](#issue-templates)

## Development Setup

### Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | 1.75+ (latest stable) | Core, app, infra, CLI |
| Node.js | 18+ | SvelteKit frontend |
| SQLite | 3.x | Database (usually pre-installed) |
| FFprobe | From FFmpeg | Media metadata extraction |
| MakeMKV | Latest | DVD ripping (optional) |

### Install Pre-commit Hooks

```bash
pip install pre-commit
pre-commit install
```

The hooks run `cargo fmt` and `cargo clippy -- -D warnings` on every commit.

### Build the Project

```bash
# Build entire workspace
cargo build

# Build release
cargo build --release

# Build specific crate
cargo build -p riply_core
cargo build -p riply_cli
cargo build -p riply_web
```

### Frontend Development

```bash
cd web/svelte
npm install

# Start dev server (HMR, API proxy)
npm run dev

# Build for production
npm run build

# Run Playwright E2E tests
npx playwright install
npm test              # Run all tests
npm run test:ui       # Interactive UI mode
npm run test:debug    # Debug mode
```

## Project Structure

```
Cargo.toml              # Workspace root, shared dependencies
core/                   # Pure domain logic (no I/O, fully testable)
│   ├── pipeline.rs     # State machine, status transitions
│   ├── confidence.rs   # Scoring algorithms, thresholds
│   ├── filename.rs     # Parsing, cleaning, TV/movie detection
│   ├── paths.rs        # Destination path generation
│   ├── file_record.rs  # File metadata model
│   ├── media_type.rs   # Movie vs TV types
│   ├── metadata.rs     # Media metadata handling
│   └── ripping.rs      # DVD ripping state machine
app/                    # Application services (orchestration)
│   ├── scanner.rs      # File discovery, hashing, metadata extraction
│   ├── analyzer.rs     # TMDb/OMDB search, episode matching
│   ├── mover.rs        # File organization, duplicate handling
│   └── jobs.rs         # Background task scheduler, progress channels
infra/                  # Infrastructure adapters (I/O heavy)
│   ├── db.rs           # SQLite + sqlx (migrations, queries, WAL mode)
│   ├── http.rs         # reqwest with retry/rate-limiting, SSE endpoints
│   ├── makemkv.rs      # Async CLI wrapper, progress parsing
│   ├── fs.rs           # File operations, safe moves, temp cleanup
│   └── config.rs       # TOML settings, validation, path resolution
web/                    # Web server + frontend
│   ├── src/            # Axum API routes (REST + SSE)
│   └── svelte/         # SvelteKit frontend (Tailwind, responsive)
cli/                    # CLI entrypoint (clap subcommands)
tests/                  # Integration and E2E tests
scripts/                # Utility scripts
docs/                   # Detailed documentation
```

**Key principle:** `core/` has zero I/O dependencies. `app/` orchestrates `core/` and `infra/`. `web/` and `cli/` both consume `app/`.

## Code Style

### Rust

```bash
# Format code (run before every commit — pre-commit hook does this automatically)
cargo fmt

# Clippy checks (pre-commit hook enforces -D warnings)
cargo clippy -- -D warnings

# Check for security vulnerabilities
cargo deny check
```

**Conventions:**
- Use `thiserror` for domain errors, not `anyhow`
- Use `tracing` for structured logging
- Prefer `Result` over `panic!`
- Type-safe queries with `sqlx` — no raw SQL with user input
- Document all public APIs with docstrings

### Svelte/TypeScript

```bash
cd web/svelte
npm run lint     # ESLint
npm run format   # Prettier
```

## Testing

### Rust

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific crate
cargo test -p riply_core

# Run specific test
cargo test -p riply_core filename::tests::parse_tv_episode
```

**Test types:**
- **Unit tests** — in `#[cfg(test)]` modules, test pure logic
- **Property-based tests** — `proptest` for confidence scoring and filename parsing
- **Integration tests** — in `tests/` directory, test multi-crate interactions
- **HTTP mocks** — `wiremock` for TMDb/OMDB API tests
- **Database mocks** — `mockall` for adapter tests

### Playwright E2E

```bash
cd web/svelte
npm test              # Run all tests
npm run test:ui       # Interactive UI mode
npm run test:debug    # Debug mode
```

Tests cover all main routes: home, files, jobs, review, ripper, settings, and navigation flow.

## Pull Request Process

### Branch Naming

Use descriptive prefixes:

| Prefix | Use case | Example |
|--------|----------|---------|
| `feat/` | New features | `feat/dvd-ripping` |
| `fix/` | Bug fixes | `fix/tmdb-rate-limit` |
| `chore/` | Maintenance, deps, config | `chore/update-tokio` |
| `docs/` | Documentation changes | `docs/add-contributing` |
| `test/` | Testing improvements | `test/add-unit-tests` |
| `refactor/` | Code refactoring | `refactor/restructure-core` |

### PR Checklist

- [ ] Branch is up to date with `main` (`git rebase main`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] All tests pass (`cargo test`)
- [ ] E2E tests pass (`npm test` in `web/svelte/`)
- [ ] Changes are documented (README, CHANGELOG, inline docs)
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) format

### Commit Message Format

```
type: short description

 Longer description if needed (optional).
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

Examples:
```
feat: add DVD ripping with MakeMKV integration
fix: handle TMDb rate limiting with exponential backoff
docs: update README with new configuration options
chore: update tokio from 1.35 to 1.37
```

### Submitting a PR

1. Create a branch from `main` with the naming convention above
2. Make your changes and commit with descriptive messages
3. Push and open a pull request targeting `main`
4. Fill out the PR template (auto-populated) with:
   - **What** — summary of changes
   - **Why** — motivation or issue reference
   - **Testing** — how you verified the changes
5. Address review comments
6. Squash merge when approved

## Issue Templates

### Bug Report

When filing a bug report, include:
- **What happened** — clear description of the bug
- **What you expected** — the correct behavior
- **Steps to reproduce** — minimal steps anyone can follow
- **Environment** — OS, Rust version, relevant dependencies
- **Logs** — any relevant output from `cargo run --verbose` or browser console

### Feature Request

When requesting a feature, include:
- **What problem does this solve?** — context and motivation
- **Proposed solution** — your idea (or ask for brainstorming)
- **Alternatives considered** — other approaches you've thought about
- **Additional context** — screenshots, examples, related issues

## Getting Help

- Check [improvements.md](improvements.md) for the project roadmap
- Check [docs/](docs/) for detailed architecture documentation
- Check [AGENTS.md](AGENTS.md) for AI agent instructions
- File an issue with the `question` label if you're stuck

## AI Usage Policy

This repository is designed for AI-assisted development. AI-generated code is welcome, but:

- **Review before committing:** AI can produce correct-looking but subtly wrong code. Always review diffs carefully.
- **Verify with tests:** AI may miss edge cases. Run the full test suite (`cargo test`) before submitting.
- **Follow conventions:** AI should follow the coding standards in this guide and AGENTS.md.
- **Credit AI assistance:** If a change was primarily generated by AI, note it in the commit message (e.g., `feat: add X (AI-assisted)`).

## Code of Conduct

Be respectful, inclusive, and constructive. Alpha software moves fast — help each other out.