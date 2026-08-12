# Playwright MCP Skill — Riply Web UI

## Overview

This skill documents how to use **Playwright MCP tools** to interact with, test, and verify the Riply SvelteKit frontend. It covers browser setup, common interaction patterns, and a reusable test checklist.

The Riply web app is a SvelteKit SPA served from the Axum backend (`riply serve`) at `http://localhost:3000`. The frontend uses client-side routing with hash-based paths (`/#/files`, `/#/settings`). API calls go to `/v1/*` endpoints (proxied in dev via Vite to `localhost:3000`).

---

## Prerequisites

### 1. Browser Setup

The environment has Chromium available at:

```
/usr/lib/chromium/chromium
```

A symlink exists at `/home/openhands/google/chrome/chrome` pointing to the same binary.

**Install Playwright browsers:**

```bash
npx playwright install chromium
npx playwright install-deps chromium
```

If `npx playwright install chromium` fails to find the system binary, point it explicitly:

```bash
PLAYWRIGHT_BROWSERS_PATH=$HOME/.cache/ms-playwright npx playwright install chromium
```

Or set the `PLAYWRIGHT_CHROMIUM_PATH` env var before launching:

```bash
export PLAYWRIGHT_CHROMIUM_PATH=/usr/lib/chromium/chromium
```

**Verify the browser works:**

```bash
npx playwright test --list   # should parse config without errors
node -e "require('playwright').chromium.launch({ executablePath: '/usr/lib/chromium/chromium' })"
```

### 2. App Running

The Axum backend must be running before testing:

```bash
# Terminal 1: Start the server
cargo run -p cli -- serve --host 127.0.0.1 --port 3000
```

The server should respond to health checks:

```bash
curl -s http://localhost:3000/health
# Expected: "OK"
```

For the SvelteKit dev server (with API proxy):

```bash
# Terminal 2: Frontend dev server
cd web/svelte && npm run dev
# SvelteKit dev at http://localhost:5173, proxying /v1/* → localhost:3000
```

**Test URLs:**

| Environment | Base URL | Notes |
|-------------|----------|-------|
| Production (Axum) | `http://localhost:3000` | Static files embedded in binary |
| Dev (Vite proxy) | `http://localhost:5173` | Proxies `/v1/*` → `localhost:3000` |

---

## Common Patterns

### Pattern 1: Navigate to a Page

```typescript
import { chromium } from 'playwright';

const browser = await chromium.launch({
  executablePath: '/usr/lib/chromium/chromium',
});
const page = await browser.newPage();

// Hash-based routing — navigate to the base URL, then click nav links
await page.goto('http://localhost:3000');

// Click a nav link to reach /files
await page.click('a.header__nav-link:has-text("Files")');
await page.waitForURL(/\/files/);

// Or navigate directly via hash
await page.goto('http://localhost:3000/#/files');
```

### Pattern 2: Take a Screenshot

```typescript
// Full-page screenshot
await page.screenshot({ path: 'screenshot.png', fullPage: true });

// Screenshot a specific element
await page.screenshot({
  path: 'dashboard.png',
  target: page.locator('.dashboard-grid'),
});

// Screenshot with label for CI
await page.screenshot({ path: `test-failed-${Date.now()}.png` });
```

### Pattern 3: Interact with Elements

**Click a button:**

```typescript
// By text content
await page.click('button:has-text("Scan")');

// By CSS class
await page.click('.btn--primary');

// With wait for response (verify action triggered an API call)
const response = page.waitForResponse(
  resp => resp.url().includes('/v1/scan') && resp.method() === 'POST'
);
await page.click('button:has-text("Scan")');
await response;
```

**Type into an input:**

```typescript
// The scan prompt uses window.prompt() — Playwright can't intercept native prompts
// Instead, trigger the scan button and handle the prompt:
page.on('dialog', async dialog => {
  await dialog.accept('/mnt/media/movies');
});
await page.click('button:has-text("Scan")');
```

**Fill a form field:**

```typescript
await page.fill('input[name="scan_directory"]', '/mnt/media');
await page.fill('input[type="text"]', 'search term');
```

**Select from a dropdown:**

```typescript
await page.selectOption('select.status-filter', { label: 'Needs Review' });
```

### Pattern 4: Assert Page State

