# ADR-026: Folder-Based Pipeline

## Status

Accepted

## Context

The previous file-based pipeline processed individual files through a 5-state machine (Scanned → Matched → NeedsReview → Confirmed → Moved). Each file was hashed (SHA-256), analyzed (ffprobe), scored (confidence), and tracked individually.

For DVD/Blu-ray ripping, users think in folders, not files. A disc produces one folder. This ADR defines the technical implementation of the folder-based pipeline.

Product behavior is defined in [PRODUCT.md](../PRODUCT.md).

## Decision

Use a **folder-based pipeline** with 6 states for the Organizer window.

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

| State | Meaning |
|-------|---------|
| `Scanned` | Folder found in scan directory, not yet analyzed |
| `Confident` | TMDb match found, year confirmed (score ≥ 0.9) |
| `Uncertain` | TMDb match found, year missing/ambiguous (score 0.7–0.9) |
| `Unknown` | No TMDb match (score < 0.7) |
| `Moved` | Folder moved to library destination (terminal) |
| `Quarantined` | Unknown folder moved to quarantine subfolder (terminal) |

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

### Folder Record (core/src/folder_record.rs)

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

### Confidence Scoring

Confidence is calculated per folder:

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

### API Endpoints (web/src/routes/organizer.rs)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/organizer/folders` | List all folders in scan directory |
| GET | `/api/organizer/folders/:id` | Single folder by ID |
| POST | `/api/organizer/scan` | Scan configured directory |
| POST | `/api/organizer/:id/search` | Manually search TMDb for folder |
| POST | `/api/organizer/:id/accept` | Accept → Moved |
| POST | `/api/organizer/:id/reject` | Reject → Quarantined |
| POST | `/api/organizer/accept-all` | Move all Confident folders |
| POST | `/api/organizer/reject-all` | Move all Unknown folders to quarantine |

### Database Schema (infra/src/migrations/005_create_folders_table.sql)

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
- Simpler pipeline — one record per folder, not per file
- No per-file hashing or probing — faster, less I/O
- Database is smaller
- UI is simpler — folder list, not file table

**Negative:**
- File-level details (duration, hash, content type) are lost
- No duplicate detection across folders
- Less granular control — users can't accept/reject individual files
