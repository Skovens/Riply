---
name: pipeline-architecture
description: >
  Core data pipeline for Riply. Covers PipelineStatus state machine,
  PipelineTransition validation, FileRecord fields and lifecycle,
  MediaType enum, MetadataSearcher trait, and the database layer
  that persists records through pipeline stages.
metadata:
  language: rust
  version: "1.0+"
  compatibility: serde, sqlx, chrono, riply_core
---

# Pipeline Architecture

## Overview

The pipeline is the core data flow through which media files progress from
raw scan to organized library. It is defined in `riply_core` and operated
by services in `app`, persisted in `infra` (SQLite via SQLx).

```
Scan -> Analyze -> Review -> Move -> Library
  |        |         |        |       |
Scanned  Matched   Needs     Confirmed Moved
           |       Review
           |         |
           +-> Failed <-+
```

## PipelineStatus State Machine

**Module:** `core/src/pipeline.rs`

```rust
pub enum PipelineStatus {
    Scanned,        // File scanned, not yet analyzed
    Matched,        // Successfully matched to a title
    NeedsReview,    // Confidence below threshold, needs human input
    Failed,         // No match found (recoverable)
    Confirmed,      // User confirmed the match
    Moved,          // File moved to library (terminal)
}
```

### State Properties

| Status | is_terminal() | requires_review() | is_failed() | is_successful() |
|--------|--------------|-------------------|-------------|-----------------|
| Scanned | false | false | false | false |
| Matched | false | false | false | true |
| NeedsReview | false | true | false | false |
| Failed | false | false | true | false |
| Confirmed | false | false | false | true |
| Moved | true | false | false | true |

### Valid Transitions

| Transition | From | To |
|-----------|------|-----|
| ScanToMatch | Scanned | Matched |
| ScanToReview | Scanned | NeedsReview |
| ScanToFail | Scanned | Failed |
| MatchToConfirm | Matched | Confirmed |
| ReviewToConfirm | NeedsReview | Confirmed |
| ReviewToFail | NeedsReview | Failed |
| FailToReview | Failed | NeedsReview |
| ConfirmToMove | Confirmed | Moved |
| MatchToReview | Matched | NeedsReview |

```rust
pub enum PipelineTransition {
    ScanToMatch,
    ScanToReview,
    ScanToFail,
    MatchToConfirm,
    ReviewToConfirm,
    ReviewToFail,
    FailToReview,
    ConfirmToMove,
    MatchToReview,
}
```

### Validation

```rust
// Check if a transition is valid from a given source state
transition.is_valid_from(&current_status);

// Get source and target states
transition.from_state();  // -> PipelineStatus
transition.to_state();    // -> PipelineStatus
```

## FileRecord

**Module:** `core/src/file_record.rs`

The central data type that carries all metadata through the pipeline.

```rust
pub struct FileRecord {
    // Identity
    pub id: String,              // SHA-256 hash prefix (first 16 hex chars)
    pub path: PathBuf,           // Full path on disk
    pub filename: String,        // File name without path
    pub hash: String,            // Full SHA-256 hash (64 hex chars)

    // Metadata
    pub size: Option<i64>,       // File size in bytes
    pub duration: Option<i64>,   // Duration in seconds (from ffprobe)
    pub content_type: String,    // "unknown", "main", "extra"

    // Pipeline state
    pub status: PipelineStatus,
    pub suggested_title: Option<String>,  // From metadata search
    pub suggested_year: Option<u32>,
    pub confidence: Option<f64>,          // 0.0 to 1.0
    pub canonical_title: Option<String>,  // Confirmed title
    pub canonical_year: Option<u32>,
    pub tmdb_id: Option<i32>,
    pub media_type: MediaType,
    pub series_name: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub episode_title: Option<String>,
    pub source: Option<String>,           // "tmdb", "omdb", "manual"
    pub subtitle_text: Option<String>,
    pub duplicate_path: Option<PathBuf>,
    pub reviewed: bool,

    // Timestamps
    pub created_at: String,  // RFC3339
    pub updated_at: String,  // RFC3339
}
```

### Construction

```rust
let record = FileRecord::new(
    id.to_string(),
    path.to_path_buf(),
    filename,
    hash,
);
// Sets: status=Scanned, media_type=Movie, content_type="unknown"
// size is None (populated by scanner)
```

### Status Update

```rust
let mut record = /* ... */;
record.update_status(PipelineStatus::Matched);
// Updates status and sets updated_at to current RFC3339 timestamp
```

### Helper Methods

