<script>
  import { onMount } from 'svelte';
  import { filesApi } from '$lib/api';
  import { FileRecord } from '$lib/types';

  const PER_PAGE = 25;

  let files = $state([]);
  let total = $state(0);
  let currentPage = $state(1);
  let currentFilter = $state('');
  let loading = $state(false);

  onMount(() => {
    loadFiles();
  });

  async function loadFiles() {
    loading = true;
    try {
      const params = { page: currentPage, per_page: PER_PAGE };
      if (currentFilter) params.status = currentFilter;

      const result = await filesApi.list(params);
      files = result.items || [];
      total = result.total || 0;
    } catch (e) {
      console.error('Failed to load files:', e);
    } finally {
      loading = false;
    }
  }

  function handleFilterChange(e) {
    currentFilter = e.target.value;
    currentPage = 1;
    loadFiles();
  }

  function handlePrev() {
    if (currentPage > 1) {
      currentPage--;
      loadFiles();
    }
  }

  function handleNext() {
    const totalPages = Math.ceil(total / PER_PAGE);
    if (currentPage < totalPages) {
      currentPage++;
      loadFiles();
    }
  }

  function statusColor(status) {
    const colors = {
      Scanned: 'default',
      Matched: 'accent',
      NeedsReview: 'warning',
      Confirmed: 'success',
      Moved: 'success',
      Error: 'error',
    };
    return colors[status] || 'default';
  }

  function formatSize(size) {
    if (!size) return '-';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (size >= 1024 && i < units.length - 1) {
      size /= 1024;
      i++;
    }
    return `${size.toFixed(1)} ${units[i]}`;
  }

  const totalPages = $derived(Math.ceil(total / PER_PAGE));
  const showingStart = $derived((currentPage - 1) * PER_PAGE + 1);
  const showingEnd = $derived(Math.min(currentPage * PER_PAGE, total));
</script>

<section class="section">
  <div class="section__header">
    <h2 class="section__title">Files</h2>
    <div class="flex gap-2">
      <select id="files-filter" class="select" onchange={handleFilterChange}>
        <option value="">All statuses</option>
        <option value="Scanned">Scanned</option>
        <option value="Matched">Matched</option>
        <option value="NeedsReview">Needs Review</option>
        <option value="Confirmed">Confirmed</option>
        <option value="Moved">Moved</option>
        <option value="Error">Failed</option>
      </select>
    </div>
  </div>

  {#if loading}
    <div class="empty"><div class="spinner"></div></div>
  {:else if files.length === 0}
    <div class="empty">
      <div class="empty__icon">&#128193;</div>
      <div class="empty__title">No files found</div>
      <div class="empty__description">{currentFilter ? 'No files with this status' : 'Run a scan to get started'}</div>
    </div>
  {:else}
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
          {#each files as file}
            <tr>
              <td class="truncate" style="max-width: 250px;" title={file.path}>{file.filename}</td>
              <td><span class="badge badge--default">{file.media_type}</span></td>
              <td class="truncate" style="max-width: 200px;">{file.canonical_title || file.suggested_title || '-'}</td>
              <td><span class="badge badge--{statusColor(file.status)}">{file.status}</span></td>
              <td class="text-muted">{formatSize(file.size)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if totalPages > 1}
      <div class="flex items-center justify-between" style="margin-top: var(--space-4);">
        <span class="text-sm text-muted">
          Showing {showingStart}–{showingEnd} of {total}
        </span>
        <div class="flex gap-2">
          <button class="btn btn--secondary btn--sm" onclick={handlePrev} disabled={currentPage <= 1}>Previous</button>
          <button class="btn btn--secondary btn--sm" onclick={handleNext} disabled={currentPage >= totalPages}>Next</button>
        </div>
      </div>
    {/if}
  {/if}
</section>
