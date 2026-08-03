import { test, expect } from '@playwright/test';

test.describe('Settings Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings');
  });

  test('should navigate to settings page', async ({ page }) => {
    await expect(page).toHaveURL('/settings');
  });

  test('should display settings content', async ({ page }) => {
    // Check that the settings page loads with content
    const settingsPage = page.locator('main');
    await expect(settingsPage).toBeVisible();
  });

  test('should have settings form elements', async ({ page }) => {
    // Look for common settings UI elements like inputs, selects, or toggles
    const settingsForm = page.locator('form').first();
    const settingsInput = page.locator('input').first();
    const settingsSection = page.locator('[class*="settings"]').first();
    
    // At least one settings element should exist
    await expect(settingsForm.or(settingsInput).or(settingsSection)).toBeVisible();
  });
});
