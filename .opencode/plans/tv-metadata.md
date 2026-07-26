# Plan: TV Show Metadata Enrichment

## Problem

The analyzer doesn't populate TV-specific fields (`series_name`, `season_number`,
`episode_number`, `episode_title`) even when TMDb/OMDb returns TV results. Filename
parsing also doesn't detect TV patterns like `S01E05`.

## Phase 1: Filename Parsing — Detect TV Patterns

**File: `app/src/analyzer.rs`**

Enhance `parse_filename` to detect:
- `S01E05` → season=1, episode=5
- `S01E05E06` → multi-episode (take first)
- `Season 1 Episode 5` → season=1, episode=5
- `S01` → season=1, no episode (season pack)
- `1x05` → season=1, episode=5

When TV pattern detected:
- Set `media_type = MediaType::Tv`
- Populate `season_number`, `episode_number`
- Extract series name from preceding text

## Phase 2: Enhance MetadataMatch for TV Details

**File: `core/src/metadata.rs`**

Add fields to `MetadataMatch`:
- `series_name: Option<String>`
- `season_number: Option<u32>`
- `episode_number: Option<u32>`
- `episode_title: Option<String>`

## Phase 3: TMDb TV Details for Episodes

**File: `infra/src/http.rs`**

When a TV match is found and filename has season/episode numbers:
1. Use TMDb `/tv/{id}/season/{s}/episode/{e}` to get episode title
2. Populate `episode_title` in `MetadataMatch`

## Phase 4: Analyzer — Populate TV Fields

**File: `app/src/analyzer.rs`**

When metadata search returns `is_tv: true`:
- Set `series_name` from match title
- Set `season_number` / `episode_number` from filename parse
- Set `episode_title` from metadata match
- Set `media_type = MediaType::Tv`

## Phase 5: OMDb TV Search

**File: `infra/src/http.rs`**

Add `search_omdb_tv` using `type=series` parameter.
Update `search` to try OMDb for TV too (fallback).

## Verification

1. `cargo fmt && cargo clippy --all && cargo build --all && cargo test --all`
2. Test filename parsing with TV patterns
3. Test analyzer populates all TV fields
4. Test TMDb episode detail fetch with wiremock

## Files to Modify

| File | Changes |
|------|---------|
| `core/src/metadata.rs` | Add TV fields to MetadataMatch |
| `app/src/analyzer.rs` | TV filename parsing, populate TV fields |
| `infra/src/http.rs` | TMDb episode details, OMDb TV search |
