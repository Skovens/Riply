---
name: sveltekit-realtime
description: >
  Frontend development standards for Riply using SvelteKit.
  Covers component structure, CSS custom properties, SSE integration for real-time progress,
  and API client patterns matching the Axum backend.
metadata:
  language: svelte, javascript
  version: "2.0+"
  compatibility: sveltekit, sse, fetch
---

# SvelteKit Realtime Frontend

## General Principles

- **Simple components** — One component per route, no atomic design hierarchy.
- **Reactivity** — Leverage Svelte's reactivity for real-time updates.
- **Custom CSS** — Use CSS custom properties (no Tailwind). Keep existing design tokens.
- **fetch** — Native fetch API, no Axios or interceptors.

## Project Structure

```
web/svelte/
  src/
    app.html                    — HTML template
    routes/
      +layout.svelte            — Root layout (header, nav, footer)
      +page.svelte              — Dashboard
      files/+page.svelte        — File list
      review/+page.svelte       — Review queue
      jobs/+page.svelte         — Job list
      ripper/+page.svelte       — Drive list
      settings/+page.svelte     — Config form
    lib/
      stores/index.js           — Svelte writable stores
      api/
        index.js                — request() helper
        files.js, review.js,    — Domain-specific API modules
        scan.js, jobs.js,
        ripper.js, config.js,
        sse.js
  static/                       — Static assets (if any)
  svelte.config.js              — adapter-static config
  vite.config.js                — Vite config with proxy to Axum
```

## Component Pattern

Each route is a single `.svelte` file:

```svelte
<script>
  import { onMount } from 'svelte';
  import { files } from '$lib/stores';
  import { filesApi } from '$lib/api';

  let loading = true;

  onMount(async () => {
    const result = await filesApi.list({ page: 1 });
    $files = result.data?.items || [];
    loading = false;
  });
</script>

{#if loading}
  <div class="spinner"></div>
{:else if $files.length === 0}
  <div class="empty">No files found</div>
{:else}
  {#each $files as file}
    <div class="card">{file.filename}</div>
  {/each}
{/if}
```

Key points:
- Import stores, auto-subscribe with `$store` syntax
- Use `{#if}`, `{#each}` for conditional rendering and lists
- Call API methods directly, update stores
- No manual DOM manipulation — Svelte handles reactivity

## State Management

Svelte writable stores in `src/lib/stores/index.js`:

```js
import { writable } from 'svelte/store';

export const connected = writable(false);
export const files = writable([]);
export const reviewQueue = writable([]);
export const drives = writable([]);
export const moveStatus = writable({ confirmed_count: 0, moved_count: 0 });
export const config = writable(null);
export const lastEvent = writable(null);
```

Components auto-subscribe via `$store` syntax. No pub/sub helper needed — Svelte handles it.

## API Client

### Core (`src/lib/api/index.js`)

```js
const BASE_URL = '';

class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

export async function request(endpoint, options = {}) {
  const response = await fetch(`${BASE_URL}${endpoint}`, {
    headers: { 'Content-Type': 'application/json', ...options.headers },
    ...options,
  });
  if (!response.ok) {
    const text = await response.text().catch(() => response.statusText);
    throw new ApiError(response.status, text);
  }
  return response.json();
}
```

### Domain modules

```js
// src/lib/api/files.js
import { request } from './index.js';

export const filesApi = {
  list: (params) => {
    const qs = new URLSearchParams(params).toString();
    return request(`/api/files${qs ? '?' + qs : ''}`);
  },
  get: (id) => request(`/api/files/${id}`),
  update: (id, data) => request(`/api/files/${id}`, { method: 'POST', body: JSON.stringify(data) }),
  delete: (id) => request(`/api/files/${id}`, { method: 'DELETE' }),
};
```

### API Endpoints (Axum backend)

```
GET  /health
GET  /api/files?page=&per_page=&status=
GET  /api/files/:id
POST /api/files/:id
DELETE /api/files/:id
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

All API responses follow the pattern:

```json
{ "success": true, "data": { ... } }
{ "success": false, "error": "message" }
```

## SSE Integration

### Connection (`src/lib/api/sse.js`)

```js
export function connectSSE(onMessage) {
  const eventSource = new EventSource('/api/sse');
  eventSource.onmessage = (event) => onMessage(event.data);
  eventSource.onerror = (error) => console.error('SSE error:', error);
  return () => eventSource.close(); // Unsubscribe function
}
```

### SSE Events

```
event:status    — pipeline status changes (scan:progress:5:20, move:complete:...)
event:progress  — progress updates
event:log       — log messages
```

### Store integration

```js
// In +layout.svelte or a dedicated SSE component
import { onMount, onDestroy } from 'svelte';
import { connectSSE } from '$lib/api/sse';
import { lastEvent } from '$lib/stores';

let unsubscribe;

onMount(() => {
  unsubscribe = connectSSE((message) => {
    $lastEvent = message;
    // Trigger view refreshes based on event type
  });
});

onDestroy(() => { unsubscribe?.(); });
```

## CSS Architecture

Keep existing CSS custom properties. No Tailwind.

```
src/app.css or static/css/
  variables.css   — Design tokens (colors, spacing, typography, shadows)
  base.css        — Reset, typography defaults
  layout.css      — Page structure (header, nav, main, footer, grid)
  components.css  — Cards, badges, buttons, tables, forms
  utilities.css   — Helper classes (flex, gap, text-muted)
```

### Design tokens (examples)

```css
:root {
  --color-bg-primary: #0f1117;
  --color-text-primary: #e4e6f0;
  --color-accent: #6366f1;
  --color-success: #22c55e;
  --color-warning: #f59e0b;
  --color-error: #ef4444;
  --space-4: 1rem;
  --radius-md: 0.375rem;
}
```

Light mode via `@media (prefers-color-scheme: light)` override. BEM-ish naming: `.header__nav-link`, `.card--compact`, `.badge--success`.

## Build Integration

### SvelteKit config (`svelte.config.js`)

```js
import adapter from '@sveltejs/adapter-static';

export default {
  kit: {
    adapter: adapter({ outDir: 'build' }),
  },
};
```

### Vite config (`vite.config.js`)

```js
import { sveltekit } from '@sveltejs/kit/vite';

export default {
  plugins: [sveltekit()],
  server: {
    proxy: {
      '/api': 'http://localhost:3000',
      '/health': 'http://localhost:3000',
    },
  },
};
```

### Axum serving

The SvelteKit build output (`web/svelte/build/`) is embedded in the Axum binary via `rust-embed` or `include_str!`. After migration, update `web/src/api.rs` to serve the built files instead of the current vanilla JS files.

### Development workflow

```bash
# Terminal 1: Axum backend
cargo run -p cli -- serve

# Terminal 2: SvelteKit dev server (proxies API to Axum)
cd web/svelte && npm run dev
```
