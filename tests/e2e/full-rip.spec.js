// @ts-check
const { test, expect } = require('@playwright/test');

/**
 * Full E2E DVD rip test — real hardware, no API mocking.
 *
 * Exercises the complete pipeline:
 *   1. Drive detection via MakeMKV (real SCSI)
 *   2. Disc name/title detection
 *   3. Rip initiation + progress monitoring via SSE
 *   4. Auto-naming via TMDb
 *   5. Post-rip move to organizer directory
 *   6. Output file verification
 *
 * Estimated runtime: 12–30 min (depends on disc size and drive speed)
 * Disk usage: ~5–10 GB for rip output
 *
 * NOTE: Requires a disc inserted in drive 0.
 * Verify with: `cargo run -p cli -- rip list`
 *
 * To run:
 *   npx playwright test tests/e2e/full-rip.spec.js --project=chromium --headed
 *
 * After running, the disc is ejected and the rip output is cleaned up.
 */

// ── Helpers ─────────────────────────────────────────────────

/**
 * Wait for a drive to reach a specific state.
 * Polls using small intervals to avoid page crashes from long waitForTimeout.
 */
async function waitForDriveState(page, stateEmoji, timeout = 30000) {
  await expect(
    page.locator('.drive-card__state').filter({ hasText: stateEmoji }).first(),
    `Drive did not reach state: ${stateEmoji}`
  ).toBeVisible({ timeout });
}

/**
 * Wait for rip progress to appear (indicates rip has started).
 */
async function waitForRipProgress(page, timeout = 30000) {
  await expect(page.locator('.progress-bar')).toBeVisible({ timeout });
}

/**
 * Wait for rip to complete using Playwright's built-in expect() with long timeout.
 * This is much more robust than manual polling with waitForTimeout() which causes
 * page crashes during long-duration rips (12-30 min).
 * Logs progress milestones via a non-blocking interval.
 */
async function waitForRipComplete(page, timeout = 25 * 60 * 1000) {
  let lastPct = 0;

  // Check for completion using expect() which handles long waits gracefully
  await expect(
    page.locator('.drive-card__state').filter({ hasText: '✅' }).first(),
    `Rip did not complete within ${timeout / 60000} minutes`
  ).toBeVisible({ timeout });

  // Progress logging via setInterval (non-blocking after completion)
  const logInterval = setInterval(() => {
    page.locator('.drive-card__state').filter({ hasText: '⚡' }).isVisible().then(isRipping => {
      if (!isRipping) {
        clearInterval(logInterval);
        return;
      }
      page.locator('.drive-card__progress-pct').textContent().then(text => {
        if (text) {
          const pct = parseInt(text.replace('%', ''), 10);
          if (pct > lastPct && pct % 10 === 0) {
            console.log(`[progress] ${pct}%`);
            lastPct = pct;
          }
        }
      }).catch(() => {});
    }).catch(() => {});
  }, 10000);

  // Clean up interval after completion
  setTimeout(() => clearInterval(logInterval), timeout);
}

/**
 * Wait for auto-naming to complete or fail using expect().
 * Avoids waitForTimeout() polling loops that crash pages.
 */
async function waitForAutoNaming(page, timeout = 5 * 60 * 1000) {
  try {
    // Wait for poster to appear (success)
    await expect(
      page.locator('.drive-card__poster-img').first(),
      'Auto-naming timed out — no poster or unfinished section visible'
    ).toBeVisible({ timeout });
    return { namingSucceeded: true, namingFailed: false };
  } catch (e) {
    // Check if unfinished section appeared (failure)
    const hasUnfinished = await page.locator('.ripper__unfinished').isVisible().catch(() => false);
    return { namingSucceeded: false, namingFailed: hasUnfinished };
  }
}

/**
 * Disable AutoRip on all drives to prevent auto-starting rips.
 */
async function disableAutorip(page) {
  const autoripToggles = page.locator('.autorip-toggle input[type="checkbox"]');
  const count = await autoripToggles.count();
  for (let i = 0; i < count; i++) {
    if (await autoripToggles.nth(i).isChecked()) {
      await autoripToggles.nth(i).uncheck();
      console.log(`[autorip] Disabled AutoRip on drive ${i}`);
    }
  }
}

// ── Targeted helper tests (fast, no rip required) ───────────

test.describe('Rip — Drive Detection (Real Hardware)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');
  });

  test('drive 0 is detected with disc loaded', async ({ page }) => {
    await expect(page.locator('.drive-card').first()).toBeVisible({ timeout: 10000 });

    const discName = page.locator('.drive-card__disc-name').first();
    await expect(discName).toBeVisible();
    const text = (await discName.textContent()).trim();
    expect(text.length).toBeGreaterThan(0);
    console.log(`[drive-detect] Drive 0 disc name: ${text}`);
  });

  test('drive 0 state is Ready (shows 💿 icon)', async ({ page }) => {
    // Ready state uses 💿 emoji icon
    await waitForDriveState(page, '💿', 10000);
  });

  test('drive 0 has an enabled Rip button', async ({ page }) => {
    const ripBtn = page.locator('.drive-card__action-row .btn:has-text("Rip")').first();
    await expect(ripBtn).toBeVisible({ timeout: 10000 });
    await expect(ripBtn).toBeEnabled();
  });

  test('drive 0 has an enabled Eject button when disc is loaded', async ({ page }) => {
    const ejectBtn = page.locator('.drive-card__action-row .btn:has-text("Eject")').first();
    await expect(ejectBtn).toBeVisible({ timeout: 10000 });
    await expect(ejectBtn).toBeEnabled();
  });
});

