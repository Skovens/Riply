# Design: File Pipeline

## Purpose
Scan media files from disk, analyze them to identify titles/episodes, present low-confidence matches for manual review, and move confirmed files into an organized library. This document describes the *current* behavior of the pipeline.

---

## Pipeline State Machine

```
                    ┌──────────┐
                    │  Scanned │
                    └────┬─────┘
                         │
              ┌──────────┼──────────────┐
              ▼          ▼              ▼
         ┌─────────┐ ┌──────────┐ ┌────────┐
         │  Matched │ │ NeedsReview│ │ Failed │
         └────┬─────┘ └────┬─────┘ └────────┘
              │             │
              │     ┌──────┴──────┐
              │     ▼             ▼
              │  ┌────────┐   ┌────────┐
              │  │ Confirmed│  │ Failed │
              │  └────┬───┘   └────────┘
              │       │
              ▼       │
         ┌──────────┐ │
         │   Moved  │◄┘
         └──────────┘
```

### States

| State | Meaning | Review Queue |
|---|---|---|
| `Scanned` | File found on disk, no analysis yet | Not shown |
| `Matched` | High-confidence title match (≥0.8) | Not shown |
| `NeedsReview` | Low-confidence match (<0.8), needs human | **Shown** |
| `Failed` | No match found | Not shown (can be reset to NeedsReview) |
| `Confirmed` | User accepted the match | Not shown |
| `Moved` | File moved to final library location | Terminal state |

### Valid Transitions

| From | To | Trigger |
|---|---|---|
| `Scanned` | `Matched` | Analyzer confidence ≥ 0.8 |
| `Scanned` | `NeedsReview` | Analyzer confidence < 0.8 |
| `Scanned` | `Failed` | No match found (edge case) |
| `Matched` | `Confirmed` | User accepts match |
| `Matched` | `NeedsReview` | Reset for re-review |
| `NeedsReview` | `Confirmed` | User accepts after review |
| `NeedsReview` | `Failed` | User rejects |
| `Failed` | `NeedsReview` | Reset for re-review |
| `Confirmed` | `Moved` | File successfully moved |

### Helper Methods

```rust
PipelineStatus::is_terminal()   // true only for Moved
PipelineStatus::requires_review() // true only for NeedsReview
PipelineStatus::is_failed()     // true only for Failed
PipelineStatus::is_successful() // Matched, Confirmed, Moved
```

---

## File Record

### `FileRecord` struct

```rust
pub struct FileRecord {
    pub id: String,                    // First 16 hex chars of SHA-256 hash
    pub path: PathBuf,                 // Full path on disk
    pub filename: String,              // File name only
    pub size: Option<i64>,             // File size in bytes
    pub duration: Option<i64>,         // Duration in seconds (ffprobe)
    pub hash: String,                  // Full SHA-256 hex digest
    pub status: PipelineStatus,        // Current pipeline state
    pub suggested_title: Option<String>,    // Title from analysis
    pub suggested_year: Option<u32>,
    pub confidence: Option<f64>,         // 0.0 to 1.0
    pub canonical_title: Option<String>,  // User-confirmed title
    pub canonical_year: Option<u32>,
    pub tmdb_id: Option<i32>,            // TMDb ID
    pub content_type: String,            // "main" or "extra"
    pub media_type: MediaType,           // Movie or Tv
    pub series_name: Option<String>,     // TV series name
    pub season_number: Option<u32>,      // TV season
    pub episode_number: Option<u32>,     // TV episode
    pub episode_title: Option<String>,   // TV episode title
    pub source: Option<String>,          // "tmdb", "omdb", or "Manual"
    pub subtitle_text: Option<String>,   // OCR-extracted subtitles
    pub duplicate_path: Option<PathBuf>, // Duplicate file path
    pub title_group_id: Option<String>,  // TitleGroup ID (folder-first scan)
    pub genres: Option<Vec<String>>,     // Enriched genres
    pub runtime: Option<i32>,            // Enriched runtime (minutes)
    pub content_rating: Option<String>,  // Enriched rating (e.g. "PG-13")
    pub overview: Option<String>,        // Enriched plot
    pub cast_list: Option<Vec<String>>,  // Enriched cast
    pub reviewed: bool,                  // Has been reviewed
    pub created_at: String,              // RFC3339 timestamp
    pub updated_at: String,              // RFC3339 timestamp
}
```

