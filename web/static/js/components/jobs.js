/**
 * Jobs Component
 *
 * Shows active and completed background jobs with progress bars.
 */

import { jobs } from '../api.js';

let pollInterval = null;

export function renderJobs(container) {
  container.innerHTML = `
    <section class="section">
      <div class="section__header">
        <h2 class="section__title">Jobs</h2>
        <button class="btn btn--secondary btn--sm" id="btn-refresh-jobs">Refresh</button>
      </div>
      <div id="jobs-container">
        <div class="empty">
          <div class="spinner"></div>
        </div>
      </div>
    </section>
  `;

  loadJobs();
  document.getElementById('btn-refresh-jobs')?.addEventListener('click', loadJobs);

  // Auto-refresh while jobs are active
  clearInterval(pollInterval);
  pollInterval = setInterval(loadJobs, 2000);
}

export function stopJobsPolling() {
  clearInterval(pollInterval);
  pollInterval = null;
}

async function loadJobs() {
  const container = document.getElementById('jobs-container');
  if (!container) return;

  try {
    const result = await jobs.list();
    const jobList = result.data || [];

    if (jobList.length === 0) {
      container.innerHTML = `
        <div class="empty">
          <div class="empty__icon">&#9881;</div>
          <div class="empty__title">No jobs yet</div>
          <div class="empty__description">Start a scan, analyze, or move from the header buttons</div>
        </div>
      `;
      stopJobsPolling();
      return;
    }

    const hasActive = jobList.some(j => j.status === 'Queued' || j.status === 'Running');
    if (!hasActive) stopJobsPolling();

    container.innerHTML = `
      <div class="jobs-list">
        ${jobList.map(j => renderJobCard(j)).join('')}
      </div>
    `;

    // Bind cancel buttons
    container.querySelectorAll('[data-cancel-job]').forEach(btn => {
      btn.addEventListener('click', async (e) => {
        const id = e.currentTarget.dataset.cancelJob;
        try {
          await jobs.cancel(id);
          loadJobs();
        } catch (err) {
          console.error('Failed to cancel job:', err);
        }
      });
    });
  } catch (error) {
    container.innerHTML = `
      <div class="empty">
        <div class="empty__icon">&#9888;</div>
        <div class="empty__title">Failed to load jobs</div>
        <div class="empty__description">${escapeHtml(error.message)}</div>
      </div>
    `;
  }
}

function renderJobCard(job) {
  const pct = job.progress_total > 0
    ? Math.round((job.progress_processed / job.progress_total) * 100)
    : 0;

  const statusClass = {
    Queued: 'default',
    Running: 'accent',
    Complete: 'success',
    Failed: 'error',
    Cancelled: 'warning',
  }[job.status] || 'default';

  const icon = {
    Queued: '&#9202;',
    Running: '&#9881;',
    Complete: '&#10003;',
    Failed: '&#10007;',
    Cancelled: '&#9898;',
  }[job.status] || '&#9881;';

  const canCancel = job.status === 'Queued' || job.status === 'Running';

  return `
    <div class="card card--compact" style="margin-bottom: var(--space-3);">
      <div class="flex items-center justify-between" style="margin-bottom: ${job.progress_total > 0 ? 'var(--space-3)' : '0'};">
        <div class="flex items-center gap-3">
          <span class="job-icon">${icon}</span>
          <div>
            <div class="font-medium">${escapeHtml(job.kind)}</div>
            <div class="text-sm text-muted">Job ${escapeHtml(job.id)}</div>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <span class="badge badge--${statusClass}">${job.status}</span>
          ${canCancel ? `<button class="btn btn--danger btn--sm" data-cancel-job="${escapeHtml(job.id)}">Cancel</button>` : ''}
        </div>
      </div>
      ${job.progress_total > 0 ? `
        <div class="progress">
          <div class="progress__bar" style="width: ${pct}%"></div>
        </div>
        <div class="text-sm text-muted" style="margin-top: var(--space-2);">
          ${job.progress_processed} / ${job.progress_total} (${pct}%)
        </div>
      ` : ''}
    </div>
  `;
}

function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
