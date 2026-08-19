// @ts-check
const { test, expect } = require('@playwright/test');

test.describe('Riply Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('dashboard loads with header', async ({ page }) => {
    // Verify header exists using specific selector
    await expect(page.locator('h1.header__title')).toBeVisible();
    await expect(page.locator('h1.header__title')).toContainText('Riply');
  });

  test('dashboard has navigation links', async ({ page }) => {
    // Verify navigation links exist - use specific selectors
    await expect(page.locator('a[href="/"]')).toBeVisible();
    // Use specific class to avoid strict mode violation
    await expect(page.locator('.header__nav-link[href="/files"]')).toBeVisible();
    await expect(page.locator('.header__nav-link[href="/settings"]')).toBeVisible();
  });

  test('dashboard has sidebar panels', async ({ page }) => {
    // Verify sidebar panels exist (drives heading, review/jobs buttons)
    await expect(page.locator('h4:has-text("Drives")')).toBeVisible();
    await expect(page.locator('button:has-text("Review")')).toBeVisible();
    await expect(page.locator('button:has-text("Jobs")')).toBeVisible();
  });

  test('dashboard has action buttons', async ({ page }) => {
    // Verify action buttons exist - use specific selectors
    await expect(page.locator('header button:has-text("Scan")')).toBeVisible();
    await expect(page.locator('header button:has-text("Move")')).toBeVisible();
  });

  test('dashboard files section shows', async ({ page }) => {
    // Verify files section exists
    await expect(page.locator('h3:has-text("Files")')).toBeVisible();
    await expect(page.locator('a:has-text("View All")')).toBeVisible();
  });

  test('dashboard shows stats bar', async ({ page }) => {
    // Verify stats bar exists
    await expect(page.locator('.stats-bar')).toBeVisible();
  });

  test('navigation to files page works', async ({ page }) => {
    await page.click('nav a[href="/files"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Files")')).toBeVisible();
  });

  test('navigation to settings page works', async ({ page }) => {
    await page.click('nav a[href="/settings"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Settings")')).toBeVisible();
  });
});