```typescript
// Verify a heading exists
await expect(page.locator('h1:has-text("Riply")')).toBeVisible();

// Verify a table has rows
const rowCount = await page.locator('.file-row').count();
expect(rowCount).toBeGreaterThan(0);

// Verify a badge with a specific status
await expect(page.locator('.badge--warning:has-text("NeedsReview")')).toBeVisible();

// Verify text content
await expect(page.locator('.stats-bar')).toContainText('0');

// Wait for an element to appear (e.g., loading completes)
await page.waitForSelector('.file-row', { state: 'attached', timeout: 10_000 });

// Wait for network idle (all API calls resolved)
await page.waitForLoadState('networkidle');
```

### Pattern 5: Test the Dashboard

```typescript
test('dashboard loads with pipeline stats', async ({ page }) => {
  await page.goto('http://localhost:3000');
  await page.waitForLoadState('networkidle');

  // Verify header
  await expect(page.locator('.header__title')).toHaveText('Riply');

  // Verify navigation
  await expect(page.locator('.header__nav-link')).toHaveCount(3); // Dashboard, Files, Settings

  // Verify stats bar exists
  await expect(page.locator('.stats-bar')).toBeVisible();

  // Verify pipeline bar
  await expect(page.locator('.pipeline-bar')).toBeVisible();

  // Verify sidebar panels exist
  await expect(page.locator('.sidebar-panel')).toHaveCount(3); // drives, review, jobs
});
```

### Pattern 6: Test File List Page

```typescript
test('file list page shows files with status badges', async ({ page }) => {
  await page.goto('http://localhost:3000/#/files');
  await page.waitForLoadState('networkidle');

  // Verify table header
  await expect(page.locator('.panel-title:has-text("Files")')).toBeVisible();

  // Verify pagination controls
  await expect(page.locator('.pagination')).toBeVisible();

  // Click a file row to open detail drawer
  await page.click('.file-row');
  await expect(page.locator('.detail-drawer')).toBeVisible();
});
```

### Pattern 7: Test SSE Events

The dashboard subscribes to SSE at `/v1/sse`. To verify SSE-driven updates:

```typescript
test('SSE updates job status', async ({ page }) => {
  await page.goto('http://localhost:3000');

  // Intercept SSE connection
  const sseMessages: string[] = [];
  page.on('console', msg => {
    if (msg.text().startsWith('SSE:')) sseMessages.push(msg.text());
  });

  // Trigger a scan
  page.on('dialog', async dialog => await dialog.accept('/tmp'));
  await page.click('button:has-text("Scan")');

  // Wait for SSE messages to arrive
  await page.waitForTimeout(3000);

  // Verify job panel updated
  await expect(page.locator('.job-card')).toBeVisible();
});
```

### Pattern 8: Test Settings Page

```typescript
test('settings page loads config form', async ({ page }) => {
  await page.goto('http://localhost:3000/#/settings');
  await page.waitForLoadState('networkidle');

  // Verify form fields
  await expect(page.locator('input[name="scan_directory"]')).toBeVisible();
  await expect(page.locator('input[name="movies_directory"]')).toBeVisible();
  await expect(page.locator('input[name="tmdb_api_key"]')).toBeVisible();

  // Verify save button
  await expect(page.locator('button:has-text("Save")')).toBeVisible();
});
```

### Pattern 9: Test Navigation

```typescript
test('navigation links work', async ({ page }) => {
  await page.goto('http://localhost:3000');

  // Click Files link
  await page.click('a.header__nav-link:has-text("Files")');
  await expect(page.locator('.panel-title:has-text("Files")')).toBeVisible();

  // Click Settings link
  await page.click('a.header__nav-link:has-text("Settings")');
  await expect(page.locator('input[name="scan_directory"]')).toBeVisible();

  // Click Dashboard link
  await page.click('a.header__nav-link:has-text("Dashboard")');
  await expect(page.locator('.stats-bar')).toBeVisible();
});
```

### Pattern 10: Test Error States

```typescript
test('shows error when backend is unreachable', async ({ page }) => {
  // Stop the backend server (in a real test, use a separate process)
  await page.goto('http://localhost:3000');

  // The page should handle connection errors gracefully
  // Look for error toasts or fallback UI
  await page.waitForTimeout(2000);

  // Verify the page doesn't crash
  await expect(page.locator('.app')).toBeVisible();
});
```

