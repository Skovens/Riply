# Riply — Product Specification

Riply is a DVD/Blu-ray ripping and media organizer. Insert a disc, click Rip, and the
content is ripped, identified, and organized into a Jellyfin-compliant library. Unidentified
discs can be reviewed and named manually in the Organizer.

> **Rebuild of [MediaAtlas](https://github.com/Skovens/MediaAtlas)** — prioritizing memory
> safety, concurrency, testability, and clean architecture.

## Table of Contents

- [The Three Windows](#the-three-windows) — Rip, Organizer, Settings
- [User Flows](#user-flows) — Rip flow, Organize flow, Media type detection
- [Tmp Folder Lifecycle](#tmp-folder-lifecycle) — Cleanup policy
- [Deprecated](#deprecated) — What was removed
- [API Endpoints](#api-endpoints) — All routes
- [Frontend Routing](#frontend-routing) — URL → window mapping
- [Tech Stack](#tech-stack) — Technologies used
- [Quick Start](#quick-start) — Prerequisites, build, run, config
- [Roadmap](#roadmap) → [improvements.md](../improvements.md)

## Where to Start

| Role | Read |
|------|------|
| **New developer** | This doc + `AGENTS.md` |
| **Understanding architecture** | `docs/adr/` (technical decisions) |
| **Understanding implementation** | `docs/designs/` + source code |
| **Understanding frontend** | `web/svelte/ARCHITECTURE.md` |
| **Planning work** | `improvements.md` |

## The Three Windows

Riply has three distinct windows, each with a single purpose.

### 1. Rip (`/ripper`) — Disc Ripping

One window, one job per drive.

**Drive cards** show per-drive state:

| State | Display |
|-------|---------|
| `Ready` | "Insert disc to rip" + Rip button active |
| `Ripping` | Progress bar + stage name + ETA |
| `Complete` | Disc cover + auto-detected name + type |
| `Error` | "Ripping failed: [reason]" + descriptive error |

**Drive card buttons** — always visible, greyed out with tooltip when inactive:

| Button | Active when | Inactive tooltip |
|--------|-------------|------------------|
| Rip | Disc inserted, not ripping | "No disc in drive" |
| Eject | Disc inserted, not ripping | "No disc to eject" |
| View in Organizer | Unidentified rip | "Already named" |

**Autorip** — per-drive toggle. When enabled, auto-starts rip when disc inserted.
Autorip state is session-scoped (cleared on restart for safety).

**Auto-naming** — runs in background after rip starts, independently:
- Parse disc name → search TMDb for title + year
- Live status display: `Disc: Inception → Found: Inception (2010)` or `Disc: Inception → Not found`
- On rip completion, folder is renamed to Jellyfin-compliant name if identified
- Unidentified rips show "Unidentified — view in Organizer"

**Naming rules** (Jellyfin-compliant output):

| Type | Destination | Main File | Extras |
|------|-------------|-----------|--------|
| Movie | `{Title} ({Year})/` | `{Title}.{ext}` | `{Title} ({Year})/Extras/` |
| TV Show | `{Show} ({Year})/Season {NN}/` | `{Show} - S{NN}E{NN}.{ext}` | `{Show} ({Year})/Season {NN}/Extras/` |

### 2. Organizer (`/organizer`) — Folder Naming and Moving

Handles folder-level media naming and moving.

**Layout:**
- Scan directory selector + "Scan" button
- Folder list: current name, suggested name, confidence, type, action
- Expandable folders: shows files inside, biggest file = main title
- Actions: Review, Accept All, Reject All

**Confidence scoring** (per folder):

| Score | Meaning | Action |
|-------|---------|--------|
| Confident (≥0.9) | TMDb match, year confirmed | Auto-accept, move on "Accept All" |
| Uncertain (0.7–0.9) | TMDb match, year missing/ambiguous | Manual review required |
| Unknown (<0.7) | No TMDb match | Manual search or reject |

**Folder pipeline states:**

```
Scanned → Confident → Moved
        → Uncertain → Moved (manual) | Unknown → Moved (manual) | Quarantined
```

| State | Meaning |
|-------|---------|
| `Scanned` | Folder found, not yet analyzed |
| `Confident` | TMDb match with year (score ≥ 0.9) |
| `Uncertain` | TMDb match without year (score 0.7–0.9) |
| `Unknown` | No TMDb match (score < 0.7) |
| `Moved` | Folder moved to library (terminal) |
| `Quarantined` | Unknown folder moved to quarantine (terminal) |

### 3. Settings (`/settings`) — Configuration

| Category | Settings |
|----------|----------|
| Ripping | Tmp folder path, MakeMKV command, autorip toggle per drive |
| Library | Movie destination folder, TV show destination folder, tmp cleanup age (days) |
| Metadata | TMDb API key, OMDB API key, search language |
| Advanced | Log level, debug options |

---

## User Flows

### Rip Flow
1. Insert disc → drive shows Ready
2. Click Rip (or autorip triggers)
3. Rip starts, auto-naming runs in background concurrently
4. Rip completes → folder moved to library if identified
5. If unidentified → drive card shows "View in Organizer"

### Organize Flow
1. Click Scan in Organizer
2. Folders appear with confidence scores
3. Accept All (moves confident), Review (handles uncertain), Reject All (quarantines unknown)

### Media Type Detection
Detection priority:
1. **TMDb match** (most accurate) — result type = movie or tv_show
2. **File-size heuristic** (fallback) — one large file = movie, similar sizes = TV
3. **User confirmation** (always required) — one-click flip between Movie/TV

---

## Tmp Folder Lifecycle

Ripped content is written to a configurable tmp folder (default `/tmp/riply`).

| Scenario | Action |
|----------|--------|
| Rip completes successfully | Content moved to library destination |
| Rip completes, unidentified | Content stays in tmp with disc name |
| Rip fails | Partial content stays in tmp |
| Server restart | Content older than N days deleted |
| Content < N days old | Kept, regardless of state |

Configuration: `temp_directory` (default `/tmp/riply`), `tmp_cleanup_age_days` (default `7`).

---

## Deprecated

The following are removed by this spec:
- Dashboard stats bar (Scanned, Matched, NeedsReview, Confirmed, Moved)
- File-based pipeline (SHA-256 hashing, ffprobe per-file, 5-state machine)
- Review queue sidebar
- DetailDrawer component
- TitleGroup per folder
- Confidence scoring per file
- Buttons that hide/show based on state

---

## API Endpoints

All endpoints use the `/v1/` prefix.

### Ripper

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/ripper/drives` | List available drives |
| POST | `/v1/ripper/drives/:index/rename` | Rename disc (user-set title/year) |
| GET | `/v1/ripper/status` | Check MakeMKV installation status |
| POST | `/v1/ripper/:index/start` | Start ripping |
| POST | `/v1/ripper/:index/cancel` | Cancel rip |
| POST | `/v1/ripper/:index/eject` | Eject disc |
| GET | `/v1/ripper/autorip` | Get autorip-enabled drives |
| POST | `/v1/ripper/autorip/:index/toggle` | Toggle autorip for drive |

### Organizer (Folders)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/folders` | List folders (optional `?status=` filter) |
| GET | `/v1/folders/stats` | Get folder counts by status |
| POST | `/v1/folders/scan` | Scan configured directory |
| GET | `/v1/folders/:id` | Get single folder |
| DELETE | `/v1/folders/:id` | Delete folder |
| POST | `/v1/folders/:id/analyze` | Run TMDb search for folder |
| POST | `/v1/folders/:id/accept` | Accept folder → Confident |
| POST | `/v1/folders/:id/reject` | Reject folder → Unknown |
| POST | `/v1/folders/accept-all` | Move all Confident folders |
| POST | `/v1/folders/reject-all` | Move all Unknown folders to quarantine |
| POST | `/v1/folders/:id/move` | Move single confident folder |
| GET | `/v1/folders/:id/preview` | Preview move destination |

### Config

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/config` | Get current settings |
| PUT | `/v1/config` | Update settings |
| POST | `/v1/config/reset` | Reset to defaults |

### Jobs

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/jobs` | List jobs |
| GET | `/v1/jobs/:id` | Get job status |
| POST | `/v1/jobs/:id/cancel` | Cancel job |

### SSE & Health

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/sse` | Server-Sent Events stream |
| GET | `/health` | Health check |
| GET | `/ready` | Readiness check (database ping) |
| GET | `/metrics` | Prometheus metrics |

---

## Frontend Routing

| Route | Window |
|-------|--------|
| `/` | Redirects to `/ripper` |
| `/ripper` | Rip window — drive cards, rip controls |
| `/organizer` | Organizer window — folder list, naming, bulk move |
| `/settings` | Settings window — configuration |

Navigation via SvelteKit `<a>` links — no hash-based routing.

---

## Tech Stack

| Concern | Technology |
|---------|-----------|
| Language | Rust |
| Runtime | Tokio |
| Database | SQLite + sqlx |
| API | Axum |
| Frontend | SvelteKit + Tailwind CSS |
| Metadata | TMDb / OMDb |
| DVD Ripping | MakeMKV |

---

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
make            # builds frontend + release, symlinks ~/.local/bin/riply
make install    # same, explicit
```

Or manually:

```bash
cargo build --release
```

### Run

```bash
# Web server (default: http://localhost:3000)
cargo run -p cli -- serve

# CLI
cargo run -- scan /path/to/media
cargo run -- analyze
cargo run -- review
cargo run -- move --movies-dir /path/to/movies --tv-dir /path/to/tv-shows
cargo run -- rip
```

### Configuration

Default config file: `config.toml`

```toml
[scan]
input_dir = "/path/to/incoming"
movies_dir = "/path/to/movies"
tv_dir = "/path/to/tv-shows"
db_path = "riply.db"

[ripping]
tmp_dir = "/tmp/riply"
tmp_cleanup_age_days = 7
autorip_eject_after = false

[api]
tmdb_enabled = true
tmdb_api_key = "your-tmdb-api-key"
omdb_enabled = false
omdb_api_key = "your-omdb-api-key"

[settings]
verbose = false
```

---

## Roadmap

See [improvements.md](../improvements.md) for the full project roadmap.
