# ADR-020: Rip Progress Bar in Drive Panel

## Status

Proposed

## Context

While a disc rips, the dashboard's `DrivePanel.svelte` shows a spinner
(emoji) with no numeric progress. The progress data already exists:
MakeMKV emits `PRGV:` lines parsed by `infra/src/makemkv/parser.rs`
(`ParsedProgress`: current, total, percent), `DryRunBackend` simulates
them, and `web/src/routes/ripper.rs` already computes percent + ETA and
pushes `rip:progress:{index}:{pct}` (and `:eta`) events over SSE. The
gap is purely frontend: `DrivePanel` never subscribes to `connectSSE()`
(`web/svelte/src/lib/api/sse.ts`).

## Decision

Surface the existing rip progress as a bar in the drive panel.

- `DrivePanel.svelte` subscribes to the SSE stream and updates a
  per-drive progress value from `rip:progress:{index}:{pct}` events,
  rendering a styled progress bar instead of the spinner while a rip is
  active.
- Add a `percent` field to the drive-status response as a poll fallback,
  so the bar survives SSE disconnects (SSE is best-effort, ADR-011).
- ETA, when present, is shown next to the bar.

No pipeline changes: the ripper already computes and streams progress.

## Consequences

- Ripping becomes visibly progress-tracked instead of an indeterminate
  spinner.
- The change is confined to the frontend plus one optional field on the
  drives endpoint.
- Progress UX degrades gracefully to polled values if SSE drops.
