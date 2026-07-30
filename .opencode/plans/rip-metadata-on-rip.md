# Rip Metadata on Rip + Folder-Based Scanning

## Problem

Currently, the rip step is disconnected from the metadata system:
- Disc name is parsed but the result is used only for a subdirectory name (`staging/Inception/`)
- The year extracted by `parse_disc_name` is discarded
- After rip, files have generic MakeMKV filenames (`title_t00.mkv`) that yield no meaningful metadata matches
- The analyze step must re-derive everything from garbage filenames → wasted API calls + low confidence
- Files from one rip are treated as independent `FileRecord`s with no grouping

## Goal

1. **At rip time**: clean disc name → search TMDb → use matched title for folder naming + organization
2. **Folder-based scanning**: scan one directory name, not thousands of files
3. **1 title per data directory**: group all files from a rip under one pre-matched title
4. **Per-provider rate limiting**: reusable trait for any metadata API
5. **Jellyfin-compatible output**: extras folders, season structure, proper naming

---

## Architecture

### Two independent flows, one shared pipeline

```
                         ┌──────────────────────┐
                         │    NameCleaner        │
                         │  (core: pure fn)      │
                         │  normalize separators │
                         │  extract year         │
                         │  title-case           │
                         │  strip noise prefixes │
                         └──────────┬───────────┘
                                    ▼ cleaned title + year
                         ┌──────────────────────┐
                         │   MetadataSearcher    │
                         │  (app: existing trait)│
                         │  search TMDb/OMDb     │
                         ├──────────────────────┤
                         │   RateLimiter trait   │
                         │  per-provider token   │
                         │  bucket (governor)    │
                         └──────────┬───────────┘
                                    ▼ MatchedTitle, Year, TMDB_ID, MediaType
                         ┌──────────────────────────────────────┐
                         │           TitleGroup                 │
                         │  1 title per source directory        │
                         │  many FileRecords underneath         │
                         └──────────────────────────────────────┘
```

**Flow A — Rip (disc → staging):**
```
Disc name → NameCleaner → MetadataSearcher (at rip time)
  → MakeMKV info → title size distribution
  → Heuristic: movie vs TV
  → Create staging/{MatchedTitle} ({Year})/
    ├── Movie: main.mkv + extras/*.mkv
    └── TV:    Season 01/*.mkv + extras/*.mkv
  → Create TitleGroup + FileRecords pre-filled
```

**Flow B — Analyze (point at existing directory):**
```
List immediate subdirectories
  ├── Found subdirs? → NameCleaner + TMDb per subdir
  │   → 1 TitleGroup per subdir, files inherit
  └── No subdirs? → per-file fallback (current behavior)
```

---

## New Components

### `core/src/name_cleaner.rs`

Pure function(s) for disc name → searchable title:

```rust
pub struct CleanedName {
    pub title: String,
    pub year: Option<u16>,
    pub raw: String,
}

/// Normalize separators, extract year, title-case.
pub fn clean_name(raw: &str) -> CleanedName;

/// Optionally strip noise prefixes by trying TMDb search
/// with and without the first word; returns best result.
pub fn clean_with_search<S: MetadataSearcher>(
    raw: &str,
    searcher: &S,
    limiter: &dyn RateLimiter,
) -> Result<Option<CleanedName>>;
```

Crate additions: `Inflector` (title case), `unidecode` (Unicode → ASCII).

### `core/src/rate_limiter.rs`

```rust
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Wait until a request is permitted.
    async fn acquire(&self);
}
```

Implementation uses `governor` crate (token bucket). Each provider gets its own limiter instance with appropriate quotas:
- TMDb: `Quota::per_second(nonzero!(50u32))`
- OMDb: `Quota::per_day(nonzero!(1000u32))`

### `core/src/title_group.rs`

```rust
pub enum TitleGroupStatus { New, Matched, Confirmed, Moved }

pub struct TitleGroup {
    pub id: String,                    // hash of source_directory
    pub source_directory: PathBuf,     // e.g., staging/Inception (2010)/
    pub title: String,                 // matched or cleaned disc name
    pub year: Option<u16>,
    pub tmdb_id: Option<u32>,
    pub media_type: MediaType,         // Movie or Tv
    pub series_name: Option<String>,
    pub status: TitleGroupStatus,
    pub file_records: Vec<FileRecord>,
}
```

### `app/src/rip_planner.rs`

New service that runs before the actual rip:

```rust
pub struct RipPlanner { /* MetadataSearcher, RateLimiter, Backend */ }

impl RipPlanner {
    /// Determine movie vs TV from title size distribution.
    /// - Movie: 1 title ≥5× larger than the rest
    /// - TV: multiple titles within ~2× of each other
    pub fn classify(titles: &[TitleInfo]) -> MediaType;
    
    /// Plan the folder layout before ripping.
    pub fn plan(cleaned: &CleanedName, media_type: MediaType) -> RipLayout;
}
```

### `infra/src/makemkv/info.rs`

New module for pre-rip disc scanning:

```rust
pub struct TitleInfo { pub index: usize, pub size_bytes: u64 }

/// Run `makemkvcon info disc:N` and enumerate title sizes.
pub fn scan_disc_titles(drive_index: usize) -> Result<Vec<TitleInfo>>;
```

---

## Modified Components

### `infra/src/makemkv/parser.rs`

Refactor `parse_disc_name` to delegate to `core::name_cleaner::clean_name()`. Keep the old function as a thin wrapper for backward compat.

### `infra/src/db.rs`

Add optional `title_group_id: Option<String>` column to `file_records` table (not a FK constraint — allows loose coupling). Store `TitleGroup` metadata inline on each `FileRecord`:

