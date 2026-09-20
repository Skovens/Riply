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
Click Rip (or autorip triggers) → rip to temp folder
    ↓
During rip: TMDb lookup runs in background
    ↓
On completion:
  If named → move to library with Jellyfin naming
  If not named → keep in temp, show in unfinished list below drives
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
                              ↘ unfinished list (if not named) ↗
```

Three staging areas:

| Area | Path | Lifespan | Purpose |
|------|------|----------|---------|
| **Temp** | `/tmp/riply/0/` | Short-lived | Raw rip output, cleaned up by age |
| **Staging** (organizer_dir) | configurable | Until reviewed | Post-rip, pre-review, name may be raw or auto-detected |
| **Library** | `movies_dir` / `tv_dir` | Permanent | Final Jellyfin-compliant structure |

Network paths are supported for all three (any path with read/write permissions).

---

## Drive Management

Multiple drives are shown simultaneously as individual cards. Each card shows:

| State | Display |
|-------|---------|
| No disc | "No disc" with Rip/Eject buttons disabled |
| Disc inserted | Disc name + thumbnail (if found) or gray placeholder (if not found) + Rip/Eject buttons enabled |
| Ripping | Progress bar + stage name + ETA |
| Complete | Disc cover + auto-detected name + type |
| Error | Descriptive error message (e.g., "Disc unreadable — clean and retry") |

Each drive is an isolated "island" — two drives can rip different shows simultaneously
without interfering with each other's session memory.

### Autorip

Per-drive toggle. When enabled:
- Monitors the drive for disc insertion
- When a disc is inserted, rips immediately (no name confirmation)
- After the rip completes, ejects the disc and waits for the next insertion
- If a rip is already in progress when autorip triggers, the current rip finishes,
  then ejects, then auto-starts the next disc

### Unfinished Titles

Rips that fail to auto-name stay in the Rip view, not the Organizer. Below the drive cards,
an "Unfinished" list shows:

- **Named successfully:** `Original Disc Title → Auto-Detected Title (Year)`
- **Naming failed:** `Original Disc Title → Media naming failed, please select title`

The user can click to open a search popup and manually name the disc.

---

## Auto-Naming Rules

### Detection pipeline

1. **Parse disc name** — MakeMKV provides a raw name like `KLOVN_SEASON_1_DISK_1`
2. **Search TMDb** — movies first, then TV if no movie match
3. **Session memory** — if the last ripped disc was "Klovn S01", the next disc labeled
   "Klovn" suggests "Klovn S01E02" (inferred from the previous rip on the same drive)
4. **User confirmation** — always required. The UI shows the suggested name with a
   thumbnail. The user can accept, search for a different match, or reject.

### Search popup interaction

When the user clicks a thumbnail or gray placeholder:
1. A centered popup opens with a search field
2. As the user types, TMDb suggestions appear in real time
3. User picks a result → the name updates in place on the drive card
4. For TV shows: if the show is known but season/episode is not, the show name is
   pre-filled and the user can accept it (season/episode will be inferred later)

### What gets shown in the UI

| Condition | Display |
|-----------|---------|
| TMDb found | Thumbnail cover + title + year |
| TMDb not found | Gray placeholder + "media title not found" |
| Session hint available | Suggested name pre-filled, user can accept or change |
| No TMDb/OMDb configured | Manual name entry mode — user types title, year, season, episode |

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

### Preview

Clicking "Preview" shows a dry-run list of all changes before committing:

**Basic view (collapsed):**
```
SPEED/ → Speed (1994)/
INCEPTION_2010/ → Inception (2010)/
```

**Expanded view:**
```
SPEED/ → Speed (1994)/
  ├─ SPEED.mkv → Speed (1994).mkv
  └─ Behind the Scenes.mkv → Extras/Behind the Scenes.mkv
```

Nothing moves until the user confirms.

### Action buttons

| Button | What it does |
|--------|-------------|
| Accept All | Apply all accepted renames and moves at once |
| Per-folder accept/reject | Granular control — one folder at a time |
| Preview | Show what will change before committing anything |

### Accept All behavior

- Only moves folders that have been accepted (not rejected)
- Renames parent folder and main file to Jellyfin convention
- Moves extras into the `Extras/` subfolder
- Leaves rejected folders untouched

### Move behavior

- If source ≠ destination: move the folder (even if name is already correct)
- If source = destination: skip (nothing to do)
- User previews all changes before anything moves

---

## Unidentified Media

When TMDb fails to find a disc (obscure movie, wrong title, API down):

- The rip stays in the temp folder with the raw disc name
- It appears in the **unfinished list** in the Rip view (not the Organizer)
- It can be cleaned up by age (configurable, default 7 days)
- The user can click to open a search popup and manually name the disc
- If no TMDb/OMDb API key is configured, the user enters the name manually

---

## Session State

The program remembers what you just did, so you don't have to repeat work:

- **Last ripped disc** — its TMDb match, season/episode info (per drive)
- **When it's used** — when the next disc has a similar name, the program suggests
  the inferred next episode/disk
- **Persistence** — survives restarts. Resettable from Settings
- **Scope** — each drive is isolated; two drives ripping different shows won't conflict
- **Same disc twice** — if the disc name is the same, a new folder is created with
  "Copy N" suffix. If a library folder with that name already exists, the user is
  prompted to confirm overwrite.

---

## Error States

| Scenario | Behavior |
|----------|----------|
| MakeMKV fails (disc scratch, hardware) | Descriptive error message ("Disc unreadable — clean and retry"). User can retry. |
| Drive removed mid-rip | Rip aborts, progress is lost. |
| TMDb down / rate limited | Rip completes, falls back to basic/manual naming. |
| Destination path not writable | Folder stays in temp. User is notified: "Move to library failed — permission issue." |
| Disk full | Basic information displayed. MakeMKV's own prompts are not shown (too noisy). |
| No TMDb/OMDb configured | Manual name entry mode. Rip still works, just no auto-naming. |

---

## Configuration

### First run

- Library paths (`movies_dir`, `tv_dir`) must be set before the first rip
- A popup prompts the user to configure them on first launch
- TMDb/OMDb API keys are optional — without them, the program falls back to manual naming

### Settings page

- Library paths
- Temp directory
- MakeMKV command
- API keys (TMDb, OMDb)
- Session state (view and reset)
- Tmp cleanup age

---

## What It Doesn't Do

- No streaming
- No poster downloads (TMDb thumbnails in UI only)
- No "library catalog" database — the folder structure *is* the catalog
- No automatic disc detection (no daemon watching drives)
- No Plex integration — Jellyfin naming convention only
- No quarantine — rejected folders are left untouched

---

## Constraints

- **Single binary** — no Docker, no PostgreSQL, no special users
- **Minimum permissions** — no root required, no `arm` user
- **Network drive support** — read/write to any path with permissions
- **TMDb + OMDb** — TMDb primary, OMDb fallback
- **MakeMKV** — the ripper backend (via CLI)
- **SQLite** — minimal persistence for session state and config
- **Web UI** — SvelteKit frontend, not a desktop app