---

## Riply-Specific Selectors Reference

| Element | Selector | Notes |
|---------|----------|-------|
| Header title | `.header__title` | "Riply" |
| Nav links | `a.header__nav-link` | Dashboard, Files, Settings |
| Active nav | `.header__nav-link--active` | Current page highlight |
| Stats bar | `.stats-bar` | 5-column grid of counts |
| Pipeline bar | `.pipeline-bar` | Stage pills (Scanned → Moved) |
| File table | `.file-row` | Clickable rows |
| Status badge | `.badge--success`, `.badge--warning`, `.badge--error` | Color-coded |
| Detail drawer | `.detail-drawer` | Opens on file row click |
| Sidebar panels | `.sidebar-panel` | Drives, Review, Jobs |
| Drive card | `.drive-card` | Individual drive info |
| Review item | `.review-item` | NeedsReview file |
| Job card | `.job-card` | Active/finished jobs |
| Toast | `.toast` | Success/error notifications |
| Settings form | `input[name="..."]` | Config fields |
| Scan button | `button:has-text("Scan")` | Dashboard scan trigger |
| Move button | `button:has-text("Move")` | Dashboard move trigger |
| Cancel button | `button:has-text("Cancel")` | Only when job is active |

---

## Troubleshooting

### Browser Not Found

**Symptom:** `Error: Could not find expected browser (chromium) locally`

**Fix:**
```bash
# Install Playwright's bundled Chromium
npx playwright install chromium

# Or verify system Chromium path
ls -la /usr/lib/chromium/chromium
file /usr/lib/chromium/chromium

# If system Chromium exists but Playwright can't find it, launch with explicit path
chromium.launch({ executablePath: '/usr/lib/chromium/chromium' })
```

### Connection Refused

**Symptom:** `Error: connect ECONNREFUSED 127.0.0.1:3000`

**Fix:**
1. Verify the Axum server is running: `curl http://localhost:3000/health`
2. Check the port: `ss -tlnp | grep 3000`
3. Restart the server: `cargo run -p cli -- serve --host 127.0.0.1 --port 3000`

### Page Times Out Loading

**Symptom:** `Timeout 30000ms exceeded` on `page.goto()` or `waitForLoadState()`

**Fix:**
```typescript
// Increase timeout
await page.goto('http://localhost:3000', { timeout: 60_000 });
await page.waitForLoadState('networkidle', { timeout: 30_000 });

// Or use 'domcontentloaded' instead of 'load'
await page.waitForLoadState('domcontentloaded');
```

### Hash Routing Navigation Fails

**Symptom:** Navigating to `/#/files` doesn't update the page

**Fix:** SvelteKit SPA uses hash routing. Navigate to the base URL and click nav links, or use `page.goto()` with the full hash URL:

```typescript
// Option 1: Click nav link (preferred)
await page.goto('http://localhost:3000');
await page.click('a.header__nav-link:has-text("Files")');

// Option 2: Direct hash navigation
await page.goto('http://localhost:3000/#/files');
await page.waitForLoadState('domcontentloaded');
```

### SSE Events Not Appearing

**Symptom:** Job progress or review updates don't appear after triggering an action

**Fix:**
1. Verify SSE endpoint is accessible: `curl -N http://localhost:3000/v1/sse`
2. The SSE channel has a capacity of 256 — rapid events may drop oldest messages
3. Wait for the SSE connection to establish before triggering actions:
   ```typescript
   await page.waitForTimeout(1000); // Allow SSE to connect
   ```

### Screenshots Show Blank Page

**Symptom:** Screenshot is all white/blank

**Fix:**
```typescript
// Wait for fonts and network before screenshot
await page.waitForLoadState('networkidle');
await page.waitForTimeout(500); // Extra buffer for CSS animations

// Ensure the viewport is large enough
await page.setViewportSize({ width: 1280, height: 900 });

// Take a full-page screenshot
await page.screenshot({ path: 'debug.png', fullPage: true });
```

### Native Prompt Can't Be Intercepted

**Symptom:** `window.prompt()` in the dashboard scan flow can't be tested

**Fix:** Use Playwright's `dialog` event handler:

