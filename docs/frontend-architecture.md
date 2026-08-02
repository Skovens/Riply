# Frontend Architecture

SvelteKit frontend served by the Axum backend. Built to `web/svelte/build/` and embedded
in the binary via Axum's `ServeDir` middleware.

## File Overview

```
web/svelte/
  package.json              — SvelteKit + Vite dependencies
  svelte.config.js          — Static adapter, output to build/
  vite.config.ts            — Vite config, dev proxy to :3000
  tsconfig.json             — Strict TS, extends .svelte-kit generated config

  src/
    app.html                — HTML shell (no JS framework hydration)
    app.css                 — Global styles (CSS custom properties)

    lib/
      api/                  — Typed API client modules
        index.ts            — Barrel export
        request.ts          — Fetch wrapper (headers, error handling)
        sse.ts              — SSE connection manager
        files.ts            — /api/files endpoints
        review.ts           — /api/review endpoints
        scan.ts             — /api/scan endpoints
        analyze.ts          — /api/analyze endpoints
        jobs.ts             — /api/jobs endpoints
        move.ts             — /api/move endpoints
        ripper.ts           — /api/ripper endpoints
        config.ts           — /api/config endpoints

      css/                  — Component-scoped CSS
        variables.css       — Design tokens (colors, spacing, typography)
        base.css            — Reset, typography defaults
        layout.css          — Page structure (header, nav, main)
        components.css      — Cards, badges, buttons, tables, forms
        utilities.css       — Helper classes (flex, gap, truncate)
        style.css           — Import chain (variables → base → layout → components → utilities)

      stores/
        index.ts            — Svelte store re-exports

      types.ts              — Shared TypeScript types (FileRecord, RipEvent, etc.)

    routes/                 — SvelteKit file-based routing
      +layout.js            — Shared layout loader
      +layout.svelte        — App shell (header nav + main content area)
      +page.svelte          — Dashboard (root route)
      files/+page.svelte    — File list with pagination
      review/+page.svelte   — Review queue
      jobs/+page.svelte     — Background jobs with progress
      ripper/+page.svelte   — Drive list, rip controls, progress bars
      settings/+page.svelte — Config form
```

## Routing

SvelteKit file-based: route directories map to URL paths.

| Route | Page |
|-------|------|
| `/` | Dashboard (`+page.svelte`) |
| `/files` | File list |
| `/review` | Review queue |
| `/jobs` | Background jobs |
| `/ripper` | Disc ripping |
| `/settings` | Configuration |

Navigation is handled by SvelteKit's `<a>` links — no hash-based routing.

## State Management

Svelte stores (reactive variables). Each page component loads data via SvelteKit's
`load` functions or on-interaction fetches. No centralized global store — each page
manages its own data.

## API Client

Typed TypeScript modules in `lib/api/`. Each domain has its own file:

```ts
// lib/api/request.ts
async function request<T>(endpoint: string, options?: RequestInit): Promise<T>
// Throws ApiError with status and message

// lib/api/ripper.ts
export async function listDrives(): Promise<DriveInfo[]>
export async function startRip(index: number, dest: string): Promise<void>
export async function cancelRip(index: number): Promise<void>
```

SSE connection via `lib/api/sse.ts`:

```ts
const unsubscribe = connectSSE((event) => {
  // Handle rip:progress, rip:complete, job:progress, etc.
});
```

## Component Pattern

Svelte components (`.svelte` files). Each page is a self-contained component:

```svelte
<script>
  // Load data on mount or via SvelteKit load
  let files = $state<FileRecord[]>([]);
  let loading = $state(true);
</script>

<main>
  {#if loading}
    <p>Loading...</p>
  {:else}
    {#each files as file}
      <div class="card">{file.filename}</div>
    {/each}
  {/if}
</main>
```

Polling via `setInterval` in onMount:
- **Jobs page**: polls `/api/jobs` every 2s while active jobs exist
- **Ripper page**: polls drive status every 3s during active rips

## CSS Architecture

Import chain in `style.css`:

```css
@import url('variables.css');   /* Design tokens */
@import url('base.css');        /* Reset, typography */
@import url('layout.css');      /* Page structure */
@import url('components.css');  /* Cards, badges, buttons, tables */
@import url('utilities.css');   /* Flex, gap, text helpers */
```

- **Dark mode default** via `:root` variables
- **Light mode** via `@media (prefers-color-scheme: light)` override
- **Naming**: BEM-ish — `.header__nav-link`, `.card--compact`, `.badge--success`
- **Theme tokens**: `--color-bg-*`, `--color-text-*`, `--color-accent`, `--color-success/warning/error`
- **Spacing scale**: `--space-1` through `--space-12`
- **Z-index scale**: `--z-base` (0) → `--z-dropdown` (100) → `--z-sticky` (200) → `--z-modal` (300) → `--z-toast` (400)

## Data Flow

```
User action (click/form submit)
  → Svelte event handler
    → API call (lib/api/*.ts)
      → Backend responds
        → Svelte $state update
          → Template re-renders

SSE event from /api/sse
  → connectSSE callback
    → $state update (progress bars, job status)
      → Template re-renders affected components
```