### ID Generation

The `id` is the first 16 hex characters of the SHA-256 hash of the file contents. This means:
- Files with identical content get identical IDs (even if different filenames)
- The ID is deterministic and content-based, not path-based
- Two files with the same hash in different locations are considered duplicates

### Status Updates

`update_status(new_status)` sets the status and updates `updated_at` to the current RFC3339 timestamp.

---

## Phase 1: Scan

### `ScannerService::scan_directory()` — `POST /v1/scan`

**Entry point:** `POST /v1/scan` → `scan_directory()`

**Behavior:**
1. Checks if directory exists (error if not)
2. Reads immediate subdirectories to determine scan mode:
   - **Folder-first mode:** If immediate subdirectories exist, each is treated as a `TitleGroup`
   - **Flat mode:** No subdirectories → recursive scan of all media files

**Folder-first mode:**
1. Scans root-level files (adds to records without a title group)
2. For each immediate subdirectory:
   - Cleans directory name with `NameCleaner::clean_name()`
   - Optionally searches TMDb for the directory name (if metadata searcher available)
   - Creates a `TitleGroup` with the cleaned/matched title
   - Recursively walks all files within the directory (including nested subdirs)
   - Assigns `title_group_id` to each file

**Flat mode:**
1. Recursively walks directory for media files
2. No title groups created

**Media file extensions:** `mkv`, `mp4`, `avi`, `mov`, `m4v`, `webm`

**Per-file processing:**
1. Computes SHA-256 hash (via `spawn_blocking`)
2. Reads file size
3. Creates `FileRecord` with `id` = first 16 hex chars of hash
4. Status set to `Scanned`

**Return:** `ScanResult { title_groups, records }`

### `TitleGroup`

```rust
pub struct TitleGroup {
    pub id: String,              // SHA-256 of directory path (first 16 hex chars)
    pub path: PathBuf,
    pub title: String,           // Cleaned directory name or TMDb-matched title
    pub year: Option<u16>,
    pub tmdb_id: Option<i32>,
    pub status: TitleGroupStatus, // Scanned, Matched, Confirmed, Failed
}
```

---

## Phase 2: Analyze

### `AnalyzerService::analyze_file()` — `POST /v1/analyze`

**Entry point:** `POST /v1/analyze` → `analyze_files()` → `analyze_file()`

**Behavior per file:**

1. **Extract metadata via ffprobe** (non-blocking):
   - `duration` from `duration_secs`
   - `content_type` from ffprobe content type detection
   - Gracefully skips if ffprobe unavailable

2. **Parse filename** for title, year, and TV episode info:
   - Strips extension, normalizes separators (`.`, `_` → space)
   - Matches patterns: `S01E05`, `S01E05E06` (multi-ep), `S01`, `1x05`
   - Extracts year (19xx or 20xx)
   - Removes episode/year markers from title for cleaner search

3. **Metadata search** (if searcher available and not an extra):
   - Searches TMDb with parsed title + year
   - On match: sets `suggested_title`, `suggested_year`, `tmdb_id`, `confidence=0.95`, `source="tmdb"`
   - For TV: sets `series_name`, `season_number`, `episode_number`, `episode_title`
   - Enriches with genres, runtime, content_rating, overview, cast_list
   - Sets status to `Matched`
   - On search failure: falls through to filename-only heuristic

4. **Filename-only heuristic** (fallback):
   - Confidence scoring:
     - Title > 3 chars: +0.5
     - Year detected: +0.3
     - Filename contains `.`: +0.2
   - TV episodes get minimum confidence of 0.85
   - Confidence ≥ 0.8 → `Matched`
   - Confidence < 0.8 → `NeedsReview`

