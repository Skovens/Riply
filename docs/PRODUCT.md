# Riply — Product Specification

Riply is a Swiss Army knife for ripping and organizing physical media (DVDs/Blu-rays) into
Jellyfin-compatible library structure. It solves one problem: ripping and organizing discs
is tedious. MakeMKV handles the ripping but leaves you with raw disc names, manual naming,
and no folder restructuring. Riply automates the boring parts and gets you to a clean library
with minimal clicks.

> **Rebuild of [MediaAtlas](https://github.com/Skovens/MediaAtlas)** — same goal, cleaner
> code, fewer moving parts. No special users, no Docker, no PostgreSQL.

---

## Two Flows

Riply has two distinct workflows, each with its own web UI view. They are separate — you
use one or the other, not both together.

### Flow 1: Rip + Name (live, disc-by-disc)

```
Insert disc → auto-detect name → TMDb lookup → show thumbnail + name
    ↓
Click Rip → rip to temp folder
    ↓
During rip: TMDb lookup runs in background
    ↓
On completion:
  If named → move to library with Jellyfin naming
  If not named → keep in temp, show in Organizer for later
```

**What the user sees:** Drive cards with thumbnails, progress bars, auto-detected names.
Click rip, walk away, come back to organized folders.

### Flow 2: Organize (batch, folder-by-folder)

```
Point at a folder (temp dir, existing library, whatever)
    ↓
Scan → show each folder with: current name, suggested name, confidence
    ↓
Preview changes (before moving anything)
    ↓
Accept All (or per-folder) → move/rename to library
```

**What the user sees:** A table of folders. Each row shows: current name, what it should be
named, whether it needs moving, and an accept/reject button. Already-named folders show
"OK" and skip the rename (but still move if source ≠ destination).

---

## What "Done" Looks Like

A folder on disk with Jellyfin-compliant naming that any media server can scan:

```
/media/movies/
  Speed (1994)/
    Speed (1994).mkv
    Extras/
      Behind the Scenes.mkv

/media/tvshows/
  Klovn (2005)/
    Season 1/
      Klovn - S01E01.mkv
      Klovn - S01E02.mkv
      Extras/
        Commentary.mkv
```

That's the end state. The program gets you there.

---

## Data Flow

```
disc → MakeMKV → temp/{index}/ → TMDb lookup → library/{jellyfin}/
                              ↘ Organizer staging ↗
```

Three staging areas:

| Area | Path | Lifespan | Purpose |
|------|------|----------|---------|
| **Temp** | `/tmp/riply/0/` | Short-lived | Raw rip output, cleaned up by age |
| **Staging** (organizer_dir) | configurable | Until reviewed | Post-rip, pre-review, name may be raw or auto-detected |
| **Library** | `movies_dir` / `tv_dir` | Permanent | Final Jellyfin-compliant structure |

Network paths are supported for all three (any path with read/write permissions).

---

## Auto-Naming Rules

### Detection pipeline

1. **Parse disc name** — MakeMKV provides a raw name like `KLOVN_SEASON_1_DISK_1`
2. **Search TMDb** — movies first, then TV if no movie match
3. **Session memory** — if the last ripped disc was "Klovn S01", the next disc labeled
   "Klovn" suggests "Klovn S01E02" (inferred from the previous rip)
4. **User confirmation** — always required. The UI shows the suggested name with a
   thumbnail. The user can accept, search for a different match, or reject.

### What gets shown in the UI

| Condition | Display |
|-----------|---------|
| TMDb found | Thumbnail cover + title + year |
| TMDb not found | Gray placeholder + "media title not found" |
| Session hint available | Suggested name pre-filled, user can accept or change |

### What "correctly named" means

**Movies:**
- Parent folder: `{Title} ({Year})/`
- Main file (biggest file on disc): `{Title}.{ext}`
- Everything else: `{Title} ({Year})/Extras/` (with whatever MakeMKV named them)

**TV Shows:**
- Parent folder: `{Show} ({Year})/Season {NN}/`
- Episodes (similarly-sized files): `{Show} - S{NN}E{NN}.{ext}`
- Odd-sized files: `Season {NN}/Extras/`

---

## Organizer Rules

### Scan behavior

- Point at any folder → scan its subdirectories
- Each subdirectory = one "folder" entry in the table
- For each folder: TMDb lookup → suggested name + confidence score
- Already-named folders: show "OK" (no rename needed, but may still need moving)

### Action buttons

| Button | What it does |
|--------|-------------|
| Accept All | Apply all suggested renames and moves at once |
| Per-folder accept/reject | Granular control — one folder at a time |
| Preview | Show what will change before committing anything |

### Confidence scoring

| Score | Meaning | Action |
|-------|---------|--------|
| ≥0.9 | TMDb match with year confirmed | Auto-accept on "Accept All" |
| 0.7–0.9 | TMDb match, year missing/ambiguous | Manual review required |
| <0.7 | No TMDb match | Reject/quarantine |

### Move behavior

- If source ≠ destination: move the folder (even if name is already correct)
- If source = destination: skip (nothing to do)
- User previews all changes before anything moves

---

## Unidentified Media

When TMDb fails to find a disc (obscure movie, wrong title, API down):

- The rip stays in the temp folder with the raw disc name
- It appears in the Organizer for manual review
- It can be cleaned up by age (configurable, default 7 days)
- The user can manually search, accept a suggestion, or reject

---

## Session State

The program remembers what you just did, so you don't have to repeat work:

- **Last ripped disc** — its TMDb match, season/episode info
- **When it's used** — when the next disc has a similar name, the program suggests
  the inferred next episode/disk
- **Persistence** — survives restarts. Resettable from Settings
- **Scope** — not tied to a single server session; it's persistent state

---

## What It Doesn't Do

- No streaming
- No poster downloads (TMDb thumbnails in UI only)
- No "library catalog" database — the folder structure *is* the catalog
- No automatic disc detection (no daemon watching drives)
- No Plex integration — Jellyfin naming convention only

---

## Constraints

- **Single binary** — no Docker, no PostgreSQL, no special users
- **Minimum permissions** — no root required, no `arm` user
- **Network drive support** — read/write to any path with permissions
- **TMDb + OMDb** — TMDb primary, OMDb fallback
- **MakeMKV** — the ripper backend (via CLI)
- **SQLite** — minimal persistence for session state and config
- **Web UI** — SvelteKit frontend, not a desktop app
