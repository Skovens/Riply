# ADR-026: Folder-Based Pipeline

## Status

Proposed

## Context

The current file-based pipeline (ADR-009, `docs/designs/file-pipeline.md`) processes individual files through a 5-state machine:

```
Scanned → Matched/NeedsReview/Failed → Confirmed → Moved
```

Each file is hashed (SHA-256), analyzed (ffprobe), scored (confidence), and tracked individually. This creates:
- Database bloat (one record per file, not per folder)
- Complex state management (5 states, 7 transitions)
- Unnecessary overhead (hashing every file, probing every file)
- UI complexity (file table, detail drawer, per-file review)

For DVD/Blu-ray ripping, users think in folders, not files. A disc produces one folder. The question is how that folder is named and where it goes.

Options considered:
1. **Keep file-based pipeline** — Current approach, works but over-engineered
2. **Folder-based pipeline** — New approach, simpler, matches user mental model
3. **Hybrid** — Folder-level tracking with file-level details on demand

## Decision

Use a **folder-based pipeline** for DVD ripping and the Organizer window.

### Pipeline States

```
          ┌──────────┐
          │ Scanned  │
          └────┬─────┘
               │
        ┌──────┼──────┐
        ▼      ▼      ▼
   ┌────────┐ ┌───────┐ ┌────────┐
   │Confident│ │Uncertain│ │Unknown │
   └───┬────┘ └───┬───┘ └───┬────┘
       │          │         │
       │   ┌──────┼──────┐  │
       │   ▼      ▼      ▼  │
       │  Accept  Manual  │  │
       │   │      │      │  │
       │   └──────┼──────┘  │
       │          │         │
       ▼          ▼         ▼
   ┌────────┐ ┌────────┐ ┌─────────┐
   │  Moved │ │  Moved │ │ Quarantined│
   └────────┘ └────────┘ └─────────┘
```

### State Definitions

| State | Meaning | Action |
|-------|---------|--------|
| `Scanned` | Folder found in scan directory, not yet analyzed | Default state after scan |
| `Confident` | TMDb match found, year confirmed (score ≥ 0.9) | Auto-accept, move on "Accept All" |
| `Uncertain` | TMDb match found, year missing/ambiguous (score 0.7–0.9) | Manual review required |
| `Unknown` | No TMDb match (score < 0.7) | Manual search or reject |
| `Moved` | Folder moved to library destination | Terminal |
| `Quarantined` | Unknown folder moved to quarantine subfolder | Terminal |

### Valid Transitions

| From | To | Trigger |
|------|-----|---------|
| `Scanned` | `Confident` | TMDb match with year |
| `Scanned` | `Uncertain` | TMDb match without year |
| `Scanned` | `Unknown` | No TMDb match |
| `Confident` | `Moved` | User clicks "Accept All" |
| `Uncertain` | `Moved` | User manually accepts after review |
| `Uncertain` | `Unknown` | User rejects uncertain |
| `Unknown` | `Moved` | User manually accepts after search |
| `Unknown` | `Quarantined` | User clicks "Reject All" |

### Folder Record

```rust
pub struct FolderRecord {
    pub id: String,                    // SHA-256 of folder path (first 16 hex chars)
    pub path: PathBuf,                 // Full path on disk
    pub name: String,                  // Folder name
    pub suggested_name: Option<String>,// Suggested new name
    pub suggested_year: Option<u32>,
    pub confidence: Option<f64>,       // 0.0 to 1.0
    pub tmdb_id: Option<i32>,
    pub media_type: MediaType,         // Movie or Tv
    pub content_type: String,          // "main" or "extra" (per file)
    pub file_count: usize,             // Number of files in folder
    pub main_file: Option<String>,     // Biggest file (main title)
    pub extra_files: Vec<String>,      // Other files
    pub status: PipelineStatus,        // Current pipeline state
    pub created_at: String,            // RFC3339 timestamp
    pub updated_at: String,            // RFC3339 timestamp
}
```

### Naming Rules

**Movies:**
- Destination: `{Title} ({Year})/`
- Main file: `{Title}.{ext}` (biggest file in folder)
- Extras: `{Title} ({Year})/Extras/` (all other files)

**TV Shows:**
- Destination: `{Show} ({Year})/Season {NN}/`
- Files: `{Show} - S{NN}E{NN}.{ext}`
- Extras: `{Show} ({Year})/Season {NN}/Extras/`

### Confidence Scoring

Confidence is calculated per folder, not per file:

| Factor | Score |
|--------|-------|
| TMDb match with year | 0.95 |
| TMDb match without year | 0.75 |
| Disc name matches TMDb title (no year) | 0.70 |
| No TMDb match | 0.00 |

Thresholds:
- ≥ 0.9 → `Confident`
- ≥ 0.7 → `Uncertain`
- < 0.7 → `Unknown`

### API Endpoints

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/organizer/folders` | `list_folders` | List all folders in scan directory |
| GET | `/v1/organizer/folders/:id` | `get_folder` | Single folder by ID |
| POST | `/v1/organizer/scan` | `scan_directory` | Scan configured directory |
| POST | `/v1/organizer/:id/search` | `search_folder` | Manually search TMDb for folder |
| POST | `/v1/organizer/:id/accept` | `accept_folder` | Accept → Confident → Moved |
| POST | `/v1/organizer/:id/reject` | `reject_folder` | Reject → Unknown → Quarantined |
| POST | `/v1/organizer/accept-all` | `accept_all` | Move all Confident folders |
| POST | `/v1/organizer/reject-all` | `reject_all` | Move all Unknown folders to quarantine |

### Database Schema

```sql
CREATE TABLE folders (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    suggested_name TEXT,
    suggested_year INTEGER,
    confidence REAL,
    tmdb_id INTEGER,
    media_type TEXT NOT NULL DEFAULT 'unknown',
    file_count INTEGER NOT NULL DEFAULT 0,
    main_file TEXT,
    extra_files TEXT,              -- JSON array
    status TEXT NOT NULL DEFAULT 'Scanned',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_folders_status ON folders(status);
```

### Consequences

**Positive:**
- Simpler pipeline — 6 states instead of 5, but folder-level not file-level
- No per-file hashing or probing — faster, less I/O
- Database is smaller — one record per folder, not per file
- UI is simpler — folder list, not file table
- Matches user mental model — users think in folders

**Negative:**
- File-level details (duration, hash, content type) are lost — acceptable for DVD ripping
- No duplicate detection across folders — acceptable, folders are already organized
- Less granular control — users can't accept/reject individual files

### What Gets Replaced

| Old | New |
|-----|-----|
| `FileRecord` struct | `FolderRecord` struct |
| `PipelineStatus` (5 states) | `FolderStatus` (6 states) |
| `ScannerService::scan_directory()` | `OrganizerService::scan_directory()` |
| `AnalyzerService::analyze_file()` | `OrganizerService::analyze_folder()` |
| `MoverService::execute_move()` | `OrganizerService::move_folder()` |
| `/v1/files` endpoints | `/v1/organizer/folders` endpoints |
| `/v1/review` endpoints | `/v1/organizer/:id/accept` + `/:id/reject` |
| `/v1/move/preview` + `/v1/move/execute` | `/v1/organizer/accept-all` |

## Supersedes

- `docs/designs/file-pipeline.md` — replaced by this folder-based pipeline
