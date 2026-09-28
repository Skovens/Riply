// @ts-check
const { test, expect } = require('@playwright/test');

test.describe('Riply Ripper Page', () => {
  test.beforeEach(async ({ page }) => {
    // Mock config API to return non-empty paths (skip first-run modal)
    await page.route('**/v1/config', async (route) => {
      if (route.request().method() === 'POST') {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ success: true, data: {} }),
        });
      } else {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            success: true,
            data: {
              scan_directory: '/mnt/media',
              movies_directory: '/mnt/media/movies',
              tv_directory: '/mnt/media/tv',
            },
          }),
        });
      }
    });

    // Mock drives API
    await page.route('**/v1/ripper/drives', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          success: true,
          data: [
            {
              index: 0,
              drive: '/dev/sr0',
              state: 'Ready',
              disc_name: 'Speed (1994)',
              disc_title: 'Speed',
            },
          ],
        }),
      });
    });

    // Mock autorip status
    await page.route('**/v1/ripper/autorip/status', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true, data: [] }),
      });
    });

    // Mock rip status
    await page.route('**/v1/ripper/status', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true, data: { installed: true, method: 'native' } }),
      });
    });

    // Mock rename API
    await page.route('**/v1/ripper/rename/**', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true, data: {} }),
      });
    });

    // Mock poster API
    await page.route('**/v1/ripper/poster/**', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true, data: { poster_url: null } }),
      });
    });

    // Mock SSE endpoint (empty stream by default - tests can override)
    await page.route('**/v1/sse', async (route) => {
      const sseStream = new ReadableStream({
        start(controller) {
          setTimeout(() => controller.close(), 10000);
        },
      });
      await route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        body: sseStream,
      });
    });

    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');
  });

  test('ripper page loads with drives heading', async ({ page }) => {
    await expect(page.locator('h2:has-text("Drives")')).toBeVisible();
  });

  test('ripper page shows drive card when drives are available', async ({ page }) => {
    await expect(page.locator('.drive-card')).toBeVisible();
  });

  test('unfinished titles section is hidden when no unfinished titles exist', async ({ page }) => {
    // With no naming events, unfinished titles section should not appear
    await expect(page.locator('.ripper__unfinished')).not.toBeVisible();
  });

  test('unfinished titles section appears when naming fails', async ({ page }) => {
    // Route SSE to send naming_failed event
    await page.route('**/v1/sse', async (route) => {
      const sseStream = new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('event: status\ndata: {"kind":"rip","action":"naming_failed","index":0,"data":{}}\n\n'));
          setTimeout(() => controller.close(), 5000);
        },
      });
      await route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        body: sseStream,
      });
    });

    // Reload to pick up new SSE mock
    await page.reload();
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(500);

    // The unfinished titles section should appear
    await expect(page.locator('.ripper__unfinished')).toBeVisible();
  });

  test('unfinished titles shows each item with disc name and status', async ({ page }) => {
    // Route SSE to send naming_failed events for multiple drives
    await page.route('**/v1/sse', async (route) => {
      const sseStream = new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('event: status\ndata: {"kind":"rip","action":"naming_failed","index":0,"data":{}}\n\n'));
          controller.enqueue(new TextEncoder().encode('event: status\ndata: {"kind":"rip","action":"naming_failed","index":1,"data":{}}\n\n'));
          setTimeout(() => controller.close(), 5000);
        },
      });
      await route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        body: sseStream,
      });
    });

    await page.reload();
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(500);

    // Verify section is visible with count
    await expect(page.locator('.ripper__unfinished')).toBeVisible();
    await expect(page.locator('.ripper__unfinished-title:has-text("Unfinished Titles (2)")')).toBeVisible();

    // Verify items are visible
    const items = page.locator('.unfinished-item');
    await expect(items).toHaveCount(2);

    // Check first item content
    await expect(page.locator('.unfinished-item__name:has-text("Speed (1994)")')).toBeVisible();
    await expect(page.locator('.unfinished-item__suggested:has-text("Media naming failed")')).toBeVisible();
    await expect(page.locator('.unfinished-item__status:has-text("failed")')).toBeVisible();
  });

  test('clicking unfinished item opens naming modal', async ({ page }) => {
    // Route SSE to send naming_failed event
    await page.route('**/v1/sse', async (route) => {
      const sseStream = new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('event: status\ndata: {"kind":"rip","action":"naming_failed","index":0,"data":{}}\n\n'));
          setTimeout(() => controller.close(), 5000);
        },
      });
      await route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        body: sseStream,
      });
    });

    await page.reload();
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(500);

    // Click the unfinished item
    await page.locator('.unfinished-item').first().click();

    // Wait for naming modal to appear
    await expect(page.locator('.media-naming-modal')).toBeVisible({ timeout: 3000 });
  });

  test('unfinished titles section is hidden when naming succeeds', async ({ page }) => {
    // Route SSE to send successful naming event
    await page.route('**/v1/sse', async (route) => {
      const sseStream = new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode('event: status\ndata: {"kind":"rip","action":"named","index":0,"data":{"title":"Speed","year":1994}}\n\n'));
          setTimeout(() => controller.close(), 5000);
        },
      });
      await route.fulfill({
        status: 200,
        contentType: 'text/event-stream',
        body: sseStream,
      });
    });

    await page.reload();
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(500);

    // The unfinished titles section should not appear for successful naming
    await expect(page.locator('.ripper__unfinished')).not.toBeVisible();
  });

  test('drive card shows disc name', async ({ page }) => {
    await expect(page.locator('.drive-card__disc-name:has-text("Speed (1994)")')).toBeVisible();
  });

  test('drive card shows drive state', async ({ page }) => {
    await expect(page.locator('.drive-card__state:has-text("Ready")')).toBeVisible();
  });

  test('navigation to organizer page works from ripper', async ({ page }) => {
    await page.click('nav a[href="/organizer"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Organizer")')).toBeVisible();
  });

  test('navigation to settings page works from ripper', async ({ page }) => {
    await page.click('nav a[href="/settings"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Settings")')).toBeVisible();
  });
});
