import { test, expect } from '@playwright/test';

test.describe('Jobs Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/jobs');
  });

  test('should navigate to jobs page', async ({ page }) => {
    await expect(page).toHaveURL('/jobs');
  });

  test('should display jobs content', async ({ page }) => {
    // Check that the jobs page loads with content
    const jobsPage = page.locator('main');
    await expect(jobsPage).toBeVisible();
  });

  test('should have job listing or status display', async ({ page }) => {
    // Look for common job/status UI elements
    const jobList = page.locator('[class*="list"]').first();
    const jobTable = page.locator('table').first();
    const statusDisplay = page.locator('[class*="status"]').first();
    
    // At least one job display should exist
    await expect(jobList.or(jobTable).or(statusDisplay)).toBeVisible();
  });
});
