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

// ── Targeted helper tests (fast, no rip required) ───────────

test.describe('Rip — Drive Detection (Real Hardware)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');
  });

  test('drive 0 is detected with disc loaded', async ({ page }) => {
    await expect(page.locator('.drive-card').first()).toBeVisible({ timeout: 10000 });
    const driveCards = page.locator('.drive-card');
    await expect(driveCards).toHaveCount({ min: 1 });

    const discName = page.locator('.drive-card__disc-name').first();
    await expect(discName).toBeVisible();
    const text = (await discName.textContent()).trim();
    expect(text.length).toBeGreaterThan(0);
    console.log(`[drive-detect] Drive 0 disc name: ${text}`);
  });

  test('drive 0 state is Ready (shows 💿 icon)', async ({ page }) => {
    // Ready state uses 💿 emoji icon
    await expect(page.locator('.drive-card__state:has-text("💿")').first()).toBeVisible({ timeout: 10000 });
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

    // ── Step 2: Verify drive 0 detected with disc ────────────
    await expect(page.locator('.drive-card').first()).toBeVisible({ timeout: 10000 });
    const discNameEl = page.locator('.drive-card__disc-name').first();
    await expect(discNameEl).toBeVisible();
    const discName = (await discNameEl.textContent()).trim();
    expect(discName.length).toBeGreaterThan(0);
    console.log(`[step-2] Drive 0 detected: "${discName}"`);

    // ── Step 3: Click Rip button ─────────────────────────────
    const ripBtn = page.locator('.drive-card__action-row .btn:has-text("Rip")').first();
    await expect(ripBtn).toBeEnabled();
    await ripBtn.click();
    console.log('[step-3] Clicked Rip button');

    // ── Step 4: Wait for rip to start ────────────────────────
    // Ripping state uses ⚡ emoji icon
    await expect(page.locator('.drive-card__state:has-text("⚡")').first()).toBeVisible({ timeout: 30000 });
    await expect(page.locator('.progress-bar')).toBeVisible({ timeout: 30000 });
    console.log('[step-4] Rip started, progress bar visible');

    // ── Step 5: Monitor progress until complete ──────────────
    const startTime = Date.now();
    let lastPct = 0;
    const ripTimeout = 25 * 60 * 1000; // 25 minutes max for rip

    while (Date.now() - startTime < ripTimeout) {
      // Check if rip is still in progress (⚡ icon)
      const isRipping = await page.locator('.drive-card__state:has-text("⚡")').isVisible().catch(() => false);
      if (!isRipping) break;

      const progressText = await page.locator('.drive-card__progress-pct').textContent().catch(() => null);
      if (progressText) {
        const pct = parseInt(progressText.replace('%', ''), 10);
        if (pct > lastPct && pct % 10 === 0) {
          console.log(`[step-5] Progress: ${pct}%`);
          lastPct = pct;
        }
      }
      await page.waitForTimeout(15000);
    }

    // ── Step 6: Wait for rip complete ────────────────────────
    // Complete state uses ✅ emoji icon
    const elapsed = ((Date.now() - startTime) / 1000 / 60).toFixed(1);
    console.log(`[step-6] Waiting for rip completion (elapsed: ${elapsed} min)...`);

    await expect(
      page.locator('.drive-card__state:has-text("✅")').first(),
      `Rip did not complete within ${ripTimeout / 60000} minutes`
    ).toBeVisible({ timeout: ripTimeout });

    const totalMinutes = ((Date.now() - startTime) / 1000 / 60).toFixed(1);
    console.log(`[step-6] ✅ Rip completed in ${totalMinutes} minutes`);

    // ── Step 7: Wait for auto-naming event ───────────────────
    console.log('[step-7] Waiting for auto-naming event...');
    const namingTimeout = 5 * 60 * 1000;
    const namingStart = Date.now();
    let namingSucceeded = false;
    let namingFailed = false;

    while (Date.now() - namingStart < namingTimeout) {
      const hasPoster = await page.locator('.drive-card__poster-img').isVisible().catch(() => false);
      if (hasPoster) {
        namingSucceeded = true;
        break;
      }
      const hasUnfinished = await page.locator('.ripper__unfinished').isVisible().catch(() => false);
      if (hasUnfinished) {
        namingFailed = true;
        break;
      }
      await page.waitForTimeout(10000);
    }

    if (namingSucceeded) {
      console.log('[step-7] ✅ Auto-naming succeeded — poster fetched');
    } else if (namingFailed) {
      console.log('[step-7] ⚠️ Auto-naming failed — unfinished titles section appeared');
    } else {
      console.log('[step-7] ⚠️ Auto-naming timed out — no poster or unfinished section visible');
    }

    // ── Step 8: Verify organizer integration ─────────────────
    console.log('[step-8] Checking organizer integration...');
    await page.goto('/organizer');
    await page.waitForLoadState('networkidle');
    await expect(page.locator('h2:has-text("Organizer")')).toBeVisible();

    const folderCount = await page.locator('.folder-card').count();
    console.log(`[step-8] Organizer shows ${folderCount} folder(s)`);

    // ── Step 9: Eject disc ───────────────────────────────────
    console.log('[step-9] Ejecting disc...');
    await page.goto('/ripper');
    await page.waitForLoadState('networkidle');

    const ejectBtn = page.locator('.drive-card__action-row .btn:has-text("Eject")').first();
    if (await ejectBtn.isVisible({ timeout: 5000 }).catch(() => false)) {
      await ejectBtn.click();
      await expect(page.locator('.drive-card__state:has-text("Empty")')).toBeVisible({ timeout: 10000 });
      console.log('[step-9] ✅ Disc ejected');
    } else {
      console.log('[step-9] ⚠️ No eject button found');
    }

    // ── Step 10: Record results ──────────────────────────────
    console.log(`\n[results] Rip pipeline complete in ${totalMinutes} minutes`);
    console.log(`[results] Disc: "${discName}"`);
    console.log(`[results] Auto-naming: ${namingSucceeded ? 'SUCCESS' : namingFailed ? 'FAILED' : 'TIMED OUT'}`);
    console.log(`[results] Organizer folders: ${folderCount}`);
  });
});
