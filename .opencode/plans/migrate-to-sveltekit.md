# Migrate to SvelteKit

## Status: DONE

## Problem

The frontend is currently vanilla JS with ES modules, embedded in the Axum
binary via `include_str!()`. The project should use SvelteKit instead.

## Architecture Decisions

- **SvelteKit 2 + Svelte 5** — Current stable (SvelteKit 2.57, Svelte 5.55)
- **TypeScript** — First-class support, auto-generated `$types` for routes
- **Svelte 5 runes** — `$state`, `$derived`, `$effect`, `$props()` (not legacy `let`/`$: `)
- **Modern syntax** — `onclick` (not `on:click`), `{#snippet}` (not `<slot>`)
- **adapter-static** — Build outputs static files served by Axum
- **No Tailwind** — Keep existing CSS custom properties
- **No separate Node.js server** — Build output embedded in Axum binary
- **Development** — SvelteKit dev server proxies to Axum backend
- **Keep prompt()** — Scan directory input stays as `window.prompt()` for now

## Current Frontend Stats

| Metric | Value |
|--------|-------|
| Total frontend files | 17 (10 JS + 6 CSS + 1 HTML) |
| Total frontend lines | 2,637 |
| Embedded files in Rust | 17 `include_str!()` constants |
| Static routes in Rust | 17 `.route()` calls to remove |
| API endpoints to preserve | 18 + health + SSE |
| Components with polling | 2 (jobs 2s, ripper 3s) |
| Global state keys | 11 |

## Phases

### Phase 1: Setup SvelteKit Project

**Create `web/svelte/` directory with SvelteKit scaffolding.**

Use `npx sv create` or manually scaffold:

```bash
cd web
npx sv create svelte
# Choose: SvelteKit minimal, TypeScript, npm
```

Files to create:
- `web/svelte/package.json` — SvelteKit + deps
- `web/svelte/svelte.config.js` — adapter-static config
- `web/svelte/vite.config.ts` — Vite config with proxy to Axum (`localhost:3000`)
- `web/svelte/tsconfig.json` — TypeScript config
- `web/svelte/src/app.html` — HTML template
- `web/svelte/src/app.css` — Global CSS (import existing design tokens)
- `web/svelte/src/routes/+layout.svelte` — Root layout (header, nav, footer)
- `web/svelte/src/routes/+page.svelte` — Dashboard (placeholder)
- `web/svelte/static/` — Static assets

**Do NOT delete old frontend yet.** Both coexist during migration.

### Phase 2: Migrate Store

**Replace `store.js` with Svelte 5 runes or writable stores.**

Current: Custom `Store` class with `set()`, `subscribe()`, `on()`
Target: Svelte `writable()` stores (compatible with `$store` auto-subscribe)

```ts
// Current
export const store = new Store({ files: [], connected: false });
store.set({ files: data });
store.on('files', (files) => render(files));

// Target
import { writable } from 'svelte/store';
export const files = writable<File[]>([]);
export const connected = writable(false);
// In components: $files = data;
```

Files to create:
- `web/svelte/src/lib/stores/index.ts` — All stores

Store keys (from current `store.js`):
`connected`, `error`, `files`, `filesTotal`, `filesLoading`,
`reviewQueue`, `reviewLoading`, `drives`, `drivesLoading`,
`moveStatus`, `config`, `lastEvent`

### Phase 3: Migrate API Client

**Replace `api.js` with TypeScript fetch wrapper.**

Current: `api.js` with `request()` helper and domain-specific methods
Target: Same structure, TypeScript types

Files to create:
- `web/svelte/src/lib/api/index.ts` — `request()` helper + `ApiError`
- `web/svelte/src/lib/api/files.ts` — Files API
- `web/svelte/src/lib/api/review.ts` — Review API
- `web/svelte/src/lib/api/scan.ts` — Scan API
- `web/svelte/src/lib/api/jobs.ts` — Jobs API
- `web/svelte/src/lib/api/ripper.ts` — Ripper API
- `web/svelte/src/lib/api/config.ts` — Config API
- `web/svelte/src/lib/api/move.ts` — Move API
- `web/svelte/src/lib/api/sse.ts` — SSE connection

### Phase 4: Migrate Components to Svelte

**Convert each vanilla JS component to a Svelte 5 component.**

Current: Render functions that set `innerHTML`
Target: Svelte components with reactive templates

