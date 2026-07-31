<script>
  import { onMount } from 'svelte';
  import { filesApi, reviewApi, scanApi } from '$lib/api';
  import { moveApi } from '$lib/api/move';
  import DetailDrawer from '$lib/components/DetailDrawer.svelte';

  const PER_PAGE = 25;

  // ── Data state ──────────────────────────────────────────────
  let files = $state([]);
  let total = $state(0);
  let currentPage = $state(1);
  let currentFilter = $state('');
  let searchQuery = $state('');
  let searchDebounce = $state(null);
  let loading = $state(false);
  let retrying = $state(false);

  // ── DetailDrawer state ─────────────────────────────────────
  let selectedFile = $state(null);
  let drawerOpen = $state(false);

  // ── Move Preview modal state ───────────────────────────────
  let movePreviewOpen = $state(false);
  let movePreviewData = $state([]);
  let movePreviewLoading = $state(false);

  // ── Toast / notification state ─────────────────────────────
  let toastMessage = $state('');
  let toastVisible = $state(false);
  let toastType = $state('');
  let toastTimeout = $state(null);

  // ── Scan state ─────────────────────────────────────────────
  let scanDir = $state('');
  let scanning = $state(false);

  // ── Reset state ────────────────────────────────────────────
  let resetting = $state(false);

  // ── Analyze state ──────────────────────────────────────────
  let analyzing = $state(false);

  // ── Derived ────────────────────────────────────────────────
  const totalPages = $derived(Math.ceil(total / PER_PAGE));
  const showingStart = $derived((currentPage - 1) * PER_PAGE + 1);
  const showingEnd = $derived(Math.min(currentPage * PER_PAGE, total));

  // ── Lifecycle ──────────────────────────────────────────────
  onMount(() => {
    loadFiles();
  });

  // ── File loading ───────────────────────────────────────────
  async function loadFiles() {
    loading = true;
    try {
      const params = { page: currentPage, per_page: PER_PAGE };
      if (currentFilter && currentFilter !== 'Duplicates') params.status = currentFilter;
      if (searchQuery) params.search = searchQuery;

      const result = await filesApi.list(params);
      if (currentFilter === 'Duplicates') {
        files = (result.items || []).filter(f => f.duplicate_path);
        total = files.length;
      } else {
        files = result.items || [];
        total = result.total || 0;
      }
    } catch (e) {
      console.error('Failed to load files:', e);
    } finally {
      loading = false;
    }
  }

  // ── Filter & Search ────────────────────────────────────────
  function handleFilterChange(e) {
    currentFilter = e.target.value;
    currentPage = 1;
    loadFiles();
  }

  function handleSearchInput(e) {
    searchQuery = e.target.value;
    if (searchDebounce) clearTimeout(searchDebounce);
    searchDebounce = setTimeout(() => {
      currentPage = 1;
      loadFiles();
    }, 300);
  }

  // ── Pagination ─────────────────────────────────────────────
  function handlePrev() {
    if (currentPage > 1) {
      currentPage--;
      loadFiles();
    }
  }

  function handleNext() {
    const totalPagesCount = Math.ceil(total / PER_PAGE);
    if (currentPage < totalPagesCount) {
      currentPage++;
      loadFiles();
    }
  }

  // ── Retry Failed ───────────────────────────────────────────
  async function handleRetryFailed() {
    retrying = true;
    try {
      const res = await fetch('/api/analyze/retry-failed', { method: 'POST' });
      if (!res.ok) throw new Error(`Retry failed: ${res.status}`);
      showToast('Retry submitted', 'success');
      loadFiles();
    } catch (e) {
      console.error('Retry error:', e);
      showToast('Retry failed', 'error');
    } finally {
      retrying = false;
    }
  }

  // ── DetailDrawer handlers ──────────────────────────────────
  function openDrawer(file) {
    selectedFile = file;
    drawerOpen = true;
  }

  function handleDrawerClose() {
    drawerOpen = false;
    selectedFile = null;
    loadFiles();
  }

  async function handleDrawerConfirm(id, data) {
    try {
      await filesApi.update(id, data);
      showToast('File confirmed', 'success');
      drawerOpen = false;
      selectedFile = null;
      loadFiles();
    } catch (e) {
      console.error('Confirm failed:', e);
      showToast('Confirm failed', 'error');
    }
  }

  async function handleDrawerReject(id) {
    try {
      await reviewApi.reject(id);
      showToast('File rejected', 'success');
      drawerOpen = false;
      selectedFile = null;
      loadFiles();
    } catch (e) {
      console.error('Reject failed:', e);
      showToast('Reject failed', 'error');
    }
  }

  // ── Quick Accept ───────────────────────────────────────────
  async function handleQuickAccept(file) {
    try {
      await filesApi.update(file.id, {
        canonical_title: file.canonical_title,
        canonical_year: file.canonical_year,
        media_type: file.media_type,
      });
      await reviewApi.accept(file.id);
      showToast('Accepted', 'success');
      loadFiles();
    } catch (e) {
      console.error('Quick accept failed:', e);
      showToast('Accept failed', 'error');
    }
  }

  // ── Move Preview ───────────────────────────────────────────
  async function handleMovePreview() {
    movePreviewLoading = true;
    movePreviewOpen = true;
    try {
      movePreviewData = await moveApi.preview();
    } catch (e) {
      console.error('Move preview failed:', e);
      showToast('Move preview failed', 'error');
    } finally {
      movePreviewLoading = false;
    }
  }

  // ── Scan ───────────────────────────────────────────────────
  async function handleScan() {
    if (!scanDir.trim()) {
      showToast('Please enter a directory path', 'warning');
      return;
    }
    scanning = true;
    try {
      await scanApi.start(scanDir.trim());
      showToast('Scan started', 'success');
      scanDir = '';
      loadFiles();
    } catch (e) {
      console.error('Scan failed:', e);
      showToast('Scan failed', 'error');
    } finally {
      scanning = false;
    }
  }

  // ── Reset ──────────────────────────────────────────────────
  async function handleResetDb() {
    if (!window.confirm('Reset the database? This will permanently delete all file records and title groups.')) {
      return;
    }
    if (!window.confirm('This cannot be undone. Are you sure?')) {
      return;
    }
    resetting = true;
    try {
      const deleted = await filesApi.reset();
      showToast(`Database reset (${deleted} files removed)`, 'success');
      loadFiles();
    } catch (e) {
      console.error('Reset failed:', e);
      showToast('Reset failed', 'error');
    } finally {
      resetting = false;
    }
  }

  // ── Analyze ────────────────────────────────────────────────
  async function handleAnalyze() {
    analyzing = true;
    try {
      const res = await fetch('/api/analyze', { method: 'POST' });
      if (!res.ok) throw new Error(`Analyze failed: ${res.status}`);
      showToast('Analyze started', 'success');
      loadFiles();
    } catch (e) {
      console.error('Analyze failed:', e);
      showToast('Analyze failed', 'error');
    } finally {
      analyzing = false;
    }
  }

  // ── Toast ──────────────────────────────────────────────────
  function showToast(message, type = '') {
    toastMessage = message;
    toastType = type;
    toastVisible = true;
    if (toastTimeout) clearTimeout(toastTimeout);
    toastTimeout = setTimeout(() => {
      toastVisible = false;
    }, 3000);
  }

  // ── Helpers ────────────────────────────────────────────────
  function statusColor(status) {
    const colors = {
      Scanned: 'default',
      Matched: 'accent',
      NeedsReview: 'warning',
      Confirmed: 'success',
      Moved: 'success',
      Failed: 'error',
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
</script>

<section class="section">
  <!-- Header bar -->
  <div class="section__header">
    <h2 class="section__title">Files</h2>
    <div class="flex gap-2 flex-wrap">
      <input
        type="text"
        placeholder="Search files..."
        class="select"
        style="min-width: 200px;"
        oninput={handleSearchInput}
        value={searchQuery}
      />
      <select id="files-filter" class="select" onchange={handleFilterChange}>
        <option value="">All statuses</option>
        <option value="Scanned">Scanned</option>
        <option value="Matched">Matched</option>
        <option value="NeedsReview">Needs Review</option>
        <option value="Confirmed">Confirmed</option>
        <option value="Moved">Moved</option>
        <option value="Failed">Failed</option>
        <option value="Duplicates">Duplicates</option>
      </select>
      <button class="btn btn--secondary btn--sm" onclick={handleMovePreview} disabled={loading}>
        Move Preview
      </button>
      <button class="btn btn--secondary btn--sm" onclick={handleAnalyze} disabled={analyzing || loading}>
        {#if analyzing}Analyzing...{:else}Analyze{/if}
      </button>
      <button class="btn btn--secondary btn--sm" onclick={handleScan} disabled={scanning || loading}>
        {#if scanning}Scanning...{:else}Scan{/if}
      </button>
      {#if currentFilter === 'Failed' && total > 0}
        <button class="btn btn--warning btn--sm" onclick={handleRetryFailed} disabled={retrying}>
          {#if retrying}Retrying...{:else}Retry All Failed{/if}
        </button>
      {/if}
      <button class="btn btn--danger btn--sm" onclick={handleResetDb} disabled={resetting || loading}>
        {#if resetting}Resetting...{:else}Reset DB{/if}
      </button>
    </div>
  </div>

  <!-- Scan directory input (shown when Scan is clicked via modal) -->
  {#if movePreviewOpen}
    <div class="modal-overlay" onclick={() => movePreviewOpen = false}>
      <div class="modal" onclick={(e) => e.stopPropagation()}>
        <div class="modal__header">
          <h3 class="modal__title">Move Preview</h3>
          <button class="modal__close" onclick={() => movePreviewOpen = false}>&times;</button>
        </div>
        <div class="modal__body">
          {#if movePreviewLoading}
            <div class="empty"><div class="spinner"></div></div>
          {:else if movePreviewData.length === 0}
            <div class="empty">
              <div class="empty__icon">&#128193;</div>
              <div class="empty__title">No files to move</div>
              <div class="empty__description">No confirmed files found</div>
            </div>
          {:else}
            <div class="table" style="overflow-x: auto;">
              <table>
                <thead>
                  <tr>
                    <th>Filename</th>
                    <th>Source</th>
                    <th>Destination</th>
                    <th>Type</th>
                  </tr>
                </thead>
                <tbody>
                  {#each movePreviewData as item}
                    <tr>
                      <td class="truncate" style="max-width: 200px;" title={item.filename}>{item.filename}</td>
                      <td class="truncate" style="max-width: 250px;" title={item.source_path}>{item.source_path}</td>
                      <td class="truncate" style="max-width: 250px;" title={item.dest_path}>{item.dest_path}</td>
                      <td><span class="badge badge--default">{item.media_type}</span></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>
        <div class="modal__footer">
          <button class="btn btn--secondary" onclick={() => movePreviewOpen = false}>Close</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Main table -->
  {#if loading}
    <div class="empty"><div class="spinner"></div></div>
  {:else if files.length === 0}
    <div class="empty">
      <div class="empty__icon">&#128193;</div>
      <div class="empty__title">No files found</div>
      <div class="empty__description">{currentFilter || searchQuery ? 'No files match your criteria' : 'Run a scan to get started'}</div>
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
            <th style="min-width: 100px;">Quick Accept</th>
          </tr>
        </thead>
        <tbody>
          {#each files as file}
            <tr class="cursor-pointer" onclick={() => openDrawer(file)}>
              <td class="truncate" style="max-width: 250px;" title={file.path}>{file.filename}</td>
              <td><span class="badge badge--default">{file.media_type}</span></td>
              <td class="truncate" style="max-width: 200px;">{file.canonical_title || file.suggested_title || '-'}</td>
              <td><span class="badge badge--{statusColor(file.status)}">{file.status}</span></td>
              <td class="text-muted">{formatSize(file.size)}</td>
              <td>
                {#if file.status === 'Matched' && file.confidence >= 0.8 && file.canonical_title}
                  <button
                    class="btn btn--primary btn--sm"
                    onclick={(e) => { e.stopPropagation(); handleQuickAccept(file); }}
                  >
                    Accept
                  </button>
                {/if}
              </td>
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

  <!-- DetailDrawer -->
  {#if drawerOpen && selectedFile}
    <DetailDrawer
      file={selectedFile}
      open={drawerOpen}
      onClose={handleDrawerClose}
      onConfirm={handleDrawerConfirm}
      onReject={handleDrawerReject}
    />
  {/if}

  <!-- Toast notification -->
  {#if toastVisible}
    <div class="toast toast--{toastType} toast--visible">
      {toastMessage}
    </div>
  {/if}
</section>

<style>
  @keyframes slideIn {
    from {
      transform: translateY(20px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }

  .toast {
    position: fixed;
    bottom: var(--space-6);
    right: var(--space-6);
    padding: var(--space-3) var(--space-5);
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    z-index: var(--z-toast);
    animation: slideIn var(--transition-normal) ease-out;
  }

  .toast--success { border-color: var(--color-success); }
  .toast--error { border-color: var(--color-error); }
  .toast--warning { border-color: var(--color-warning); }
</style>