```typescript
page.on('dialog', async dialog => {
  console.log('Prompt message:', dialog.message());
  await dialog.accept('/mnt/media/movies');
});
await page.click('button:has-text("Scan")');
```

---

## Test Checklist

Use this checklist before running Playwright tests against the Riply web UI:

### Pre-Flight

- [ ] **Axum server running** — `curl http://localhost:3000/health` returns `"OK"`
- [ ] **Database initialized** — `riply.db` exists and migrations are applied
- [ ] **Chromium available** — `/usr/lib/chromium/chromium` exists and is executable
- [ ] **Playwright browsers installed** — `npx playwright install chromium` succeeded
- [ ] **Port 3000 free** — no other process bound to the port

### Page-Level Checks

- [ ] **Dashboard loads** — header, stats bar, pipeline bar, sidebar panels all visible
- [ ] **Navigation works** — all nav links (Dashboard, Files, Settings) navigate correctly
- [ ] **Files page loads** — file table renders with pagination
- [ ] **Settings page loads** — config form fields are present and populated
- [ ] **Detail drawer opens** — clicking a file row opens the drawer
- [ ] **Empty states render** — pages show "no data" UI when the database is empty

### Interaction Checks

- [ ] **Scan button works** — triggers prompt, accepts path, shows progress
- [ ] **Move button works** — triggers move job, shows progress
- [ ] **Cancel button works** — appears during active jobs, cancels on click
- [ ] **Accept/Reject review** — review items show action buttons, actions update counts
- [ ] **Drive panel updates** — drive state changes reflect in the UI
- [ ] **Job list updates** — new jobs appear, completed jobs show final status

### SSE Checks

- [ ] **SSE connection establishes** — connection event received on page load
- [ ] **Job progress updates** — progress events update the UI in real-time
- [ ] **Review events** — accept/reject events update the queue
- [ ] **Drive change events** — disc insert/eject updates drive panel
- [ ] **Reconnection** — SSE reconnects after brief server restart

### Visual Checks

- [ ] **Dark mode default** — page uses dark theme
- [ ] **Responsive layout** — sidebar collapses at ≤1024px
- [ ] **Status badge colors** — success (green), warning (amber), error (red)
- [ ] **Loading states** — spinners appear during data fetch
- [ ] **Toast notifications** — success/error toasts appear and auto-dismiss

### Error Handling Checks

- [ ] **Backend unreachable** — page doesn't crash when server is down
- [ ] **API errors** — 4xx/5xx responses handled gracefully
- [ ] **SSE disconnect** — page handles lost connection without crashing
- [ ] **Invalid config** — settings page handles missing config fields

---

## Quick Start Example

```typescript
import { test, expect } from '@playwright/test';

test('Riply dashboard smoke test', async ({ page }) => {
  // 1. Navigate
  await page.goto('http://localhost:3000');
  await page.waitForLoadState('networkidle');

  // 2. Verify header
  await expect(page.locator('.header__title')).toHaveText('Riply');

  // 3. Verify navigation
  const navLinks = page.locator('.header__nav-link');
  await expect(navLinks).toHaveCount(3);

  // 4. Verify stats bar
  await expect(page.locator('.stats-bar')).toBeVisible();

  // 5. Navigate to Files
  await page.click('a.header__nav-link:has-text("Files")');
  await expect(page.locator('.panel-title:has-text("Files")')).toBeVisible();

  // 6. Navigate to Settings
  await page.click('a.header__nav-link:has-text("Settings")');
  await expect(page.locator('input[name="scan_directory"]')).toBeVisible();

  // 7. Screenshot
  await page.goto('http://localhost:3000');
  await page.screenshot({ path: 'dashboard.png', fullPage: true });
});
```

---

## Reuse for Any Svelte/Vite Project

This skill is reusable for any SvelteKit + Vite project by adjusting:

1. **Base URL** — Update `http://localhost:3000` to your dev/prod URL
2. **CSS selectors** — Replace Riply-specific class names with your project's selectors
3. **API endpoints** — Update `/v1/*` paths to your API prefix
4. **Routing** — Replace hash-based routing with path-based routing if applicable
5. **Browser path** — Update `/usr/lib/chromium/chromium` to your system's Chromium location

The patterns (navigate, screenshot, interact, assert, test SSE) are generic and apply to any SPA.
