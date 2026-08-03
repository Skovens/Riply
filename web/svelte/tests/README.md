# Riply Web E2E Tests

This directory contains End-to-End (E2E) tests for the Riply web application using [Playwright](https://playwright.dev/).

## Quick Start

### Prerequisites
- Node.js 18+
- npm or yarn

### Installation

```bash
cd web/svelte
npm install
npx playwright install
```

### Running Tests

```bash
# Run all tests
npm test

# Run tests with UI mode (interactive)
npm run test:ui

# Run tests in debug mode
npm run test:debug

# Run tests for a specific browser
npx playwright test --project=chromium
```

## Test Structure

```
tests/
├── home.spec.ts        # Home page tests
├── files.spec.ts       # Files page tests
├── jobs.spec.ts        # Jobs page tests
├── review.spec.ts      # Review page tests
├── ripper.spec.ts      # Ripper page tests
├── settings.spec.ts    # Settings page tests
└── navigation.spec.ts  # Navigation and responsive tests
```

## Writing New Tests

1. Create a new `.spec.ts` file in the `tests/` directory
2. Use the `test` and `expect` imports from `@playwright/test`
3. Follow the pattern of existing tests
4. Run tests locally before committing

### Example Test

```typescript
import { test, expect } from '@playwright/test';

test.describe('My Feature', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/my-feature');
  });

  test('should display correctly', async ({ page }) => {
    await expect(page.locator('.my-feature')).toBeVisible();
  });
});
```

## CI Integration

Tests run automatically on GitHub Actions for every push and pull request. See `.github/workflows/ci.yml` for configuration.

## Troubleshooting

### Tests failing on CI but passing locally
- Check if the app requires authentication
- Verify the dev server starts correctly
- Check for timing issues (add appropriate waits)

### Slow tests
- Use `test.setTimeout()` to increase timeout for slow operations
- Check if the dev server is starting properly in `playwright.config.ts`

### Browser issues
- Run with `--debug` flag for interactive debugging
- Check screenshots and videos in the test report on failure
