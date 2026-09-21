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
    // Simulate a naming_failed SSE event
    await page.evaluate(() => {
      // Trigger SSE event handler directly
      const event = {
        kind: 'rip',
        action: 'naming_failed',
        index: 0,
        data: {},
      };
      // Dispatch custom event that the ripper page listens to
      window.dispatchEvent(new CustomEvent('sse-event', { detail: event }));
    });

    // The unfinished titles section should appear
    await expect(page.locator('.ripper__unfinished')).toBeVisible({ timeout: 3000 });
  });

  test('unfinished titles section is hidden when naming succeeds', async ({ page }) => {
    // Simulate a successful naming SSE event
    await page.evaluate(() => {
      const event = {
        kind: 'rip',
        action: 'named',
        index: 0,
        data: { title: 'Speed', year: 1994 },
      };
      window.dispatchEvent(new CustomEvent('sse-event', { detail: event }));
    });

    // The unfinished titles section should not appear for successful naming
    await expect(page.locator('.ripper__unfinished')).not.toBeVisible({ timeout: 3000 });
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
