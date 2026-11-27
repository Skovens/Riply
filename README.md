# Riply

Rip DVDs and Blu-rays, auto-name with TMDb, and organize into a Jellyfin-compliant library. Run as a single binary — opens your browser automatically.

## Quick Start

```bash
# Build
go build -o riply ./cmd/server/

# Run
./riply
```

The server starts on `http://127.0.0.1:3000` and opens your default browser automatically. The binary is self-contained — the frontend (HTML/CSS/JS) is compiled in via `embed`.

### Download a release

Prebuilt binaries for Linux (amd64/arm64) are attached to each [GitHub release](https://github.com/Skovens/Riply/releases). Verify with `checksums.txt`:

```bash
curl -LO https://github.com/Skovens/Riply/releases/latest/download/riply_<version>_linux_amd64.zip
unzip riply_<version>_linux_amd64.zip && chmod +x riply
```


### Runtime dependencies

| Binary | Needed for |
|--------|------------|
| `makemkvcon` | ripping and disc info (MakeMKV CLI) |
| `eject` | ejecting discs (MakeMKV v2.0.0 removed its own eject) |
| `lsblk` | volume-label fallback when MakeMKV cannot name the disc |

### Installing MakeMKV (Linux)

MakeMKV has no packaged Linux install, but Riply can do it for you. On a
machine without MakeMKV, `./riply` asks before it starts:

```text
MakeMKV is not installed — Riply cannot rip without it.
  Install MakeMKV now? ... [Y/n]
```

Answer **yes** and the bundled installer takes care of the rest in one go —
it finds the latest version, downloads the release (verifying the
published SHA256), installs build dependencies, compiles the MakeMKV
libraries, installs everything, and verifies `makemkvcon` runs. It asks
for the sudo password once. Answer **no** (or run Riply without a
terminal, e.g. as a service) and it prints the manual steps instead; the
web UI shows the same hint as a banner while MakeMKV is missing.

To run the installer by hand:

```bash
bash scripts/install-makemkv.sh        # latest version
bash scripts/install-makemkv.sh --help # options (pinned version, no-deps, ...)
```

Manual install, if you prefer it (details also printed by the prompt):

1. Download both tarballs for the latest version (see
   `https://www.makemkv.com/download/history.html`):
   `makemkv-bin-<V>.tar.gz` (precompiled `makemkvcon`) and
   `makemkv-oss-<V>.tar.gz` (libraries source); verify against
   `makemkv-sha-<V>.txt`.
2. `sudo apt-get install -y build-essential pkg-config zlib1g-dev libexpat1-dev libssl-dev libavcodec-dev libavutil-dev`
3. Build/install the libraries from the oss tarball with
   `./configure --disable-gui --disable-qt5 && make -j && sudo make install`.
4. Install `makemkvcon` + data from the bin tarball (`sudo make install`,
   accepting the EULA prompt).
5. Verify: `makemkvcon -r info disc:0`.

Riply re-probes MakeMKV every few seconds, so no restart is needed after
the install.

## Debugging

Requires [Delve](https://github.com/go-delve/delve) (`dlv`). Install with:

```bash
go install github.com/go-delve/delve/cmd/dlv@latest
```

| Mode | Command |
|------|---------|
| Launch with debugger | `dlv debug ./cmd/server` |
| Attach to running process | `dlv attach <PID>` |
| Debug existing binary | `dlv exec ./riply` |
| Headless (remote attach) | `dlv debug ./cmd/server --headless --listen=127.0.0.1:40000 --api-version=2` |

For headless mode, attach with `dlv connect 127.0.0.1:40000`.

## Configuration

Config lives at `$XDG_CONFIG_HOME/riply/config.toml` (default `~/.config/riply/config.toml`) and is created automatically with defaults on first run. `config.toml.example` shows the available fields.

The library database is an index of ripped and scanned media, lives next to the config at `$XDG_CONFIG_HOME/riply/library.db`, and is created automatically. It can be reset from Settings or with `POST /api/library/reset`; resetting never modifies or deletes the media files themselves.

```toml
movies_dir = "/media/movies"
tv_shows_dir = ""
temp_dir = "/tmp/riply"
http_host = "127.0.0.1"
http_port = 3000
makemkv_path = ""
auto_sequence = true
min_title_length = 300

[metadata]
movies = ["tmdb", "omdb"] # try-order for movie discs; first entry is primary
tv = ["tmdb", "omdb"]     # try-order for TV discs; may differ from movies

[providers.tmdb]
api_key = ""

[providers.omdb]
api_key = ""
```

| Field | Description |
|-------|-------------|
| `movies_dir` | Destination directory for ripped movies (Jellyfin-compliant structure) |
| `tv_shows_dir` | Destination directory for ripped TV shows — `Show (Year)/Season N/…` (empty = under `movies_dir`) |
| `temp_dir` | Temporary directory for rip operations |
| `providers.<name>.api_key` | Per-provider API key (`tmdb`, `omdb`); a legacy flat `tmdb_api_key` is migrated into `[providers.tmdb]` automatically. Naming is disabled when no key is configured. |
| `metadata.movies` / `metadata.tv` | Provider try-order per media type; on error or empty results the next provider in the list is tried (providers with keys but absent from a list are appended after it) |
| `http_host` | HTTP server bind address (restart required) |
| `http_port` | HTTP server bind port (restart required) |
| `makemkv_path` | MakeMKV binary path, empty = PATH lookup (restart required) |
| `auto_sequence` | Pre-fill the search modal from the last ripped title and warn about duplicate discs (default true; per-drive override on each drive card) |
| `min_title_length` | Skip disc titles shorter than this many seconds — passed to MakeMKV as `--minlength` (default 300; 0 = rip every title) |

`movies_dir`, `temp_dir`, `providers` and `metadata`, `auto_sequence` and `min_title_length` can also be changed at runtime via `PUT /api/config`; the bind address/port and MakeMKV path take effect on restart. Rate limits are per provider and hard-coded (TMDb ~1 req/s; OMDb 1,000 requests/day free tier) — not configurable.

## Usage

1. Run `./riply`
2. Browser opens to the Riply dashboard
3. Insert a disc — it appears as a drive card after ~2s (polling interval)
4. Click **Rip** — progress shows in real-time
5. When the rip completes, a naming suggestion appears below
6. Click **Accept** to move to your library, or **Search** to pick manually

## Features

- **Drive detection** — single poller every 2s probing `CDROM_DRIVE_STATUS` (ioctl), with a sysfs fallback and a 6s removal debounce
- **MakeMKV integration** — async ripping with progress tracking
- **TMDb auto-naming** — automatic movie/TV identification on disc insert
- **Real-time progress** — SSE-powered live updates in the browser
- **Jellyfin-compliant output** — `{Title} ({Year})/` folder structure with extras; TV shows get `Season N/` subfolders
- **Single binary** — assets embedded and library history stored in a local SQLite file; no Docker
- **Dark theme UI** — vanilla HTML/CSS/JS, no framework

## Architecture

```
cmd/server/        — Main entry point, wires dependencies together
internal/config/   — TOML configuration loading and defaults
internal/drive/    — Drive scanning and state management (NoDisc/Loading/Ready/Ripping/Complete/Error)
internal/rip/      — MakeMKV integration and rip lifecycle
internal/naming/   — TMDb API client with rate limiting
internal/move/     — File organization (temp → library)
internal/sse/      — Server-Sent Events broadcast manager
internal/metrics/  — Atomic counters (total/active/failed rips)
internal/logger/   — slog-based INFO/DEBUG logging
internal/library/  — SQLite library history, library scan, TV next-up
web/               — HTML template + static assets, embedded into the binary
```

## API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/drives` | List all drives with state |
| POST | `/api/drives/{index}/start` | Start rip on drive |
| POST | `/api/drives/{index}/eject` | Eject disc |
| PUT | `/api/drives/{index}/auto-naming` | Toggle auto-naming for a drive |
| PUT | `/api/drives/{index}/auto-sequence` | Toggle Auto-Sequence for a drive |
| PUT | `/api/drives/{index}/name` | Set canonical name / TMDb match for a drive |
| POST | `/api/drives/{index}/accept` | Accept the naming suggestion before/without a rip |
| POST | `/api/drives/{index}/undo-accept` | Cancel a pre-accepted name |
| GET | `/api/rips` | List rips needing naming (newest first) |
| POST | `/api/rips/{id}/cancel` | Cancel an active rip |
| POST | `/api/rips/{id}/accept` | Accept name, move to library (queued while ripping) |
| POST | `/api/rips/{id}/accept-title` | Accept a single title from a multi-title rip |
| POST | `/api/rips/{id}/undo-accept` | Cancel a queued accept |
| GET | `/api/search?q=` | Manual TMDb search |
| GET | `/api/library` | Owned movies/episodes and per-series next-up |
| POST | `/api/library/scan` | Scan `movies_dir` into library history (202; completion via SSE) |
| POST | `/api/library/reset` | Clear the library index; media files are untouched |
| GET | `/api/config` | Get current config (API key never returned) |
| PUT | `/api/config` | Update config |
| GET | `/api/status` | Backend status — Riply version + MakeMKV availability + version |
| GET | `/api/metrics` | JSON metrics |
| GET | `/events` | SSE event stream (see below) |
| GET | `/` | Web UI (single page) |
| GET | `/static/…` | Embedded UI assets |

## SSE Events

| Event | Description |
|-------|-------------|
| `drive_loading` | Disc inserted, name being resolved |
| `drive_ready` | Disc detected and named |
| `drive_ejected` | Disc ejected |
| `rip_progress` | Rip progress update |
| `rip_complete` | Rip finished |
| `rip_failed` | Rip failed |
| `rip_cancelled` | Rip cancelled by user or shutdown |
| `rip_accepted` | Moved to library |
| `rip_duplicate` | Disc was ripped before; warning toast (never blocks the rip) |
| `naming_started` | Auto-naming TMDb search started (`startAutoNaming`) |
| `naming_match` | Auto-naming found a TMDb match (`startAutoNaming`) |
| `naming_failed` | Auto-naming found no match or errored (`startAutoNaming`) |
| `drive_accepted` | Drive-level name accepted |
| `library_scanned` | Library scan finished |
| `library_scan_failed` | Library scan errored |
| `library_reset` | Library index cleared; media files untouched |
| `shutdown` | Server is shutting down |

## Diagnostics

| Switch | Effect |
|--------|--------|
| `RIPLY_DEBUG=1` | Enables per-line DEBUG logs on stderr, each with `file:line` source info |
| `RIPLY_MAKEMKV_LOG=/path/to/makemkv.log` | Appends every raw `makemkvcon` robot-mode line to the file |

On a failed rip, `Rip` also logs the tail of the captured MakeMKV output alongside the error.

## Known Limitations

- **Rip history** covers accepted rips and scanned files; rips still in progress are not persisted across restarts
- **No authentication** — keep the bind address on loopback unless you protect it yourself; the default is `127.0.0.1`
- **Settings directory browser** lists every directory on the filesystem (no restriction); acceptable because Riply is a localhost, single-user service
- **Movie/TV classification** is the file-size heuristic in `classifyRip` (median size plus min/max ratio)
- **Library scans** run on Riply's single SQLite connection, so a scan briefly delays the drive and library views until it finishes

## License

MIT