```rust
record.is_tv_episode()    // media_type is TV AND episode_number is Some
record.is_extra()          // content_type == "extra"
record.needs_review()      // status.requires_review()
record.is_ready_to_move()  // status == Confirmed
```

## MediaType

**Module:** `core/src/media_type.rs`

```rust
pub enum MediaType {
    Movie,
    Tv,
}
```

**Default:** `Movie`

```rust
media_type.is_tv()   // true if MediaType::Tv
media_type.is_movie() // true if MediaType::Movie
```

Parsed from filename patterns (SxxExx, NxNN) during analysis.

## MetadataSearcher Trait

**Module:** `core/src/metadata.rs`

Trait for external metadata lookups. Implemented by `HttpClient` in `infra`.

```rust
pub trait MetadataSearcher: Send {
    fn search_movies(
        &self,
        query: &str,
        year: Option<u32>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, anyhow::Error>> + Send + '_>>;

    fn search_tv(
        &self,
        query: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<MetadataMatch>, anyhow::Error>> + Send + '_>>;
}
```

### Implementation (HttpClient in infra)

Uses TMDb v3 API (primary) with OMDb fallback. Implements rate limiting
(token bucket: 40 req/min for TMDb, 1000/day for OMDb) and local JSON
file caching (24h TTL).

### MetadataMatch

```rust
pub struct MetadataMatch {
    pub title: String,
    pub year: Option<u32>,
    pub tmdb_id: Option<i32>,
    pub is_tv: bool,
    pub series_name: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub episode_title: Option<String>,
}
```

## Database Layer

**Module:** `infra/src/db.rs`

SQLite database backed by SQLx. Handles persistence of FileRecords
through all pipeline stages.

### Connection

```rust
let db = Database::new(&config.database_path).await?;
db.migrate().await?;  // Run pending SQLx migrations
```

Uses `SqlitePoolOptions` with `create_if_missing(true)` and WAL journal mode.

### Key Methods

```rust
// Save a file record
db.save_file(&record).await?;

// Update a file record
db.update_file(&record).await?;

// Get by ID
db.get_file_by_id(&id).await?;  // -> Option<FileRecord>

// Find by hash (for duplicate detection)
db.find_file_by_hash(&hash).await?;  // -> Option<FileRecord>

// Query by status
db.get_files_by_status("Scanned").await?;  // -> Vec<FileRecord>

// Count by status
db.count_files_by_status("Confirmed").await?;  // -> i64
db.count_files_by_status_all().await?;  // -> HashMap<String, i64>

// Delete
db.delete_file(&id).await?;

// Pagination
db.get_files_paginated(Some("NeedsReview"), Some("search"), 1, 50)
    .await?;  // -> (Vec<FileRecord>, total: i64)
```

## Pipeline Usage in Services

### ScannerService (`app::ScannerService`)

Produces `FileRecord` objects with `status=Scanned`:

```rust
let scanner = ScannerService::new();
let records = scanner.scan_directory(&path).await?;
// Each record has: id, path, filename, hash, size, status=Scanned
```

### AnalyzerService (`app::AnalyzerService`)

Consumes `Scanned` records, produces `Matched`/`NeedsReview`/`Failed`:

```rust
let analyzer = AnalyzerService::new(searcher);
let updated = analyzer.analyze_file(&record).await?;
// Sets: suggested_title, suggested_year, confidence, media_type,
//        series_name, season_number, episode_number, source
```

**Confidence scoring:**
- Title > 3 chars: +0.5
- Year present: +0.3
- Extension present: +0.2
- TV episode from filename: minimum 0.85
- External metadata match: 0.95

**Status assignment:**
- confidence >= 0.8 -> `Matched`
- confidence < 0.8 -> `NeedsReview`

### MoverService (`app::MoverService`)

Consumes `Confirmed` records, produces `Moved`:

```rust
let mover = MoverService::new();
let updated = mover.move_file(&record, &movies_dir, &tv_dir).await?;
// Creates destination dirs, moves file, sets status=Moved
```

**Destination structure:**
- Movies: `movies_dir/Title (Year)/filename`
- TV: `tv_dir/Series Name/Season XX/filename`

## Pipeline in Web API

### File Update (`web/src/routes/files.rs`)

The review UI updates `FileRecord` fields directly:

```rust
pub struct UpdateFileBody {
    pub canonical_title: Option<String>,
    pub canonical_year: Option<u32>,
    pub series_name: Option<String>,
    pub season_number: Option<u32>,
    pub episode_number: Option<u32>,
    pub media_type: Option<String>,
}
```

