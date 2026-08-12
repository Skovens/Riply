// @ts-check
const { test, expect } = require('@playwright/test');

test.describe('Riply Files Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/files');
    await page.waitForLoadState('networkidle');
  });

  test('files page loads with title', async ({ page }) => {
    const title = page.locator('h2:has-text("Files")');
    await expect(title).toBeVisible();
  });

  test('files page has search input', async ({ page }) => {
    const searchInput = page.locator('input[placeholder="Search files..."]');
    await expect(searchInput).toBeVisible();
  });

  test('files page has status filter', async ({ page }) => {
    const statusFilter = page.locator('select');
    await expect(statusFilter).toBeVisible();
    
    // Check that filter has options by selecting and checking values
    await statusFilter.selectOption({ label: 'All statuses' });
    await expect(statusFilter).toHaveValue('');
    
    await statusFilter.selectOption({ label: 'Scanned' });
    await expect(statusFilter).toHaveValue('Scanned');
    
    await statusFilter.selectOption({ label: 'Needs Review' });
    await expect(statusFilter).toHaveValue('NeedsReview');
  });

  test('files page has action buttons', async ({ page }) => {
    await expect(page.locator('button:has-text("Move Preview")')).toBeVisible();
    await expect(page.locator('button:has-text("Analyze")')).toBeVisible();
    // Use specific selector to avoid strict mode violation
    await expect(page.locator('#main-content button:has-text("Scan")')).toBeVisible();
    await expect(page.locator('button:has-text("Reset DB")')).toBeVisible();
  });

  test('navigation back to dashboard works', async ({ page }) => {
    await page.click('a[href="/"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h1.header__title')).toBeVisible();
  });

  test('navigation to settings from files works', async ({ page }) => {
    await page.click('a[href="/settings"]');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Settings")')).toBeVisible();
  });
});
