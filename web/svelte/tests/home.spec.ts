import { test, expect } from '@playwright/test';

test.describe('Home Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should navigate to home page', async ({ page }) => {
    await expect(page).toHaveTitle(/Riply/);
  });

  test('should have navigation links', async ({ page }) => {
    // Check for main navigation elements
    const nav = page.locator('nav');
    await expect(nav).toBeVisible();
    
    // Check for key navigation links
    const filesLink = page.getByRole('link', { name: /files/i });
    const jobsLink = page.getByRole('link', { name: /jobs/i });
    const reviewLink = page.getByRole('link', { name: /review/i });
    
    await expect(filesLink).toHaveAttribute('href', '/files');
    await expect(jobsLink).toHaveAttribute('href', '/jobs');
    await expect(reviewLink).toHaveAttribute('href', '/review');
  });

  test('should display main content', async ({ page }) => {
    // Check that the page has meaningful content
    const mainContent = page.locator('main');
    await expect(mainContent).toBeVisible();
  });
});
