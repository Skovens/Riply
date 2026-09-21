// @ts-check
const { test, expect } = require('@playwright/test');

test.describe('Riply First-Run Modal', () => {
  test.beforeEach(async ({ page }) => {
    // Mock config API to return empty paths (first-run state)
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
              scan_directory: '',
              movies_directory: '',
              tv_directory: '',
            },
          }),
        });
      }
    });

    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');
    // Wait for first-run modal to appear
    await expect(page.locator('.first-run-modal')).toBeVisible({ timeout: 5000 });
  });

  test('first-run modal shows with correct title', async ({ page }) => {
    await expect(page.locator('.modal__title:has-text("Welcome to Riply")')).toBeVisible();
  });

  test('first-run modal has all three directory fields', async ({ page }) => {
    await expect(page.getByLabel('Scan Directory')).toBeVisible();
    await expect(page.getByLabel('Movies Directory')).toBeVisible();
    await expect(page.getByLabel('TV Directory')).toBeVisible();
  });

  test('first-run modal has skip and save buttons', async ({ page }) => {
    await expect(page.locator('button:has-text("Skip for now")')).toBeVisible();
    await expect(page.locator('button:has-text("Save & Continue")')).toBeVisible();
  });

  test('skip for now dismisses the modal', async ({ page }) => {
    await page.locator('button:has-text("Skip for now")').click();
    await expect(page.locator('.first-run-modal')).not.toBeVisible();
  });

  test('save with empty fields shows validation error', async ({ page }) => {
    await page.locator('button:has-text("Save & Continue")').click();
    await expect(page.locator('.banner__text:has-text("All three paths are required")')).toBeVisible();
  });

  test('save with valid paths updates config and dismisses modal', async ({ page }) => {
    await page.getByLabel('Scan Directory').fill('/mnt/media/scan');
    await page.getByLabel('Movies Directory').fill('/mnt/media/movies');
    await page.getByLabel('TV Directory').fill('/mnt/media/tv');

    const responsePromise = page.waitForResponse(
      (response) => response.url().includes('/v1/config') && response.request().method() === 'POST'
    );

    await page.locator('button:has-text("Save & Continue")').click();
    await expect(page.locator('.first-run-modal')).not.toBeVisible();

    const response = await responsePromise;
    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body.data.scan_directory).toBe('/mnt/media/scan');
    expect(body.data.movies_directory).toBe('/mnt/media/movies');
    expect(body.data.tv_directory).toBe('/mnt/media/tv');
  });

  test('modal closes when clicking overlay', async ({ page }) => {
    await page.locator('.modal-overlay').click({ position: { x: 0, y: 0 } });
    await expect(page.locator('.first-run-modal')).not.toBeVisible();
  });
});
