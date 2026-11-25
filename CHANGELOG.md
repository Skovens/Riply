# Changelog

All notable changes to Riply are documented here. Tags mark rollback points;
`git log` has the full history.

## v0.0.1 (alpha) — 2026-11-25

First public release. Everything in the private v0.4/v0.5 history plus:

### Added
- MakeMKV detection with interactive install offer at startup (`internal/makemkvinstall`), SHA256-verified bundled installer, `GET /api/status` endpoint and web banner when MakeMKV is missing
- Autorip: auto-eject after accept and auto-start rip on disc insert (config `autorip`, default true)
- Media-capacity-driven rip progress with ETA
- Accept All button for multi-title rips

## v0.5 — 2026-11-07

### Added
- Duplicate rip warnings (issue #51): when a disc already exists in the library index, a `rip_duplicate` SSE toast fires on disc ready and at rip start — the rip is never blocked.
- Auto-Sequence (issue #47): the search modal pre-fills from the last ripped title when the disc name is unusable, backed by new `LastRip`/`RipMatches` queries in `internal/library`.
- Per-drive Auto-Sequence state (re-seeded from the global default at drive discovery and on each disc insert) with a drive-card checkbox, a `PUT /api/drives/{index}/auto-sequence` endpoint, and a global `auto_sequence` config toggle (default true) in Settings and `config.toml.example`.
- Drive cards show a `Last rip: …` line and a `Suggested for this disc: SxxEyy` line (explicit episode marker wins, otherwise library continuation).
- Test isolation: `newLibServer`/`newServerWithLib` point `MoviesDir`/`TempDir` at temp dirs, so tests no longer move files into `~/Movies`; new tests cover `LastRip`, `RipMatches` (case-insensitive, LIKE-metachar escaping, missing-flag exclusion), duplicate event publishing, last-rip payload gating, and the `auto_sequence` config round-trip.

## v0.4 — 2026-11-03

### Fixed
- TMDb API key saved in Settings now takes effect immediately, without a restart (and clearing it disables naming live).
- `undo-accept` can no longer leave a finished rip stuck as "ripping".
- Accept-title acts on the clicked row's index instead of matching by name, and moves exactly that title's file — wrong-file-moved is gone for multi-title rips.
- `config.Save` is atomic (temp + fsync + rename): a crash can no longer truncate config.toml into silent defaults.
- Concurrent accepts on one rip are serialised per session (`acceptMu`), preventing duplicate destination folders and biggest-file drift.
- Drive endpoints reject stale clicks after hotplug via a `?path=` echo check (409 "drive changed, refresh").
- TMDb rate limiter no longer holds its mutex across a sleep; startup warns when the bind address is not loopback (Riply has no auth).

### Changed
- Legacy MakeMKV `TITLE:`/`STREO:` disc-name parse branch removed — it never matched real output; MakeMKV v2 robot mode only.
- Disc-resolution event loop moved from `main.go` into `internal/drive.EventLoop`.
- Rip states are typed constants (`RipState`) instead of bare string literals.
- `resolveDrive` helper replaces six copies of drive-index resolution.
- `populateTitles` fallback de-duplicated into `listFilesAsTitles`.

### Added
- GitHub Actions CI: build + vet + `go test -race` on every push/PR.
- `.golangci.yml` for local monthly linting (5 linters).
- Tests for previously untested packages: `internal/config` (save/load round-trip, atomicity), plus naming client lifecycle and accept/undo race regression tests in `internal/api`.

## v0.3 — 2026-11-02

- SQLite library history with scan, reset, and TV next-up autofill (`internal/library`).

## Earlier

- Initial MakeMKV rip pipeline, drive state machine, SSE dashboard, TMDb naming. Untracked; first tagged release is v0.4.