5. **Extras** (`content_type == "extra"`):
   - Skips metadata search entirely
   - Uses filename-only heuristic

### Filename Parsing Patterns

| Pattern | Example | Result |
|---|---|---|
| `SxxExx` | `Show.S01E05.mkv` | title="Show", season=1, episode=5 |
| `SxxExxEyy` | `Show.S01E05E06.mkv` | title="Show", season=1, episode=5 (first) |
| `Sxx` | `Show.S01.mkv` | title="Show", season=1 (season pack) |
| `NxNN` | `Show.1x05.mkv` | title="Show", season=1, episode=5 |
| Year | `Movie.2023.mkv` | title="Movie", year=2023 |

### Confidence Thresholds

| Confidence | Status |
|---|---|
| ≥ 0.95 (TMDb match) | `Matched` |
| ≥ 0.85 (TV episode heuristic) | `Matched` |
| ≥ 0.8 (generic heuristic) | `Matched` |
| < 0.8 | `NeedsReview` |

---

## Phase 3: Review

### Review Queue

**Endpoint:** `GET /v1/review` → `list_review_items()`

**Behavior:**
1. Queries database for files with `status == NeedsReview`
2. Returns list sorted by confidence ascending (lowest first)
3. Each item includes: `id`, `filename`, `suggested_title`, `canonical_title`, `confidence`, `media_type`

### Accept

**Endpoint:** `POST /v1/review/:id/accept` → `accept_item()`

**Behavior:**
1. Updates file with `canonical_title`, `canonical_year`, `series_name`, `season_number`, `episode_number`, `media_type` from the review item
2. Sets `status = Confirmed`
3. Sets `reviewed = true`
4. Returns updated file

### Reject

**Endpoint:** `POST /v1/review/:id/reject` → `reject_item()`

**Behavior:**
1. Sets `status = Failed`
2. Sets `reviewed = true`
3. Returns updated file

### Name Review Item (from dashboard)

**Endpoint:** `POST /v1/files/:id` (update) → `update_file()`

**Behavior:**
1. Opens media naming modal with `canonical_title` pre-filled
2. User searches for correct media via TMDb/OMDb
3. On selection: updates file with canonical metadata
4. Automatically accepts (sets `status = Confirmed`)

---

## Phase 4: Move

### Preview Move

**Endpoint:** `POST /v1/move/preview` → `preview_move()`

**Behavior:**
1. Queries database for files with `status == Confirmed`
2. For each file, calculates destination path (without moving)
3. Returns list of `{ filename, source_path, dest_path, media_type }`

### Execute Move

**Endpoint:** `POST /v1/move/execute` → `execute_move()`

**Behavior:**
1. For each confirmed file:
   - Calls `MoverService::move_file()`
   - Updates status to `Moved`
   - Saves to database
2. Returns summary of moved files

### Destination Path Calculation

**Movies (Jellyfin convention):**
```
{movies_dir}/{Title} ({Year})/{Title}.{ext}
```
- Main title file is **renamed** to `{Title}.{ext}` (e.g., `Inception.mkv`)
- Year added to directory name if available
- Extras go into `{movies_dir}/{Title} ({Year})/Extras/` (Jellyfin convention: capitalized `Extras/`)
- Supported extras: behind-the-scenes, trailers, interviews, deleted scenes

**TV (Jellyfin convention):**
```
{tv_dir}/{Series} ({Year})/Season {NN}/{Title} - S{NN}E{NN}.{ext}
```
- Main title file is **renamed** to `{Title} - S{NN}E{NN}.{ext}` (e.g., `Breaking Bad - S01E01.mkv`)
- Season zero-padded (e.g., `Season 01`)
- Year added to series name if available
- Extras go into `Season {NN}/Extras/` (Jellyfin convention: capitalized `Extras/`)
- Series name from metadata; falls back to title

**Fallback:** If `canonical_title` unavailable, uses `suggested_title`

### Filename Renaming

During the move step, the original filename is replaced with a Jellyfin-compatible name:

