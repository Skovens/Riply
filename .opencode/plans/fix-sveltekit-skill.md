# Fix sveltekit-realtime Skill

## Status: DONE

## Problem

The `.agents/skills/sveltekit-realtime.md` skill documents SvelteKit + Tailwind + Axios,
but the actual frontend is vanilla JS with ES modules. More importantly, the skill has
several inaccuracies that would mislead an agent:

1. **Axios** — SvelteKit uses `fetch` natively; no Axios needed
2. **Tailwind** — The project uses custom CSS with CSS custom properties
3. **Atomic Design** — Overly complex for this project's component size
4. **SSE events** — Listed as generic names; should match actual API endpoints
5. **API client** — Describes interceptors and auth; project has neither

## Fix

Rewrite the skill to accurately document SvelteKit patterns for this project.
The skill should reflect the TARGET architecture (SvelteKit), not the current
vanilla JS state (which will be migrated separately).

### What the Skill Should Document

1. **SvelteKit project structure** under `web/`
2. **Component pattern** — simple Svelte components, not atomic design
3. **State management** — Svelte stores (writable/readable)
4. **SSE integration** — `EventSource` connected to `/api/sse`
5. **API client** — `fetch` wrapper matching the actual Axum endpoints
6. **CSS** — Keep existing CSS custom properties, no Tailwind
7. **Build integration** — SvelteKit build output served by Axum

### Actual API Endpoints to Document

```
GET  /health
GET  /api/files?page=&per_page=&status=
GET  /api/review
POST /api/review/:id/accept
POST /api/review/:id/reject
POST /api/scan
POST /api/analyze
POST /api/move
GET  /api/move/status
GET  /api/jobs
GET  /api/jobs/:id
POST /api/jobs/:id/cancel
GET  /api/ripper/drives
POST /api/ripper/:index/start
POST /api/ripper/:index/cancel
GET  /api/config
PUT  /api/config
GET  /api/sse
```

### SSE Events

```
event:status    — pipeline status changes
event:progress  — progress updates (scan, analyze, move, rip)
event:log       — log messages
```

## Files

| File | Action |
|------|--------|
| `.agents/skills/sveltekit-realtime.md` | Rewrite content |

## Decision Needed

**Tailwind vs custom CSS?** The current skill mentions Tailwind. The project
uses custom CSS (`web/static/css/`). Options:

- **Keep custom CSS** — Less migration work, consistent with existing design
- **Add Tailwind** — More standard, utility classes, but adds build step

Recommendation: Keep custom CSS. It's working and removing it adds unnecessary work.

## Verification

After rewriting, check that the skill:
- Accurately describes SvelteKit patterns ✅
- References correct API endpoints ✅
- Documents SSE event format ✅
- Mentions build integration with Axum ✅
- Uses fetch, not Axios ✅
- Uses custom CSS, not Tailwind ✅
- Simple components, not atomic design ✅
