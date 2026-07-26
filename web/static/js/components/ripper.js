/**
 * Ripper Component
 *
 * Drive list with disc info and rip start/cancel controls.
 */

import { ripper } from '../api.js';

let pollInterval = null;

export function renderRipper(container) {
  container.innerHTML = `
    <section class="section">
      <div class="section__header">
        <h2 class="section__title">Disc Drives</h2>
        <button class="btn btn--secondary btn--sm" id="btn-refresh-drives">Refresh</button>
      </div>
      <div id="drives-container">
        <div class="empty"><div class="spinner"></div></div>
      </div>
    </section>
  `;

  loadDrives();
  document.getElementById('btn-refresh-drives')?.addEventListener('click', loadDrives);
}

export function stopRipperPolling() {
  clearInterval(pollInterval);
  pollInterval = null;
}

async function loadDrives() {
  const container = document.getElementById('drives-container');
  if (!container) return;

  try {
    const result = await ripper.listDrives();
    const drives = result.data || [];

    if (drives.length === 0) {
      container.innerHTML = `
        <div class="empty">
          <div class="empty__icon">&#128190;</div>
          <div class="empty__title">No drives found</div>
          <div class="empty__description">Insert a disc or check your drive connection</div>
        </div>
      `;
      stopRipperPolling();
      return;
    }

    const hasActive = drives.some(d => d.state === 'Reading' || d.state === 'Ripping');
    if (hasActive && !pollInterval) {
      pollInterval = setInterval(loadDrives, 3000);
    } else if (!hasActive) {
      stopRipperPolling();
    }

    container.innerHTML = `
      <div class="drives-list">
        ${drives.map((drive, idx) => renderDriveCard(drive, idx)).join('')}
      </div>
    `;

    container.querySelectorAll('[data-rip-start]').forEach(btn => {
      btn.addEventListener('click', async (e) => {
        const index = parseInt(e.currentTarget.dataset.ripStart, 10);
        try {
          await ripper.start(index);
          loadDrives();
        } catch (err) {
          console.error('Failed to start rip:', err);
        }
      });
    });

    container.querySelectorAll('[data-rip-cancel]').forEach(btn => {
      btn.addEventListener('click', async (e) => {
        const index = parseInt(e.currentTarget.dataset.ripCancel, 10);
        try {
          await ripper.cancel(index);
          loadDrives();
        } catch (err) {
          console.error('Failed to cancel rip:', err);
        }
      });
    });
  } catch (error) {
    container.innerHTML = `
      <div class="empty">
        <div class="empty__icon">&#9888;</div>
        <div class="empty__title">Failed to load drives</div>
        <div class="empty__description">${escapeHtml(error.message)}</div>
      </div>
    `;
  }
}

function renderDriveCard(drive, index) {
  const stateColor = {
    Ready: 'success',
    Reading: 'accent',
    Ripping: 'accent',
    'No Disc': 'default',
    Error: 'error',
  }[drive.state] || 'default';

  const isActive = drive.state === 'Reading' || drive.state === 'Ripping';
  const isReady = drive.state === 'Ready' && drive.disc_name;

  return `
    <div class="card card--compact" style="margin-bottom: var(--space-3);">
      <div class="drive">
        <div class="drive__icon">&#128191;</div>
        <div class="drive__info">
          <div class="drive__name">${escapeHtml(drive.name)}</div>
          <div class="drive__disc">${drive.disc_name ? escapeHtml(drive.disc_name) : 'No disc inserted'}</div>
        </div>
        <div class="drive__actions flex gap-2">
          <span class="badge badge--${stateColor}">${drive.state}</span>
          ${isReady ? `<button class="btn btn--primary btn--sm" data-rip-start="${index}">Rip</button>` : ''}
          ${isActive ? `<button class="btn btn--danger btn--sm" data-rip-cancel="${index}">Cancel</button>` : ''}
        </div>
      </div>
    </div>
  `;
}

function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
