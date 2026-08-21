// @ts-check
const { test, expect } = require('@playwright/test');

test.describe('Riply — Root Redirect', () => {
  test('root redirects to /ripper', async ({ page }) => {
    await page.goto('/');
    // Wait for redirect to complete
    await page.waitForURL(/\/ripper/);
    // Verify we're on the ripper page
    await expect(page.locator('h2:has-text("Drives")')).toBeVisible();
  });
});

test.describe('Riply Ripper Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');
  });

  test('ripper page loads with drives heading', async ({ page }) => {
    await expect(page.locator('h2:has-text("Drives")')).toBeVisible();
  });

  test('ripper page has navigation links', async ({ page }) => {
    // Verify navigation links exist - use specific selectors
    await expect(page.locator('a[href="/ripper"]')).toBeVisible();
    await expect(page.locator('a[href="/organizer"]')).toBeVisible();
    await expect(page.locator('a[href="/settings"]')).toBeVisible();
    // Files link should NOT exist (removed)
    await expect(page.locator('a[href="/files"]')).not.toBeVisible();
  });

  test('navigation to settings page works', async ({ page }) => {
    await page.click('nav a[href="/settings"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Settings")')).toBeVisible();
  });

  test('navigation to organizer page works', async ({ page }) => {
    await page.click('nav a[href="/organizer"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Organizer")')).toBeVisible();
  });
});
