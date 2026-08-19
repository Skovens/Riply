# ADR-025: Three-Window Architecture

## Status

Proposed

## Context

Riply's current architecture has a single dashboard with a file-based pipeline (Scanned → Matched → NeedsReview → Confirmed → Moved), a stats bar, a review queue, and tightly coupled rip/naming flows. This design has several problems:

1. **Too many concerns in one view** — Drive management, file scanning, metadata review, and job tracking are all crammed into a single dashboard.
2. **File-first pipeline is over-engineered** — SHA-256 hashing, ffprobe analysis per file, confidence scoring, 5-state machine — users think in folders, not files.
3. **Dashboard clutter** — Five top stats bars (Scanned, Matched, NeedsReview, Confirmed, Moved), pipeline visualization, review queue sidebar, jobs sidebar, and files table create cognitive overload.
4. **Rip and naming are coupled** — Despite ADR-024's attempt to separate them, the naming modal is still embedded in the rip flow.
5. **Buttons disappear** — Controls hide/show based on state, making the UI unpredictable and hard to learn.

Options considered:
1. **Single dashboard with tabs** — Keep one window, use tabs for sections
2. **Three separate windows** — Rip, Organizer, Settings as distinct views
3. **Split view** — Two panels (Rip + Organizer) with Settings as a modal

## Decision

Use **three separate windows** with distinct purposes:

### Window 1: Rip

Handles disc ripping. One window, one job.

**Layout:**
- Drive cards (one per drive)
- Each card shows:
  - Drive name + model
  - Disc name (if present)
  - **Cover image** — placeholder when unidentified, metadata cover when identified. Fixed size to prevent layout shift.
  - Rip progress bar (during rip)
  - Status indicator
  - **Action buttons** (never disappear — greyed out with tooltip when inactive)

**Drive card buttons:**

| Button | Active | Inactive (greyed out) |
|--------|--------|----------------------|
| Rip | Disc inserted, not ripping | Disc ejected → "No disc in drive" |
| Eject | Disc inserted, not ripping | No disc → "No disc to eject" |
| View in Organizer | Unidentified rip | Identified rip → "Already named" |

**Drive card states:**

| State | Display |
|-------|---------|
| `Ready` | "Insert disc to rip" + Rip button active |
| `Ripping` | Progress bar + "Ripping..." + ETA |
| `Complete` | "Ripping successful" + disc cover + auto-detected name + type |
| `Error` | "Ripping failed: [reason]" + descriptive error (e.g., "Clean the disc", "Disc unreadable") |

**Flow:**
1. User clicks "Rip" (or autorip triggers)
2. Rip starts — MakeMKV writes to tmp folder
3. **Concurrently**, autodetection runs independently (uses disc name, available at rip start):
   - Parse disc name → search TMDb for title + year
   - If match found → show result live: `"Disc: Inception → Found: Inception (2010)"`
   - If no match → show: `"Disc: Inception → Not found"`
4. Rip completes — files moved to final destination based on autodetection:
   - **Movie**: biggest file → `{Title} ({Year})/`, rest → `{Title} ({Year})/Extras/`
   - **TV show**: user confirms show name + season → `{Show} ({Year})/Season {NN}/`, files → `{Show} - S{NN}E{NN}.{ext}`, extras → `Season {NN}/Extras/`
   - **Unidentified**: files stay in tmp with disc name, card shows "Unidentified — view in Organizer"
5. Drive card ends in terminal state: `Complete`, `Error`, or returns to `Ready` if unidentified

**Autorip:**
- Enabled → auto-starts rip when disc inserted
- Clears previous state, starts on new disc
- Event-driven (no constant polling)

### Window 2: Organizer

Handles folder-level media naming and moving.

**Layout:**
- Scan directory selector + "Scan" button
- Folder list (one row per folder):
  | Current name | Suggested name | Confidence | Type | Action |
- Expandable folder view (click folder name):
  - Shows files inside
  - Shows biggest file (main title)
  - Shows files going to "Extras"
- **Review button** — reviews uncertain/unknown folders, skips confident ones
- **Accept All button** — moves all confident folders, skips uncertain/unknown
- **Reject All button** — moves unknown folders to "Quarantine"

**Naming rules:**
- Movies: `{Title} ({Year})/` with main file `{Title}.{ext}`
- TV Shows: `{Show} ({Year})/Season {NN}/` with files `{Show} - S{NN}E{NN}.{ext}`
- Extras: `Extras/` subfolder
- Uncertain: not moved, stays in scan directory

**Confidence scoring:**
| Score | Meaning | Action |
|-------|---------|--------|
| ✅ Confident (≥0.9) | TMDb match, year confirmed | Auto-accept, move on "Accept All" |
| ⚠️ Uncertain (0.7–0.9) | TMDb match, year missing/ambiguous | Manual review required |
| ❌ Unknown (<0.7) | No TMDb match | Manual search or reject |

### Window 3: Settings

All configuration that doesn't belong in Rip or Organizer.

| Category | Settings |
|----------|----------|
| Ripping | Tmp folder path, MakeMKV command, autorip toggle per drive |
| Library | Movie destination folder, TV show destination folder, tmp cleanup age (days) |
| Metadata | TMDb API key, OMDB API key, search language |
| Advanced | Log level, debug options |

### UI Principles

**Buttons never disappear.** If a button has no function in the current state, it is greyed out with a tooltip explaining why:

```css
button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
button:disabled:hover::after {
  content: attr(data-tooltip);
  /* show tooltip */
}
```

This makes the UI predictable — users always see what controls are available, even if inactive.

### Consequences

**Positive:**
- Each window has a single, clear purpose
- No dashboard clutter — no stats bars, no review queue, no pipeline visualization
- Folder-first approach matches how users think about media
- Autodetection runs concurrently with rip — independent, non-blocking
- Autorip is event-driven, not polling-based
- Buttons are always visible — predictable, learnable UI
- Tmp folder is ephemeral — clean by default, no cleanup help

**Negative:**
- File-level analysis (SHA-256, ffprobe, confidence per file) is removed — this is acceptable for DVD ripping where folder-level metadata is sufficient
- Users must manually switch between windows — acceptable since windows have distinct purposes
- Drive cards have fixed button set — slightly more screen real estate, but more predictable

### What Gets Deprecated

| Old | Replaced By |
|-----|-------------|
| Dashboard stats bar (Scanned, Matched, etc.) | Gone |
| File-based pipeline (SHA-256, ffprobe, 5 states) | Folder-based pipeline |
| Review queue | Organizer window |
| Move preview/confirm | Accept All with confirmation popup |
| DetailDrawer | Gone — folder-level UI only |
| TitleGroup per folder | Simplified — folder name + suggested name |
| Confidence scoring per file | Confidence scoring per folder |

### What Stays

| Existing | Why |
|----------|-----|
| MakeMKV backend abstraction | Core ripping functionality |
| TMDb/OMDb search | Metadata lookup |
| SSE for real-time updates | Rip progress, drive state |
| Job scheduler | Long-running operations |
| SQLite database | Config, drive state, rip history |
| Jellyfin naming convention | Output format |

## Supersedes

- ADR-020 (Rip Progress Bar) — replaced by drive card progress
- ADR-021 (Collapsible Folder Preview) — replaced by Organizer expandable folders
- ADR-022 (Single Media-Naming Service) — replaced by Organizer window
- ADR-024 (Separate Rip and Naming Flows) — replaced by Rip + Organizer windows