| Current (261 lines) | Target |
|---------------------|--------|
| `components/dashboard.js` | `src/routes/+page.svelte` |
| `components/files.js` (131) | `src/routes/files/+page.svelte` |
| `components/review.js` (85) | `src/routes/review/+page.svelte` |
| `components/jobs.js` (140) | `src/routes/jobs/+page.svelte` |
| `components/ripper.js` (129) | `src/routes/ripper/+page.svelte` |
| `components/settings.js` (196) | `src/routes/settings/+page.svelte` |

Each component should:
- Use `$props()` for parameters (not `export let`)
- Use `$state` for local reactive state
- Use `$derived` for computed values
- Use `$effect` for side effects (polling, SSE)
- Use `onclick` (not `on:click`)
- Use `{#each items as item (item.id)}` with keyed blocks
- Call API methods directly, update stores
- Use `onMount`/`onDestroy` for polling cleanup

### Phase 5: Migrate SSE Integration

**Replace manual `EventSource` with Svelte store integration.**

Current: `connectSSE()` in `app.js`, manually updates store
Target: SSE store with auto-reconnect

Files to create:
- `web/svelte/src/lib/stores/sse.ts` — SSE store with reconnect logic

Connection established in `+layout.svelte`, events flow through stores.

### Phase 6: Routing

**Replace hash-based routing with SvelteKit file-based routing.**

Current: `RiplyApp.navigate()` with manual component switching
Target: SvelteKit file-based routes

Routes:
- `/` — Dashboard
- `/files` — File list
- `/review` — Review queue
- `/jobs` — Job list
- `/ripper` — Drive list
- `/settings` — Config

Header nav uses `<a href="/files">` with SvelteKit's `page` store for active state.

### Phase 7: Build Integration

**Configure Axum to serve SvelteKit build output.**

Current: `include_str!()` serves 17 individual files from `web/static/`
Target: `include_str!()` serves SvelteKit build output from `web/svelte/build/`

The current `api.rs` has:
- 17 `include_str!()` constants (one per static file)
- 17 `.route()` calls (one per file)
- `css_response()` and `js_response()` helpers

After migration:
- Remove all 17 `include_str!()` constants
- Remove all 17 `.route()` calls for static files
- Remove `css_response()` and `js_response()` helpers
- Add a catch-all route that serves `index.html` for SPA routing
- SvelteKit's hashed filenames handle caching automatically

Files to modify:
- `web/Cargo.toml` — May need to adjust paths
- `web/src/api.rs` — Remove static file routes, add SPA fallback

### Phase 8: Cleanup

**Remove old vanilla JS files.**

Files to delete:
- `web/static/js/` — All old JS files (10 files)
- `web/static/index.html` — Old entry point
- `web/static/css/` — Old CSS (migrated to Svelte)

Files to verify don't exist:
- `.opencode/node_modules/` — Should not be in repo
- `.opencode/package.json` — Should not be in repo

## Verification

After each phase:
1. `cargo build --all` — Ensure Axum still compiles
2. `cargo test --all` — Ensure no regressions
3. `cd web/svelte && npm run build` — Ensure SvelteKit builds (after Phase 1)

After all phases:
1. `cargo run -p cli -- serve` — Start the server
2. Open `http://localhost:3000` — Verify SvelteKit UI loads
3. Test all views: Dashboard, Files, Review, Jobs, Ripper, Settings
4. Test SSE: Trigger scan, verify progress updates appear
5. Test API: Scan, analyze, review, move pipeline works end-to-end
6. Verify `prompt()` still works for scan directory input

## Files Summary

| Phase | Files Created | Files Modified | Files Deleted |
|-------|--------------|----------------|---------------|
| 1 | ~10 SvelteKit scaffolding | — | — |
| 2 | 1 store file (.ts) | — | — |
| 3 | 9 API files (.ts) | — | — |
| 4 | 6 Svelte components (.svelte) | — | — |
| 5 | 1 SSE store (.ts) | — | — |
| 6 | Routes (in Phase 4) | — | — |
| 7 | — | 1 (api.rs) | — |
| 8 | — | — | ~17 old JS/HTML/CSS files |

## Verification

- `cargo build --all` ✅
- `cargo test --all` ✅ (139 tests passed)
- `cd web/svelte && npm run build` ✅
- All phases completed successfully