Accepts `"tv"`, `"Tv"`, `"TV"` -> `MediaType::Tv`, anything else -> `MediaType::Movie`.

After updating, the record's `updated_at` is refreshed.

### Review Actions (`web/src/routes/review.rs`)

```rust
// Accept: Matched or NeedsReview -> Confirmed
file.update_status(PipelineStatus::Confirmed);

// Reject: NeedsReview -> Failed
file.update_status(PipelineStatus::Failed);
```

### Job Workflows

**Scan job:** Creates records with `Scanned` status, detects duplicates by hash.

**Analyze job:** Reads `Scanned` records, runs `AnalyzerService`, updates to
`Matched`/`NeedsReview`/`Failed`.

**Retry-failed:** Resets `Failed` -> `Scanned`, then runs analyze.

**Move job:** Reads `Confirmed` records, runs `MoverService`, updates to `Moved`.

## Database Schema

### files table

| Column | Type | Notes |
|--------|------|-------|
| id | TEXT PRIMARY KEY | SHA-256 hash prefix |
| path | TEXT | Full file path |
| filename | TEXT | File name |
| hash | TEXT | Full SHA-256 hash |
| size | INTEGER | File size in bytes |
| duration | INTEGER | Duration in seconds |
| status | TEXT | PipelineStatus string |
| suggested_title | TEXT | |
| suggested_year | INTEGER | |
| confidence | REAL | 0.0 to 1.0 |
| canonical_title | TEXT | |
| canonical_year | INTEGER | |
| tmdb_id | INTEGER | |
| content_type | TEXT | "unknown", "main", "extra" |
| media_type | TEXT | "Movie" or "Tv" |
| series_name | TEXT | |
| season_number | INTEGER | |
| episode_number | INTEGER | |
| episode_title | TEXT | |
| source | TEXT | "tmdb", "omdb", "manual" |
| subtitle_text | TEXT | |
| duplicate_path | TEXT | |
| reviewed | INTEGER | 0 or 1 |
| created_at | TEXT | RFC3339 |
| updated_at | TEXT | RFC3339 |

## Testing Patterns

### PipelineStatus Tests

```rust
assert!(!PipelineStatus::Scanned.is_terminal());
assert!(PipelineStatus::Moved.is_terminal());

assert!(PipelineStatus::NeedsReview.requires_review());
assert!(!PipelineStatus::Matched.requires_review());

assert!(PipelineTransition::ScanToMatch.is_valid_from(&PipelineStatus::Scanned));
assert!(!PipelineTransition::ScanToMatch.is_valid_from(&PipelineStatus::Matched));
```

### FileRecord Tests

```rust
let mut record = FileRecord::new("id".to_string(), path, "file.mkv".to_string(), "hash".to_string());
assert_eq!(record.status, PipelineStatus::Scanned);
assert_eq!(record.media_type, MediaType::Movie);

record.update_status(PipelineStatus::Matched);
assert!(record.updated_at > original_updated);
```

### Database Tests

Use `tempfile::tempdir()` for temporary database paths:

```rust
let dir = tempdir().unwrap();
let db_path = dir.path().join("test.db");
let db = Database::new(&db_path).await?;
db.migrate().await?;
```

## Common Pitfalls

1. **Status is a String in DB** — The database stores `PipelineStatus` as a
   string. Use exact string matching: `"Scanned"`, `"Matched"`, `"Needs Review"`
   (note the space), `"Confirmed"`, `"Moved"`, `"Failed"`.

2. **NeedsReview has a space** — `PipelineStatus::NeedsReview` displays as
   `"Needs Review"` (with a space). Queries must use `"Needs Review"`, not
   `"NeedsReview"`.

3. **MediaType default is Movie** — New FileRecords default to `MediaType::Movie`.
   Set it to `Tv` before analysis if you know it's a TV show, or let the
   analyzer detect it from filename patterns.

4. **confidence is Option<f64>** — Unanalyzed records have `confidence = None`.
   Always check for `Some()` before comparing.

5. **update_status updates timestamp** — `FileRecord::update_status()` sets
   `updated_at` to the current time. Use this to detect when records change.

6. **Duplicate detection is by hash** — The scanner detects duplicates by
   comparing SHA-256 hashes. The first occurrence is saved; subsequent ones
   get `duplicate_path` set.

7. **is_ready_to_move checks Confirmed only** — `FileRecord::is_ready_to_move()`
   returns true only when `status == Confirmed`. Matched files must be
   confirmed before moving.

8. **Content type default is "unknown"** — New records have `content_type = "unknown"`.
   Set to `"main"` or `"extra"` during analysis if the content type is known.
