/**
 * Files Component
 *
 * Paginated file list with status filtering.
 */

import { files } from '../api.js';
import { escapeHtml, formatSize, statusColor } from '../utils.js';

let currentPage = 1;
let currentFilter = '';
const PER_PAGE = 25;

export function renderFiles(container) {
  currentPage = 1;
  currentFilter = '';

  container.innerHTML = `
    <section class="section">
      <div class="section__header">
        <h2 class="section__title">Files</h2>
        <div class="flex gap-2">
          <select id="files-filter" class="select">
            <option value="">All statuses</option>
            <option value="Scanned">Scanned</option>
            <option value="Matched">Matched</option>
            <option value="Needs Review">Needs Review</option>
            <option value="Confirmed">Confirmed</option>
            <option value="Moved">Moved</option>
            <option value="Failed">Failed</option>
          </select>
        </div>
      </div>
      <div id="files-container">
        <div class="empty"><div class="spinner"></div></div>
      </div>
      <div id="files-pagination" class="pagination"></div>
    </section>
  `;

  document.getElementById('files-filter')?.addEventListener('change', (e) => {
    currentFilter = e.target.value;
    currentPage = 1;
    loadFiles();
  });

  loadFiles();
}

async function loadFiles() {
  const container = document.getElementById('files-container');
  const pagination = document.getElementById('files-pagination');
  if (!container) return;

  try {
    const params = { page: currentPage, per_page: PER_PAGE };
    if (currentFilter) params.status = currentFilter;

    const result = await files.list(params);
    const data = result.data || { items: [], total: 0, page: 1, per_page: PER_PAGE };
    const items = data.items || [];

    if (items.length === 0) {
      container.innerHTML = `
        <div class="empty">
          <div class="empty__icon">&#128193;</div>
          <div class="empty__title">No files found</div>
          <div class="empty__description">${currentFilter ? 'No files with this status' : 'Run a scan to get started'}</div>
        </div>
      `;
      if (pagination) pagination.innerHTML = '';
      return;
    }

    container.innerHTML = `
      <div class="table" style="overflow-x: auto;">
        <table>
          <thead>
            <tr>
              <th>Filename</th>
              <th>Type</th>
              <th>Title</th>
              <th>Status</th>
              <th>Size</th>
            </tr>
          </thead>
          <tbody>
            ${items.map(f => `
              <tr>
                <td class="truncate" style="max-width: 250px;" title="${escapeHtml(f.path)}">${escapeHtml(f.filename)}</td>
                <td><span class="badge badge--default">${f.media_type}</span></td>
                <td class="truncate" style="max-width: 200px;">${escapeHtml(f.canonical_title || f.suggested_title || '-')}</td>
                <td><span class="badge badge--${statusColor(f.status)}">${f.status}</span></td>
                <td class="text-muted">${formatSize(f.size)}</td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      </div>
    `;

    if (pagination) {
      const totalPages = Math.ceil(data.total / PER_PAGE);
      if (totalPages > 1) {
        pagination.innerHTML = `
          <div class="flex items-center justify-between" style="margin-top: var(--space-4);">
            <span class="text-sm text-muted">
              Showing ${(currentPage - 1) * PER_PAGE + 1}–${Math.min(currentPage * PER_PAGE, data.total)} of ${data.total}
            </span>
            <div class="flex gap-2">
              <button class="btn btn--secondary btn--sm" id="page-prev" ${currentPage <= 1 ? 'disabled' : ''}>Previous</button>
              <button class="btn btn--secondary btn--sm" id="page-next" ${currentPage >= totalPages ? 'disabled' : ''}>Next</button>
            </div>
          </div>
        `;
        document.getElementById('page-prev')?.addEventListener('click', () => { currentPage--; loadFiles(); });
        document.getElementById('page-next')?.addEventListener('click', () => { currentPage++; loadFiles(); });
      } else {
        pagination.innerHTML = '';
      }
    }
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
