# Task 11: ffprobe Metadata Extraction

## Status: DONE

## Problem

`FileRecord` has `duration`, `content_type` fields that are never
populated. No media metadata (duration, resolution, codec) is extracted
during scanning.

## Fix

### New module: `infra/src/ffprobe.rs`

Wraps the `ffprobe` CLI to extract media metadata:

```rust
pub struct FfprobeMetadata {
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codec: Option<String>,
    pub content_type: Option<String>,  // "main" or "extra" based on duration
}

pub fn extract_metadata(path: &Path) -> Result<FfprobeMetadata> {
    // Run: ffprobe -v quiet -print_format json -show_format -show_streams <path>
    // Parse JSON output
}
```

### Content type classification

- Duration > 60 min → `"main"` (likely the feature)
- Duration 10-60 min → `"extra"` (behind the scenes, documentaries)
- Duration < 10 min → `"extra"` (trailers, clips)
- Unknown → `"unknown"`

### Integration

Option A: Run ffprobe during scanning (in `ScannerService::scan_directory`)
- Pro: metadata available immediately
- Con: slows down scanning for large libraries

Option B: Run ffprobe during analysis (in `AnalyzerService::analyze_file`)
- Pro: analysis already does metadata work, natural place
- Con: requires ffprobe binary, may not be available

**Recommendation: Option B** — analysis is already slow (network calls),
adding ffprobe there is fine. Skip gracefully if ffprobe not found.

### Dependencies

- `ffprobe` binary must be installed (part of ffmpeg)
- No new Rust crate needed — just `std::process::Command` + JSON parsing
- `serde_json` already available

## Files

| File | Change |
|------|--------|
| `infra/src/ffprobe.rs` | **NEW** — ffprobe wrapper module |
| `infra/src/lib.rs` | Add `pub mod ffprobe` |
| `app/src/analyzer.rs` | Call ffprobe in `analyze_file()`, populate duration/content_type |
| `infra/Cargo.toml` | No new deps needed |

## Tests

- Unit test with mock ffprobe output (sample JSON)
- Integration test with a real video file if available
- Test graceful handling when ffprobe not installed

## Verification

- `cargo test --all`
- Analyze a file — should populate duration and content_type
