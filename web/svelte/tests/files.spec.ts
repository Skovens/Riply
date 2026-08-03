import { test, expect } from '@playwright/test';

test.describe('Files Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/files');
  });

  test('should navigate to files page', async ({ page }) => {
    await expect(page).toHaveURL('/files');
  });

  test('should display files content', async ({ page }) => {
    // Check that the files page loads with content
    const filesPage = page.locator('main');
    await expect(filesPage).toBeVisible();
  });

  test('should have file management UI elements', async ({ page }) => {
    // Look for common file management elements
    const fileTable = page.locator('table').first();
    const fileGrid = page.locator('[class*="grid"]').first();
    
    // At least one file display should exist
    await expect(fileTable.or(fileGrid)).toBeVisible();
  });
});
