/**
 * Review Component
 *
 * Dedicated review queue for files needing manual confirmation.
 */

import { review } from '../api.js';

export function renderReview(container) {
  container.innerHTML = `
    <section class="section">
      <div class="section__header">
        <h2 class="section__title">Review Queue</h2>
        <span class="badge badge--warning" id="review-count">0</span>
      </div>
      <div id="review-container">
        <div class="empty"><div class="spinner"></div></div>
      </div>
    </section>
  `;

  loadReviewQueue();
}

async function loadReviewQueue() {
  const container = document.getElementById('review-container');
  const countBadge = document.getElementById('review-count');
  if (!container) return;

  try {
    const result = await review.list();
    const queue = result.data || [];

    if (countBadge) countBadge.textContent = queue.length;

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

    container.innerHTML = queue.map(file => `
      <div class="card card--compact" style="margin-bottom: var(--space-3);">
        <div class="flex items-start justify-between gap-4">
          <div class="flex-1 min-w-0">
            <div class="font-medium" style="margin-bottom: var(--space-1);">
              ${escapeHtml(file.canonical_title || file.suggested_title || file.filename)}
            </div>
            <div class="text-sm text-muted" style="margin-bottom: var(--space-2);">
              ${escapeHtml(file.filename)}
            </div>
            <div class="flex gap-2" style="flex-wrap: wrap;">
              <span class="badge badge--default">${file.media_type}</span>
              ${file.suggested_year ? `<span class="badge badge--default">${file.suggested_year}</span>` : ''}
              ${file.confidence ? `<span class="badge badge--default">${Math.round(file.confidence * 100)}% match</span>` : ''}
              ${file.tmdb_id ? `<span class="badge badge--accent">TMDb</span>` : ''}
            </div>
          </div>
          <div class="flex gap-2" style="flex-shrink: 0;">
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

function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
