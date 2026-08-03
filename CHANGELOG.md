# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with pre-release tags.

Versioning scheme: `0.x.0-alpha.N` for alpha releases, `0.x.0-beta.N` for beta releases, `x.y.z` for stable releases.

## [Unreleased]

### Added
- Core domain logic: pipeline state machine, confidence scoring, filename parsing, path generation
- Infrastructure adapters: SQLite/sqlx, HTTP clients (TMDb/OMDB), MakeMKV, filesystem operations
- Application services: scanner, analyzer, mover, jobs scheduler, review workflow
- CLI interface with subcommands: scan, analyze, review, move, rip, status
- Web server with Axum REST API + SSE for real-time progress
- SvelteKit frontend with Tailwind CSS (dashboard, review, settings, ripper, logs screens)
- Pre-commit hooks (cargo fmt, cargo clippy)
- `.editorconfig` for consistent code style
- `deny.toml` for cargo-deny configuration

### Changed
- Replaced `anyhow` with `MetadataError` for type-safe error handling
- Centralized workspace dependencies (axum, tower, tower-http)
- Split `improvements.md` into roadmap + detailed docs

### Removed
- `anyhow` dependency from core crate

## [0.1.0-alpha.1] - TBA

### Added
- Initial project structure and workspace setup
- Phase 1-4 implementation (foundation, infrastructure, CLI/API, frontend)
- Master plan and architecture documentation

[Unreleased]: https://github.com/Skovens/Riply/compare/0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/Skovens/Riply/releases/tag/0.1.0-alpha.1