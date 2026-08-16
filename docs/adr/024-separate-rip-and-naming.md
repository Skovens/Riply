# ADR-024: Separate Rip and Naming Flows

## Status

Proposed

## Context

Currently, clicking the "Rip" button opens the media naming dialog before ripping starts. This creates friction:

1. The user may not know what to name the media yet — ripping is the primary action.
2. The naming dialog blocks the rip action, adding an unnecessary step.
3. Auto-naming (TMDb lookup) happens silently before the rip, giving no feedback about whether a name was found.
4. The progress bar is visible but lacks percentage and time-remaining information.

ADR-022 established that naming should be a non-blocking, user-facing step. This ADR extends that decision to the rip flow specifically.

## Decision

Separate the rip action from the naming action:

### 1. Rip Button Starts Immediately

Clicking "Rip" starts the ripping process with no dialog. The naming modal is never shown at rip start.

### 2. Auto-Naming During Rip

After the rip starts, a background TMDb lookup runs on the disc name. The result is shown live in the UI:

- **Success:** `Disc: Foo Bar → Found: Foo Bar (2020)`
- **Failure:** `Disc: Foo Bar → Failed to find name` (subtle indicator)

### 3. Naming Dialog Available Anytime

The user can click the disc name at any time to open the naming modal. If a name was auto-detected, it is pre-selected so the user can accept or edit it.

### 4. Progress Bar with Percentage and ETA

The progress bar is displayed inline with the drive card and shows:
- Percentage complete
- Estimated time remaining (e.g., "45% · ~2m 30s left")

### 5. Post-Rip Folder Rename

After ripping completes:

1. The rip folder is created with the disc name (always).
2. If a name was found or set by the user:
   - The folder is renamed to the new name at rip completion (not during).
   - If the destination already exists, "(Copy N)" is applied.
   - An SSE event `rip:renamed:{index}:{new_name}` is sent.
3. If no name was found: the folder keeps the disc name.

Renaming happens at the end of ripping — not during — to avoid interfering with files being written.

### 6. Mid-Rip Name Changes

If the user opens the naming dialog during a rip and changes the name, the folder is renamed immediately. This is safe because no new files are being written at that point.

### New SSE Event Types

| Event | Format | Description |
|---|---|---|
| Auto-naming success | `rip:named:{index}:{title} ({year})` | TMDb lookup found a name |
| Auto-naming failure | `rip:naming_failed:{index}` | TMDb lookup found nothing |
| Post-rip rename | `rip:renamed:{index}:{new_name}` | Folder renamed after rip |

### Jellyfin Naming Convention

Per the file-pipeline design, ripped content is restructured into Jellyfin-compatible format:

- **Movies:** `{Title} ({Year})/` with main title file named `{Title}.{ext}`, extras in `Extras/` subfolder
- **TV Shows:** `{Series} ({Year})/Season {NN}/` with files named `{Series} - S{NN}E{NN}.{ext}`
- All ripped content from a disc is grouped into one Jellyfin-structured folder

### Consequences

- Rip action is always one click — no blocking dialogs.
- Naming is non-blocking and available anytime via the disc name.
- User gets clear feedback on auto-naming attempts (success or failure).
- Folder rename happens at rip end (safe — no files being written).
- Progress bar shows percentage + ETA for better UX.
- Mid-rip name changes rename the folder immediately.

## Implementation

- **Backend:** `web/src/routes/ripper.rs` — auto-naming after rip start, post-rip rename, new SSE events
- **Frontend:** `web/svelte/src/routes/+page.svelte` — rip button skips naming dialog, inline progress bar, live naming feedback
- **API:** `web/svelte/src/lib/api/parse-event.ts` — new event parsers for `rip:named`, `rip:naming_failed`, `rip:renamed`
- **Modal:** `web/svelte/src/lib/components/MediaNamingModal.svelte` — pre-select auto-detected name