**Movies:**
- Extract extension from original filename (`inception_2010.1080p.bluray.x264.mkv` → `.mkv`)
- Use `canonical_title` (or `suggested_title` as fallback) as the new base name
- Result: `Inception.mkv`

**TV episodes:**
- Extract extension from original filename
- Use `canonical_title` (or `suggested_title` as fallback) as the show name
- Format: `{Title} - S{season:02}E{episode:02}.{ext}`
- Result: `Breaking Bad - S01E01.mkv`

**Extras:**
- Keep original filename (extras are not renamed)
- Only the destination folder changes (moved into `Extras/` subfolder)

### Extras Detection

The `content_type` field distinguishes main titles from extras:

- **Detection via ffprobe:** Duration-based classification
  - Duration > 3600s (1 hour) → `content_type = "main"`
  - Duration ≤ 3600s → `content_type = "extra"`
- **Manual override:** User can set `content_type` during review
- **Extras skip metadata search:** Files with `content_type == "extra"` bypass TMDb/OMDb search to avoid false matches on commentary/behind-the-scenes content

---

## Database Layer

### SQLite Schema (migrations 001–004)

```sql
CREATE TABLE files (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    filename TEXT NOT NULL,
    size INTEGER,
    duration INTEGER,
    hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Scanned',
    suggested_title TEXT,
    suggested_year INTEGER,
    confidence REAL,
    canonical_title TEXT,
    canonical_year INTEGER,
    tmdb_id INTEGER,
    content_type TEXT NOT NULL DEFAULT 'unknown',
    media_type TEXT NOT NULL DEFAULT 'Movie',
    series_name TEXT,
    season_number INTEGER,
    episode_number INTEGER,
    episode_title TEXT,
    source TEXT,
    subtitle_text TEXT,
    duplicate_path TEXT,
    title_group_id TEXT,
    genres TEXT,           -- JSON array
    runtime INTEGER,
    content_rating TEXT,
    overview TEXT,
    cast_list TEXT,        -- JSON array
    reviewed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_files_status ON files(status);
```

### Key Queries

| Operation | Method | Notes |
|---|---|---|
| List files | `get_files_paginated(status?, search?, page, per_page)` | Supports status filter, search (filename), pagination |
| Get file | `get_file_by_id(id)` | Returns single file |
| Update file | `update_file(&file)` | Full record replacement |
| Delete file | `delete_file(id)` | Soft delete (removes from DB) |
| Count by status | `count_by_status()` | Used for dashboard stats |
| Clear all | `clear_all()` | Drops and recreates table |

### Enrichment Columns

Genres and cast are stored as JSON-encoded TEXT using `cast_list` (not `CAST` — which is a SQLite keyword).

---

## API Endpoints

### Files

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/files` | `list_files` | Paginated file list with filters |
| GET | `/v1/files/:id` | `get_file` | Single file by ID |
| POST | `/v1/files/:id` | `update_file` | Update canonical metadata |
| DELETE | `/v1/files/:id` | `delete_file` | Delete file record |
| DELETE | `/v1/files` | `reset_db` | Wipe all data |

**List params:** `status`, `search`, `page`, `per_page`

### Review

| Method | Path | Handler | Description |
|---|---|---|---|
| GET | `/v1/review` | `list_review_items` | Files needing review |
| POST | `/v1/review/:id/accept` | `accept_item` | Accept match → Confirmed |
| POST | `/v1/review/:id/reject` | `reject_item` | Reject → Failed |

### Move

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/move/preview` | `preview_move` | Where files would move |
| POST | `/v1/move/execute` | `execute_move` | Move confirmed files |

### Scan

| Method | Path | Handler | Description |
|---|---|---|---|
| POST | `/v1/scan` | `scan_directory` | Scan directory |

---

## Job Scheduler

### `JobScheduler` — `app/src/jobs.rs`

**Purpose:** Track long-running operations (scan, analyze, move) with progress and cancellation.

**Job kinds:** `Scan`, `Analyze`, `Move`, `Rip`

