# Frontend Architecture

Vanilla JS frontend embedded in the Axum binary via `include_str!`.

## File Overview

```
web/static/
  index.html            — Entry point, minimal DOM shell
  css/
    style.css           — Import chain (variables → base → layout → components → utilities)
    variables.css       — Design tokens (colors, spacing, typography, shadows, z-index)
    base.css            — Reset, typography defaults
    layout.css          — Page structure (header, nav, main, footer, grid)
    components.css      — Reusable elements (cards, badges, buttons, tables, forms)
    utilities.css       — Helper classes (flex, gap, truncate, text-muted, etc.)
  js/
    app.js              — RiplyApp class, init, routing, SSE, global API
    store.js            — Store class, pub/sub state management
    api.js              — Fetch wrapper, domain-specific API objects
    utils.js            — escapeHtml, formatSize, statusColor
    components/
      dashboard.js      — Overview: stats, drives, recent files, review queue
      files.js          — Paginated file list with status filtering
      review.js         — Review queue with accept/reject
      jobs.js           — Background jobs with progress bars, polling
      ripper.js         — Drive list, rip start/cancel, polling
      settings.js       — Config form with dirty-tracking, save
```

## Entry Point

`index.html` loads a single ES module:

```html
<script type="module" src="/static/js/app.js?v=2"></script>
```

The DOM is minimal — header with nav buttons, `<main id="main-content">`, footer.
Navigation buttons use inline handlers: `onclick="window.riply.navigate('view')"`

## Routing

Hash-based: `#/dashboard`, `#/files`, `#/review`, `#/ripper`, `#/jobs`, `#/settings`

```
handleRoute()  →  parse hash  →  renderView()  →  dispatch to component
                  updateNav()     stopPolling()    (dashboard/files/review/jobs/ripper/settings)
```

- `handleRoute()` parses `window.location.hash`, defaults to `dashboard`
- `renderView()` clears `<main>`, calls the matching render function
- `updateNav()` toggles `header__nav-link--active` on nav buttons
- Route changes call `stopJobsPolling()` and `stopRipperPolling()`

## State Management (`store.js`)

```js
const store = new Store(initialState);

store.set({ files: data });          // Shallow merge, notify listeners
store.subscribe((state, prev) => {}); // Global listener, returns unsubscribe fn
store.on('files', (val, prev) => {}); // Key-specific listener
```

Initial state:

| Key | Type | Purpose |
|-----|------|---------|
| `connected` | bool | Backend health check result |
| `files` | array | Current file list |
| `filesTotal` | number | Total file count |
| `filesLoading` | bool | Loading state |
| `reviewQueue` | array | Files needing review |
| `drives` | array | Disc drive list |
| `moveStatus` | object | `{ confirmed_count, moved_count }` |
| `config` | object | App configuration |
| `lastEvent` | string | Most recent SSE message |

## API Client (`api.js`)

Generic fetch wrapper:

```js
async function request(endpoint, options = {})
// Returns parsed JSON, throws ApiError(status, message)
```

Domain objects with methods:

| Domain | Methods | Endpoint Pattern |
|--------|---------|-----------------|
| `files` | `list(params)`, `get(id)`, `update(id, data)`, `delete(id)` | `/api/files` |
| `review` | `list()`, `accept(id)`, `reject(id)` | `/api/review` |
| `scan` | `start(directory)` | `/api/scan` |
| `analyze` | `start()` | `/api/analyze` |
| `jobs` | `list()`, `get(id)`, `cancel(id)` | `/api/jobs` |
| `move` | `start(options)`, `status()` | `/api/move` |
| `ripper` | `listDrives()`, `start(index, dest)`, `cancel(index)` | `/api/ripper` |
| `config` | `get()`, `update(data)` | `/api/config` |

SSE connection:

```js
const unsubscribe = connectSSE((message) => {
  // Handle event:status, event:progress, event:log
});
// Call unsubscribe() to close EventSource
```

## Component Pattern

Each component is an exported render function:

```js
export async function renderDashboard(container) {
  container.innerHTML = `<section>...</section>`;
  // Load data, update DOM, attach listeners
}
```

Pattern:
1. Set `container.innerHTML` with skeleton HTML (spinner placeholders)
2. Call async load functions that fetch data and update the DOM
3. Attach event listeners to buttons/inputs after rendering
4. Some components export a `stopPolling()` function for cleanup

Polling components:
- **jobs.js**: `setInterval(loadJobs, 2000)` while active jobs exist
- **ripper.js**: `setInterval(loadDrives, 3000)` while drives are reading/ripping

## Global API (`window.riply`)

Set up by `RiplyApp.setupGlobals()`, called from inline HTML handlers:

| Method | Behavior |
|--------|----------|
| `riply.scan()` | Prompt for directory → `scan.start()` → toast → navigate to jobs |
| `riply.analyze()` | `analyze.start()` → toast → navigate to jobs |
| `riply.moveFiles()` | Confirm → `move.start()` → toast → navigate to jobs |
| `riply.review.accept(id)` | `review.accept(id)` → toast → re-render |
| `riply.review.reject(id)` | `review.reject(id)` → toast → re-render |
| `riply.navigate(view)` | Set `window.location.hash` |

Toast notifications: `showToast(message, type)` — auto-dismiss after 3s.

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
User action (click/prompt)
  → window.riply.* handler
    → API call (api.js request())
      → Backend responds
        → store.set({ ... }) updates state
          → Component subscriptions re-render DOM

SSE event from /api/sse
  → connectSSE callback
    → store.set({ lastEvent: message })
      → App checks event type, re-renders jobs view if needed
```
