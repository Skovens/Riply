import { test, expect } from '@playwright/test';

test.describe('Review Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/review');
  });

  test('should navigate to review page', async ({ page }) => {
    await expect(page).toHaveURL('/review');
  });

  test('should display review content', async ({ page }) => {
    // Check that the review page loads with content
    const reviewPage = page.locator('main');
    await expect(reviewPage).toBeVisible();
  });

  test('should have review UI elements', async ({ page }) => {
    // Look for common review UI elements
    const reviewList = page.locator('[class*="review"]').first();
    const reviewCard = page.locator('[class*="card"]').first();
    const reviewTable = page.locator('table').first();
    
    // At least one review display should exist
    await expect(reviewList.or(reviewCard).or(reviewTable)).toBeVisible();
  });
});