**States:** `Queued`, `Running`, `Complete`, `Cancelled`, `Failed`

**Progress:** `ProgressHandle` with `processed` and `total` counters.

**SSE events:**
| Event | Format |
|---|---|
| `scan:progress` / `analyze:progress` / `move:progress` | Job progress |
| `scan:starting` / `analyze:starting` / `move:starting` | Job started |
| `scan:complete` / `analyze:complete` / `move:complete` | Job finished |
| `scan:cancelled` / `analyze:cancelled` / `move:cancelled` | Job cancelled |

---

## Frontend Display

### Dashboard (`+page.svelte`)

**Stats bar:** Shows counts for Scanned, Matched, NeedsReview, Confirmed, Moved.

**Pipeline bar:** Visual flow Scanned → Matched → Review → Confirmed → Moved with clickable counts.

**Files panel:** Table showing up to 50 files with filename, media type, status badge. Clicking opens `DetailDrawer`.

**Review queue:** Sidebar panel listing items needing review with confidence %, "Name" (opens naming modal), "Accept", and "Reject" buttons.

**Jobs panel:** Sidebar panel listing active jobs with status, progress bar, and cancel button.

**DetailDrawer:** Slide-out panel showing full file metadata, with Confirm/Reject buttons for review items.

### SSE Event Handling

| Event | Handler |
|---|---|
| `rip:*` | Update rip progress bars |
| `drive:*` | Re-fetch drives |
| `scan:*` / `analyze:*` / `move:*` | Refresh jobs + files |
| `review:accepted` / `review:rejected` | Refresh review queue + stats |

---

## Where This Lives

| Concern | Layer |
|---|---|
| Pipeline states & transitions | `core/src/pipeline.rs` |
| File record definition | `core/src/file_record.rs` |
| Title groups | `core/src/title_group.rs` |
| File scanning | `app/src/scanner.rs` |
| File analysis | `app/src/analyzer.rs` |
| File moving | `app/src/mover.rs` |
| Job scheduling | `app/src/jobs.rs` |
| FFprobe extraction | `app/src/ffprobe.rs` |
| File API | `web/src/routes/files.rs` |
| Review API | `web/src/routes/review.rs` |
| Move API | `web/src/routes/move.rs` |
| Scan API | `web/src/routes/scan.rs` |
| Database layer | `infra/src/db.rs` |
| Dashboard UI | `web/svelte/src/routes/+page.svelte` |
| Files UI | `web/svelte/src/routes/files/+page.svelte` |
| Detail drawer | `web/svelte/src/lib/components/DetailDrawer.svelte` |

---

## Notes

- **Database is ephemeral:** SQLite DB is disposable — can be rebuilt via re-scan. Schema changes are made via versioned migrations (001–004).
- **Duplicate detection:** Files with identical content (same SHA-256 hash) get identical IDs. The `duplicate_path` field tracks known duplicates.
- **Confidence is a heuristic:** 0.8 threshold is arbitrary. TMDb matches get 0.95; filename-only matches score 0.5–1.0 based on heuristics.
- **TV detection:** Multiple patterns supported (SxxExx, NxNN). Multi-episode files (S01E05E06) take the first episode number.
- **Extras skip metadata search:** Files with `content_type == "extra"` bypass TMDb/OMDb search to avoid false matches on commentary/behind-the-scenes content.
- **Cross-device move:** `fs::rename()` used first; falls back to `copy` + `delete` with size verification on `CrossesDevices` error.
- **Folder-first scan:** Immediate subdirectories are treated as title groups. Root-level files without a group are included but have `title_group_id = None`.
- **Jellyfin naming:** Movies: `{Title}.{ext}` in `{Title} ({Year})/`. TV: `{Title} - S{NN}E{NN}.{ext}` in `{Series} ({Year})/Season {NN}/`. Extras kept as-is in `Extras/` subfolder.
- **Extras detection:** Duration-based — main titles > 3600s, extras ≤ 3600s. User can override during review.
