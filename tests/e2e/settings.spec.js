// @ts-check
const { test, expect } = require('@playwright/test');

test.describe('Riply Settings Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
    await page.waitForLoadState('networkidle');
  });

  test('settings page loads', async ({ page }) => {
    // Verify settings page is accessible
    await expect(page.locator('h2:has-text("Settings")')).toBeVisible();
  });

  test('settings has scan directory field', async ({ page }) => {
    const scanDir = page.locator('input').first();
    await expect(scanDir).toBeVisible();
  });

  test('settings has movies directory field', async ({ page }) => {
    const moviesDir = page.locator('input').nth(1);
    await expect(moviesDir).toBeVisible();
  });

  test('settings has TV directory field', async ({ page }) => {
    const tvDir = page.locator('input').nth(2);
    await expect(tvDir).toBeVisible();
  });

  test('settings has TMDb API key field', async ({ page }) => {
    // TMDb API key is in the metadata APIs section
    await expect(page.locator('text=TMDb API Key')).toBeVisible();
  });

  test('settings has save button', async ({ page }) => {
    await expect(page.locator('button:has-text("Save Settings")')).toBeVisible();
  });

  test('settings has reset button', async ({ page }) => {
    await expect(page.locator('button:has-text("Reset to Defaults")')).toBeVisible();
  });

  test('can fill in settings form', async ({ page }) => {
    const scanDir = page.locator('input').first();
    await scanDir.fill('/mnt/media/test');
    await expect(scanDir).toHaveValue('/mnt/media/test');
  });

  test('navigation to dashboard works', async ({ page }) => {
    await page.click('a[href="/"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h1.header__title')).toBeVisible();
  });

  test('navigation to files works', async ({ page }) => {
    await page.click('a[href="/files"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Files")')).toBeVisible();
  });
});
