---
name: media-analysis-core
description: >
  Core logic for media file analysis.
  Covers filename cleaning, TV info extraction, FFprobe metadata extraction,
  and SHA-256 hashing strategies.
metadata:
  language: rust
  version: "1.0+"
  compatibility: regex, sha2, ffprobe, path
---

# Media Analysis Core

## General Principles
- **Pure Functions:** Filename cleaning and parsing should be pure functions (no I/O).
- **Performance:** Hashing should be parallelized for large batches.
- **Accuracy:** TV info extraction must handle various scene naming conventions.

## Filename Cleaning
- **Scene Tags:** Strip common tags (e.g., `BluRay`, `x264`, `AAC`, `1080p`, `WEBRip`).
- **Release Groups:** Remove release group markers (e.g., `[YTS]`, `[RARBG]`, `-GROUP`).
- **Normalization:**
  - Replace underscores/hyphens with spaces.
  - Collapse multiple spaces into one.
  - Trim whitespace.
- **Year Extraction:** Detect 4-digit years (1900-2099) in filenames.

## TV Info Extraction
- **Patterns:**
  - `SxxExx`: Standard TV format (e.g., `S01E02`).
  - `NxNN`: Alternative TV format (e.g., `1x02`).
  - `Season N`: Season pack (e.g., `Season 1`).
  - `Ep x-x`: Episode range (e.g., `Ep 1-5`).
- **Output:**
  - Series name (from folder or filename).
  - Season number.
  - Episode number.
  - Episode title (if available).
  - Disk number (if applicable).

## FFprobe Metadata
- **Extraction:** Use `ffprobe` to extract duration, codec, and resolution.
- **Parsing:** Parse JSON output efficiently.
- **Error Handling:** Handle missing or invalid metadata gracefully.
- **Caching:** Cache ffprobe results to avoid redundant calls.

## Hashing
- **Algorithm:** Use SHA-256 for file identification.
- **Strategy:**
  - For small files (< 8MB), hash the entire file.
  - For large files, hash head + tail chunks (4MB each) for speed.
- **Fallback:** If hashing fails, use path-based hash as fallback.

## Edge Cases
- **Extras:** Detect extras folders/files (e.g., `extras/`, `trailer.mkv`).
- **Duplicates:** Handle duplicate filenames in different folders.
- **Special Characters:** Handle non-ASCII characters in filenames.