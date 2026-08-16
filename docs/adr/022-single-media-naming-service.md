# ADR-022: Single Media-Naming Service

## Status

Proposed

## Context

Media naming logic is duplicated:

1. `DetailDrawer.svelte` inlines a debounced search-then-apply flow
   (`searchApi.search` -> pick result -> `filesApi.update` with
   canonical_title/year, series fields).
2. `resolve_rip_title()` in `web/src/routes/ripper.rs` is an unattended
   auto-namer: it parses the disc name, cleans it, searches, and silently
   takes the first TMDb match.

Each new place that needs to name media reimplements the flow, so the
logic drifts (different debounce, different fallback, different fields).
Rip naming also needs a user-facing step: click the disc being ripped ->
a popup to search and name the title.

## Decision

Extract one media-naming function and one UI used everywhere.

- One reusable modal component (search box + results + custom-title
  fallback) that *returns* a chosen `MetadataMatch` or custom title.
  Callers apply the choice through their own endpoint.
- One shared client helper in `$lib/api/` wrapping `searchApi.search` +
  the modal, used by:
  - DetailDrawer (replaces the inline flow)
  - The rip preview (clicking a ripped disc opens the modal)
  - The review queue (ADR-004/007 review step)
- `/v1/search` remains the single search backend (`Vec<MetadataMatch>`).
  No new search path is added; a minimal endpoint to attach a chosen
  name to a title group / rip output is added only if rip naming needs
  it.
- `resolve_rip_title` becomes a *prefill*: its auto-lookup fills the
  modal's search box as a default suggestion, and the user confirms,
  instead of silently committing the first match before anyone sees it.
  Per ADR-024, auto-naming runs *during* the rip (not before), and the
  result is shown live as `"Disc: X → Found: Y"`.
- Phase 1: naming applies after the rip (renames the rip folder). Per
  ADR-024, the rip folder is renamed at rip completion if a name was
  found or set by the user.
- Phase 2 (optional): pass the chosen name into makemkv so the
  output folder is named correctly at rip time.

## Consequences

- One naming flow everywhere: identical UX for file, rip, and review.
- One place to fix debounce/fallback/validation bugs.
- The surprise of silent auto-naming is removed (lookup becomes a
  suggestion).
- DetailDrawer shrinks; drift risk between naming UIs disappears.
- Scope is explicit: phase 1 is post-rip naming; pre-rip naming is a
  separate, optional phase.
