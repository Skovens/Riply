# ADR-028: Movie vs TV Show Detection

## Status

Proposed

## Context

When ripping a DVD, the system must determine whether the content is a movie or a TV show. This affects the output structure:

- **Movie**: `{Title} ({Year})/` with main file `{Title}.{ext}`, extras in `Extras/`
- **TV Show**: `{Show} ({Year})/Season {NN}/` with files `{Show} - S{NN}E{NN}.{ext}`, extras in `Season {NN}/Extras/`

The detection must be accurate enough to provide a good default, but the user must be able to override it. Some titles exist as both movies and TV shows (e.g., "Doctor Who", "Star Trek", "The Twilight Zone").

Options considered:
1. **TMDb-only detection** — Search TMDb, check if result is movie or tv_show. Accurate but requires TMDb match.
2. **File-size heuristic** — Analyze ripped file sizes to detect patterns. Fast but fragile.
3. **Hybrid** — Use TMDb when available, fall back to file-size heuristic, user confirms either way.

## Decision

Use a **hybrid detection** approach with user confirmation.

### Detection Priority

1. **TMDb match** (if available) — Most accurate
   - If TMDb returns a movie → `media_type = Movie`
   - If TMDb returns a tv_show → `media_type = Tv`
   - If TMDb returns multiple results (movie + tv_show) → `media_type = Unknown`, user must choose

2. **File-size heuristic** (if TMDb fails) — Fallback
   - **Movie pattern**: 1 large file (>3x the median of other files) + small extras
   - **TV pattern**: Multiple similarly-sized files (std dev < 30% of mean) + occasional small extras
   - **Ambiguous**: Doesn't match either pattern → `media_type = Unknown`

3. **User confirmation** (always required) — Final authority
   - Detection result is a suggestion, not a decision
   - User can flip Movie ↔ TV Show with one click
   - If TV Show, user must provide show name and season number

### File-Size Heuristic Details

```rust
fn detect_media_type(files: &[FileInfo]) -> MediaType {
    if files.len() < 2 {
        return MediaType::Movie; // Single file is likely a movie
    }

    let sizes: Vec<u64> = files.iter().map(|f| f.size).collect();
    let max_size = sizes.iter().max().unwrap();
    let median_size = median(&sizes);
    let mean_size = mean(&sizes);
    let std_dev = standard_deviation(&sizes);

    // Movie pattern: one file much larger than the rest
    if *max_size > median_size * 3 {
        return MediaType::Movie;
    }

    // TV pattern: multiple similarly-sized files
    if mean_size > 0 && std_dev / mean_size < 0.3 {
        return MediaType::Tv;
    }

    // Ambiguous
    MediaType::Unknown
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted[sorted.len() / 2]
}

fn mean(values: &[u64]) -> u64 {
    values.iter().sum::<u64>() / values.len() as u64
}

fn standard_deviation(values: &[u64]) -> f64 {
    let m = mean(values) as f64;
    let variance = values.iter().map(|v| (v as f64 - m).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}
```

### Edge Cases

| Scenario | Expected | Detection |
|----------|----------|-----------|
| Single movie DVD | Movie | ✅ Movie (1 large file) |
| Season DVD with 6 episodes | TV | ✅ TV (multiple similar files) |
| Movie with commentary track | Movie | ⚠️ Could be TV if commentary is long |
| Documentary with one 2hr feature + 1hr extras | Movie | ✅ Movie (1 large file) |
| "Best of" compilation DVD | Movie | ⚠️ Could be TV if multiple big files |
| TV special (90min, single file) | TV | ⚠️ Detected as Movie (1 file) |
| Anime DVD (3 episodes, 24min each) | TV | ✅ TV (multiple similar files) |

### UI Flow

**During rip:**
```
Disc: "Inception"
→ TMDb: "Inception (2010)" → Movie
→ Drive card: "Inception (2010) 🎬"
→ Rip completes → Move to "Inception (2010)/"
```

**During rip (TV show):**
```
Disc: "Breaking Bad S01"
→ TMDb: "Breaking Bad" → TV Show
→ Drive card: "Breaking Bad (2008) 📺 Season 1"
→ User confirms season number
→ Rip completes → Move to "Breaking Bad (2008)/Season 1/"
```

**During rip (ambiguous):**
```
Disc: "Doctor Who"
→ TMDb: "Doctor Who" → Movie + TV Show
→ Drive card: "Doctor Who — select type"
→ User clicks 🎬 or 📺
→ Rip completes → Move based on selection
```

**During rip (no TMDb match):**
```
Disc: "Unknown Disc 123"
→ TMDb: No match
→ Heuristic: 1 large file → Movie
→ Drive card: "Unknown Disc 123 🎬 (detected)"
→ User can flip to 📺 if wrong
→ Rip completes → Move based on selection
```

### API Endpoints

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/ripper/:index/detect` | `detect_media_type` | Detect movie/TV from disc |
| POST | `/v1/ripper/:index/set-type` | `set_media_type` | User sets media type |

### Consequences

**Positive:**
- TMDb detection is accurate when available
- File-size heuristic provides a fallback
- User always has final say
- One-click flip between Movie and TV Show
- Works with or without TMDb

**Negative:**
- Heuristic is fragile — edge cases exist
- Single-file TV specials (e.g., "Saturday Night Live") may be misdetected as movies
- Commentary tracks may confuse the heuristic

**Mitigation:**
- Heuristic is always a suggestion — user confirms
- Drive card shows "detected" badge when heuristic is used (not TMDb)
- Organizer window can re-detect if needed

## Supersedes

- None (new ADR)
