# ADR-025: Three-Window Architecture

## Status

Accepted

## Context

Riply's architecture evolved from a single dashboard (file-based pipeline, stats bar, review queue) to three distinct windows. This decision defines the technical architecture of the frontend routing and state management.

Product behavior is defined in [PRODUCT.md](../PRODUCT.md). This ADR captures the technical decisions.

## Decision

Three separate windows with distinct purposes, implemented via SvelteKit file-based routing:

| Window | Route | Purpose |
|--------|-------|---------|
| Rip | `/ripper` | Drive cards, rip controls, progress |
| Organizer | `/organizer` | Folder list, naming, bulk move |
| Settings | `/settings` | Configuration |

The root route `/` redirects to `/ripper`.

Navigation uses SvelteKit `<a>` links — no hash-based routing.

Each window manages its own data via Svelte stores ($state). No centralized global store.

### State Management

- **Rip window**: `ripProgress` (per-drive progress), `ripNaming` (auto-naming status), `autoripDrives` (session-scoped)
- **Organizer window**: `folders` (scan results), `reviewQueue` (manual review items)
- **Settings window**: `currentConfig` (loaded from API)

### SSE Integration

All windows share a single SSE connection. See `web/svelte/ARCHITECTURE.md` for frontend details. Events are dispatched to the active window:
- `rip:*` → Rip window
- `job:*` → Any window with job tracking
- `scan:*` → Organizer window

### Consequences

**Positive:**
- Each window has a single, clear purpose
- Independent state management per window
- Shared SSE connection for real-time updates
- SvelteKit routing is simple and standard

**Negative:**
- Users must manually switch between windows
- No shared state between windows (by design)

## Supersedes

- ADR-020 (Rip Progress Bar) — replaced by drive card progress
- ADR-021 (Collapsible Folder Preview) — replaced by Organizer expandable folders
- ADR-022 (Single Media-Naming Service) — replaced by Organizer window
- ADR-024 (Separate Rip and Naming Flows) — replaced by Rip + Organizer windows