```rust
pub struct FileRecord {
    // ... existing fields ...
    pub title_group_id: Option<String>,
}
```

No new table in Phase 1. Phase 2 can add a `title_groups` table if query patterns demand it.

### `app/src/analyzer.rs`

- When `analyze_file()` encounters a `FileRecord` with pre-filled metadata from a `TitleGroup`, skip the metadata search step (it's already done at rip time).
- Still extract ffprobe metadata (duration, resolution) and validate against the pre-filled title.

### `app/src/scanner.rs`

New scanning strategy:
1. Walk directory → list subdirectories
2. For each subdir → `NameCleaner` + `MetadataSearcher` → create `TitleGroup`
3. If no subdirs → fall back to per-file scanning (current behavior)

### `web/src/routes/ripper.rs`

Update `start_rip` to:
1. Get disc name from `detect_drives()`
2. Call `NameCleaner` + `MetadataSearcher` before ripping
3. Determine `MediaType` (default Movie unless TV hint)
4. Create output folder with **matched** name: `staging/Inception (2010)/`
5. Rip into folder
6. Create `TitleGroup` + pre-filled `FileRecord`s

### `web/src/routes/files.rs` / `+page.svelte`

Show files grouped by `TitleGroup` instead of flat list. Display "1 title" per group.

---

## Jellyfin Output Layout

### Movies

```
staging/Inception (2010)/
  ├── Inception (2010).mkv          ← biggest title = main feature
  ├── extras/
  │   ├── featurette.mkv
  │   └── behind_the_scenes.mkv
  └── trailers/
```

### TV Shows

```
staging/Breaking Bad (2008)/
  ├── Season 01/
  │   ├── Breaking Bad S01E01.mkv    ← disc order, user remaps in review
  │   ├── Breaking Bad S01E02.mkv
  │   └── ...
  ├── extras/
  └── trailers/
```

TV show detection: search TMDb with cleaned disc name. If result is a TV show → TV layout. User maps episode numbers in review UI (select season, then map files to episode slots: "these 4 files = E01-E04").

---

## Rate Limiting

Per-provider token bucket using `governor`:

```rust
// infra/src/http.rs
pub struct HttpClient {
    client: reqwest::Client,
    tmdb_limiter: Arc<dyn RateLimiter>,
    omdb_limiter: Arc<dyn RateLimiter>,
}
```

Calls `limiter.acquire().await` before every outbound request. Providers don't know about each other's limits.

---

## Movie vs TV Heuristic

Pre-rip via `makemkvcon info disc:N`:

| Pattern | Classification | Logic |
|---------|---------------|-------|
| 1 title ≥5× larger than others | Movie | Main feature + extras |
| 3-6 titles within ~2× of each other | TV | Episodes |
| 1 title only | Movie | — |
| Mixed / ambiguous | User chooses (default Movie) | — |

The heuristic only runs if TMDb search itself didn't already determine the type (TMDb returns `media_type` for each result). If TMDb says it's a TV show, skip the heuristic.

---

## Phase Plan

### Phase 1 — Rip-time metadata (minimal viable)

| Component | Change |
|-----------|--------|
| `Cargo.toml` (core) | Add `Inflector`, `unidecode` |
| `core/src/name_cleaner.rs` | **New** — `clean_name()`, reuse in `parse_disc_name` |
| `core/src/lib.rs` | Export `pub mod name_cleaner` |
| `infra/src/makemkv/parser.rs` | Refactor `parse_disc_name` → delegates to `core::name_cleaner` |
| `app/src/analyzer.rs` | Update `analyze_file()`: detect disc-generated filenames (`title_t00.mkv`), look at parent folder name instead |
| `web/src/routes/ripper.rs` | `start_rip`: call MetadataSearcher after parsing disc name, use matched title for folder |

**Phase 1 delivers:** rip creates `staging/Inception (2010)/` instead of `staging/INCEPTION_2010/`.

### Phase 2 — TitleGroup + folder scanning

| Component | Change |
|-----------|--------|
| `core/src/title_group.rs` | **New** — `TitleGroup` struct |
| `infra/src/db.rs` | Add `title_group_id` column to file_records |
| `app/src/scanner.rs` | Folder-first scanning strategy |
| `app/src/analyzer.rs` | Skip search for pre-filled records |
| `app/src/mover.rs` | Optionally skip move if already in correct location |
| `web/src/routes/files.rs` | Group files by title_group_id |

**Phase 2 delivers:** 1 title per data directory, folder-based scanning works for both rip and "point at dir" modes.

### Phase 3 — Rate limiter + TV handling

| Component | Change |
|-----------|--------|
| `core/src/rate_limiter.rs` | **New** — `RateLimiter` trait |
| `Cargo.toml` (infra) | Add `governor` |
| `infra/src/http.rs` | Inject rate limiters into TMDb/OMDb clients |
| `infra/src/makemkv/info.rs` | **New** — pre-rip title size enumeration |
| `app/src/rip_planner.rs` | **New** — movie/TV heuristic |
| `web/src/routes/review.rs` | UI for mapping files → episodes ("these 4 = E01-E04") |
| `web/src/routes/ripper.rs` | TV output layout (Season 01/ + extras/) |
| All metadata searchers | Call `limiter.acquire()` before HTTP requests |

**Phase 3 delivers:** TV show detection, Jellyfin extras layout, rate-limited metadata calls.

---

## Dependencies

| Crate | Phase | Version | Purpose |
|-------|-------|---------|---------|
| `Inflector` | 1 | 0.11 | Title case conversion for disc names |
| `unidecode` | 1 | 0.3 | Unicode → ASCII for search queries |
| `strsim` | 1 | 0.11 | String similarity for confidence scoring |
| `governor` | 3 | 0.8 | Token bucket rate limiting |