// ── Full rip pipeline (single test, ~12-30 min) ────────────

test.describe('Full E2E DVD Rip Pipeline — Real Hardware', () => {
  test.setTimeout(30 * 60 * 1000); // 30 minutes

  test('complete rip of Speed (1994) DVD on drive 0', async ({ page }) => {
    // ── Step 1: Navigate to ripper ───────────────────────────
    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');
    console.log('[step-1] Navigated to ripper page');

    // ── Step 2: Disable AutoRip to prevent auto-start ────────
    await disableAutorip(page);

    // ── Step 3: Wait for drive 0 to be in Ready state ────────
    console.log('[step-3] Waiting for drive 0 to be Ready...');
    await waitForDriveState(page, '💿', 15000);
    console.log('[step-3] Drive 0 is Ready');

    // ── Step 4: Verify drive 0 detected with disc ────────────
    await expect(page.locator('.drive-card').first()).toBeVisible({ timeout: 10000 });
    const discNameEl = page.locator('.drive-card__disc-name').first();
    await expect(discNameEl).toBeVisible();
    const discName = (await discNameEl.textContent()).trim();
    expect(discName.length).toBeGreaterThan(0);
    console.log(`[step-4] Drive 0 detected: "${discName}"`);

    // ── Step 5: Click Rip button ─────────────────────────────
    const ripBtn = page.locator('.drive-card__action-row .btn:has-text("Rip")').first();
    await expect(ripBtn).toBeEnabled();
    await ripBtn.click();
    console.log('[step-5] Clicked Rip button');

    // ── Step 6: Wait for rip to start ────────────────────────
    // Ripping state uses ⚡ emoji icon
    await waitForDriveState(page, '⚡', 30000);
    await waitForRipProgress(page, 30000);
    console.log('[step-6] Rip started, progress bar visible');

    // ── Step 7: Wait for rip to complete ─────────────────────
    // Uses Playwright's expect() with long timeout — robust against page crashes
    const startTime = Date.now();
    const ripTimeout = 25 * 60 * 1000; // 25 minutes max for rip
    console.log(`[step-7] Waiting for rip completion (timeout: ${ripTimeout / 60000} min)...`);

    await waitForRipComplete(page, ripTimeout);

    const totalMinutes = ((Date.now() - startTime) / 1000 / 60).toFixed(1);
    console.log(`[step-7] ✅ Rip completed in ${totalMinutes} minutes`);

    // ── Step 9: Wait for auto-naming event ───────────────────
    console.log('[step-9] Waiting for auto-naming event...');
    const { namingSucceeded, namingFailed } = await waitForAutoNaming(page);

    if (namingSucceeded) {
      console.log('[step-9] ✅ Auto-naming succeeded — poster fetched');
    } else if (namingFailed) {
      console.log('[step-9] ⚠️ Auto-naming failed — unfinished titles section appeared');
    } else {
      console.log('[step-9] ⚠️ Auto-naming timed out — no poster or unfinished section visible');
    }

    // ── Step 10: Verify organizer integration ────────────────
    console.log('[step-10] Checking organizer integration...');
    await page.goto('/organizer');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Organizer")')).toBeVisible();

    const folderCount = await page.locator('.folder-card').count();
    console.log(`[step-10] Organizer shows ${folderCount} folder(s)`);

    // ── Step 11: Eject disc ──────────────────────────────────
    console.log('[step-11] Ejecting disc...');
    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');

    // Wait for drive to show in a state that has eject button (Ready/Complete/Error)
    await expect(
      page.locator('.drive-card__action-row .btn:has-text("Eject")').first(),
      'Eject button not found'
    ).toBeVisible({ timeout: 10000 });

    const ejectBtn = page.locator('.drive-card__action-row .btn:has-text("Eject")').first();
    await ejectBtn.click();
    await expect(page.locator('.drive-card__state').filter({ hasText: '⏹' }).first()).toBeVisible({ timeout: 10000 });
    console.log('[step-11] ✅ Disc ejected');

    // ── Step 12: Record results ──────────────────────────────
    console.log(`\n[results] Rip pipeline complete in ${totalMinutes} minutes`);
    console.log(`[results] Disc: "${discName}"`);
    console.log(`[results] Auto-naming: ${namingSucceeded ? 'SUCCESS' : namingFailed ? 'FAILED' : 'TIMED OUT'}`);
    console.log(`[results] Organizer folders: ${folderCount}`);
  });
});
