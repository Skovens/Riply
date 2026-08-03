import { test, expect } from '@playwright/test';

test.describe('Navigation Flow', () => {
  test('should navigate between all main pages', async ({ page }) => {
    // Start at home
    await page.goto('/');
    await expect(page).toHaveTitle(/Riply/);

    // Navigate to Files
    await page.getByRole('link', { name: /files/i }).click();
    await expect(page).toHaveURL('/files');

    // Navigate to Jobs
    await page.getByRole('link', { name: /jobs/i }).click();
    await expect(page).toHaveURL('/jobs');

    // Navigate to Review
    await page.getByRole('link', { name: /review/i }).click();
    await expect(page).toHaveURL('/review');

    // Navigate to Ripper
    await page.getByRole('link', { name: /ripper/i }).click();
    await expect(page).toHaveURL('/ripper');

    // Navigate to Settings
    await page.getByRole('link', { name: /settings/i }).click();
    await expect(page).toHaveURL('/settings');

    // Navigate back to home
    await page.getByRole('link', { name: /Riply/i }).first().click();
    await expect(page).toHaveURL('/');
  });

  test('should maintain navigation on page refresh', async ({ page }) => {
    await page.goto('/files');
    await page.reload();
    await expect(page).toHaveURL('/files');
  });
});

test.describe('Responsive Design', () => {
  test('should work on mobile viewport', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');
    await expect(page.locator('nav')).toBeVisible();
  });

  test('should work on tablet viewport', async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 });
    await page.goto('/');
    await expect(page.locator('nav')).toBeVisible();
  });
});

test.describe('Error Handling', () => {
  test('should handle 404 page', async ({ page }) => {
    await page.goto('/nonexistent-page');
    // Should not crash, should show some error or 404 content
    await expect(page.locator('main')).toBeVisible();
  });
});
