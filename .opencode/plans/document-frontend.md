# Document Frontend Architecture

## Status: DONE

## Problem

The frontend uses a custom vanilla JS architecture with no documentation.
An agent working on frontend code wouldn't know:
- How routing works
- How state management works
- How components are structured
- How SSE integration works
- How the API client is organized

This documentation is also needed as a reference BEFORE the SvelteKit migration,
so the migration knows exactly what patterns to replace.

## What to Document

### 1. Architecture Overview

```
app.js          — Entry point, RiplyApp class, routing
store.js        — Pub/sub state management
api.js          — Fetch wrapper, organized by API domain
utils.js        — Shared helpers (escapeHtml, formatSize, statusColor)
components/     — Render functions (dashboard, files, review, jobs, ripper, settings)
```

### 2. Routing

Hash-based: `#/dashboard`, `#/files`, `#/review`, `#/ripper`, `#/jobs`, `#/settings`

`RiplyApp.navigate(view)` updates `currentView` state and calls the
appropriate render function. Navigation buttons in the header call
`window.riply.navigate()`.

### 3. State Management (`store.js`)

- `Store` class with `set(updates)` (shallow merge) and `subscribe(listener)`
- `store.on(key, listener)` — subscribe to a specific key
- Initial state: `connected`, `files`, `reviewQueue`, `drives`, `moveStatus`, `config`, `lastEvent`

### 4. API Client (`api.js`)

- `request(endpoint, options)` — generic fetch wrapper with error handling
- `ApiError` class with status code
- Organized by domain: `files.*`, `review.*`, `scan.*`, `analyze.*`, `jobs.*`, `move.*`, `ripper.*`, `config.*`
- `connectSSE(onMessage)` — EventSource connection to `/api/sse`

### 5. Component Pattern

Components are render functions that return HTML strings and attach event listeners:

```js
export async function renderDashboard(container) {
  const html = `<div class="dashboard">...</div>`;
  container.innerHTML = html;
  // Attach event listeners after rendering
}
```

Each component receives the `container` element and renders into it.
Components poll for updates using `setInterval` + store subscriptions.

### 6. SSE Events

Connected via `connectSSE()` in `app.js`. Events update the store,
which triggers component re-renders via subscriptions.

Events: `event:status`, `event:progress`, `event:log`

### 7. CSS Architecture

- `variables.css` — CSS custom properties (dark/light themes)
- `base.css` — Reset and base styles
- `layout.css` — Grid, nav, header, footer
- `components.css` — Component-specific styles
- `utilities.css` — Utility classes

Theme switching via `data-theme` attribute on `<html>`.

## Files

| File | Action |
|------|--------|
| `docs/frontend-architecture.md` | Created |

## Verification

After writing:
- All 6 components mentioned ✅
- Routing pattern documented ✅
- Store API documented ✅
- API client structure documented ✅
- CSS architecture documented ✅
- Global API (`window.riply`) documented ✅
- SSE connection pattern documented ✅
- Data flow diagram included ✅
