/**
 * Dashboard Component
 * 
 * Main overview showing stats, recent files, and quick actions.
 */

import { files, review, move, ripper } from '../api.js';
import { store } from '../store.js';
import { escapeHtml, statusColor } from '../utils.js';

/**
 * Render the dashboard
 */
export function renderDashboard(container) {
  container.innerHTML = `
    <section class="section">
      <div class="section__header">
        <h2 class="section__title">Overview</h2>
      </div>
      <div class="grid grid--5" id="stats-grid">
        <div class="card stat">
          <div class="stat__value" id="stat-scanned">-</div>
          <div class="stat__label">Scanned</div>
        </div>
        <div class="card stat stat--accent">
          <div class="stat__value" id="stat-matched">-</div>
          <div class="stat__label">Matched</div>
        </div>
        <div class="card stat stat--warning">
          <div class="stat__value" id="stat-review">-</div>
          <div class="stat__label">Needs Review</div>
        </div>
        <div class="card stat stat--success">
          <div class="stat__value" id="stat-confirmed">-</div>
          <div class="stat__label">Confirmed</div>
        </div>
        <div class="card stat stat--accent">
          <div class="stat__value" id="stat-moved">-</div>
          <div class="stat__label">Moved</div>
        </div>
      </div>
    </section>

    <div class="grid grid--2">
      <section class="section">
        <div class="section__header">
          <h3 class="section__title">Drives</h3>
          <button class="btn btn--secondary btn--sm" id="btn-refresh-drives">
            Refresh
          </button>
        </div>
        <div id="drives-container">
          <div class="empty">
            <div class="spinner"></div>
          </div>
        </div>
      </section>

      <section class="section">
        <div class="section__header">
          <h3 class="section__title">Recent Files</h3>
          <a href="#files" class="btn btn--secondary btn--sm">View All</a>
        </div>
        <div id="recent-files-container">
          <div class="empty">
            <div class="spinner"></div>
          </div>
        </div>
      </section>
    </div>

    <section class="section">
      <div class="section__header">
        <h3 class="section__title">Review Queue</h3>
        <span class="badge badge--warning" id="review-count">0</span>
      </div>
      <div id="review-container">
        <div class="empty">
          <div class="empty__icon">&#10003;</div>
          <div class="empty__title">All caught up!</div>
          <div class="empty__description">No files need review</div>
        </div>
      </div>
    </section>
  `;

  // Load data
  loadStats();
  loadDrives();
  loadRecentFiles();
  loadReviewQueue();

  // Event listeners
  document.getElementById('btn-refresh-drives')?.addEventListener('click', loadDrives);
}

async function loadStats() {
  try {
    const [scanned, matched, reviewRes, moveRes] = await Promise.all([
      files.list({ status: 'Scanned', per_page: 1 }),
      files.list({ status: 'Matched', per_page: 1 }),
      review.list(),
      move.status(),
    ]);

    document.getElementById('stat-scanned').textContent = scanned.total || 0;
    document.getElementById('stat-matched').textContent = matched.total || 0;
    document.getElementById('stat-review').textContent = reviewRes.data?.length || 0;
    document.getElementById('stat-confirmed').textContent = moveRes.data?.confirmed_count || 0;
    document.getElementById('stat-moved').textContent = moveRes.data?.moved_count || 0;
  } catch (error) {
    console.error('Failed to load stats:', error);
  }
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
      return;
    }

    container.innerHTML = drives.map(drive => `
      <div class="card card--compact drive">
        <div class="drive__icon">&#128191;</div>
        <div class="drive__info">
          <div class="drive__name">${escapeHtml(drive.name)}</div>
          <div class="drive__disc">${drive.disc_name ? escapeHtml(drive.disc_name) : 'No disc'}</div>
        </div>
        <div class="drive__actions">
          <span class="badge badge--${drive.state === 'Ready' ? 'success' : 'default'}">
            ${drive.state}
          </span>
        </div>
      </div>
    `).join('');
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

async function loadRecentFiles() {
  const container = document.getElementById('recent-files-container');
  if (!container) return;

  try {
    const result = await files.list({ per_page: 5 });
    const fileList = result.data?.items || [];

    if (fileList.length === 0) {
      container.innerHTML = `
        <div class="empty">
          <div class="empty__icon">&#128193;</div>
          <div class="empty__title">No files yet</div>
          <div class="empty__description">Run a scan to get started</div>
        </div>
      `;
      return;
    }

    container.innerHTML = `
      <div class="table" style="overflow-x: auto;">
        <table>
          <thead>
            <tr>
              <th>Filename</th>
              <th>Type</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            ${fileList.map(file => `
              <tr>
                <td class="truncate" style="max-width: 200px;">${escapeHtml(file.filename)}</td>
                <td><span class="badge badge--default">${file.media_type}</span></td>
                <td><span class="badge badge--${statusColor(file.status)}">${file.status}</span></td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>
    `;
  } catch (error) {
    container.innerHTML = `
      <div class="empty">
        <div class="empty__icon">&#9888;</div>
        <div class="empty__title">Failed to load files</div>
        <div class="empty__description">${escapeHtml(error.message)}</div>
      </div>
    `;
  }
}

async function loadReviewQueue() {
  const container = document.getElementById('review-container');
  const countBadge = document.getElementById('review-count');
  if (!container) return;

  try {
    const result = await review.list();
    const queue = result.data || [];

    countBadge.textContent = queue.length;

    if (queue.length === 0) {
      container.innerHTML = `
        <div class="empty">
          <div class="empty__icon">&#10003;</div>
          <div class="empty__title">All caught up!</div>
          <div class="empty__description">No files need review</div>
        </div>
      `;
      return;
    }

    container.innerHTML = queue.slice(0, 5).map(file => `
      <div class="card card--compact" style="margin-bottom: var(--space-2);">
        <div class="flex items-center justify-between">
          <div class="flex-1 min-w-0">
            <div class="font-medium truncate">${escapeHtml(file.canonical_title || file.filename)}</div>
            <div class="text-sm text-muted">${escapeHtml(file.filename)}</div>
          </div>
          <div class="flex gap-2">
            <button class="btn btn--primary btn--sm" onclick="window.riply.review.accept('${file.id}')">
              Accept
            </button>
            <button class="btn btn--danger btn--sm" onclick="window.riply.review.reject('${file.id}')">
              Reject
            </button>
          </div>
        </div>
      </div>
    `).join('');
  } catch (error) {
    container.innerHTML = `
      <div class="empty">
        <div class="empty__icon">&#9888;</div>
        <div class="empty__title">Failed to load review queue</div>
        <div class="empty__description">${escapeHtml(error.message)}</div>
      </div>
    `;
  }
}
