import { test, expect } from '@playwright/test';

test.describe('Ripper Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ripper');
  });

  test('should navigate to ripper page', async ({ page }) => {
    await expect(page).toHaveURL('/ripper');
  });

  test('should display ripper content', async ({ page }) => {
    // Check that the ripper page loads with content
    const ripperPage = page.locator('main');
    await expect(ripperPage).toBeVisible();
  });

  test('should have ripper controls', async ({ page }) => {
    // Look for common ripper UI elements like buttons, forms, or controls
    const ripperControls = page.locator('button').first();
    const ripperForm = page.locator('form').first();
    const ripperSection = page.locator('[class*="ripper"]').first();
    
    // At least one ripper element should exist
    await expect(ripperControls.or(ripperForm).or(ripperSection)).toBeVisible();
  });
});
