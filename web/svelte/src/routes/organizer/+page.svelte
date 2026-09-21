<script>
  import { onMount } from 'svelte';
  import { organizerApi } from '$lib/api';
  import Tooltip from '$lib/components/Tooltip.svelte';
  import PreviewModal from '$lib/components/PreviewModal.svelte';
  import ConfirmModal from '$lib/components/ConfirmModal.svelte';

  /** @type {import('$lib/types').FolderRecord[]} */
  let folders = $state([]);
  let foldersLoading = $state(true);

  // ── Preview Modal ──────────────────────────────────────────
  /** @type {import('$lib/types').FolderRecord | null} */
  let previewFolder = $state(null);

  // ── Confirm Modal ──────────────────────────────────────────
  let confirmModal = $state(null);
  // { title, message, confirmText, cancelText, variant, onConfirm }

  // ── Filter ─────────────────────────────────────────────────
  let filterStatus = $state('All');

  // ── Stats ──────────────────────────────────────────────────
  let stats = $state({});

  // ── Scan ───────────────────────────────────────────────────
  let scanDirectory = $state('');
  let scanLoading = $state(false);

  // ── Actions ────────────────────────────────────────────────
  let actionLoading = $state(null);

  // ── Lifecycle ──────────────────────────────────────────────

  onMount(async () => {
    await Promise.all([fetchFolders(), fetchStats()]);
  });

  // ── Data fetching ──────────────────────────────────────────

  async function fetchFolders() {
    try {
      foldersLoading = true;
      folders = await organizerApi.list(filterStatus === 'All' ? undefined : filterStatus);
    } catch (e) {
      console.error('Failed to load folders:', e);
    } finally {
      foldersLoading = false;
    }
  }

  async function fetchStats() {
    try {
      const result = await organizerApi.stats();
      const counts = {};
      for (const [status, count] of result.by_status) {
        counts[status] = Number(count);
      }
      stats = counts;
    } catch (e) {
      console.error('Failed to load stats:', e);
    }
  }

  // ── Preview Modal ──────────────────────────────────────────

  function openPreview(folder) {
    previewFolder = folder;
  }

  function closePreview() {
    previewFolder = null;
  }

  async function handlePreviewAccept(id) {
    try {
      await organizerApi.accept(id);
      await fetchFolders();
      await fetchStats();
      closePreview();
    } catch (e) {
      console.error('Accept failed:', e);
    }
  }

  async function handlePreviewReject(id) {
    try {
      await organizerApi.reject(id);
      await fetchFolders();
      await fetchStats();
      closePreview();
    } catch (e) {
      console.error('Reject failed:', e);
    }
  }

  // ── Actions ────────────────────────────────────────────────

  async function handleScan() {
    if (!scanDirectory.trim()) return;
    scanLoading = true;
    try {
      const result = await organizerApi.scan(scanDirectory);
      folders = result.folders;
      await fetchStats();
    } catch (e) {
      console.error('Scan failed:', e);
    } finally {
      scanLoading = false;
    }
  }

  async function handleAnalyze(id) {
    actionLoading = id;
    try {
      await organizerApi.analyze(id);
      await fetchFolders();
      await fetchStats();
    } catch (e) {
      console.error('Analyze failed:', e);
    } finally {
      actionLoading = null;
    }
  }

  async function handleAccept(id) {
    actionLoading = id;
    try {
      await organizerApi.accept(id);
      await fetchFolders();
      await fetchStats();
    } catch (e) {
      console.error('Accept failed:', e);
    } finally {
      actionLoading = null;
    }
  }

  async function handleReject(id) {
    actionLoading = id;
    try {
      await organizerApi.reject(id);
      await fetchFolders();
      await fetchStats();
    } catch (e) {
      console.error('Reject failed:', e);
    } finally {
      actionLoading = null;
    }
  }

  async function handleAcceptAll() {
    confirmModal = {
      title: 'Accept All',
      message: 'Accept all confident folders?',
      confirmText: 'Accept All',
      cancelText: 'Cancel',
      variant: 'primary',
      onConfirm: async () => {
        try {
          await organizerApi.acceptAll();
          await fetchFolders();
          await fetchStats();
        } catch (e) {
          console.error('Accept all failed:', e);
        } finally {
          confirmModal = null;
        }
      },
      onCancel: () => { confirmModal = null; },
    };
  }

  async function handleRejectAll() {
    confirmModal = {
      title: 'Reject All',
      message: 'Reject all unknown folders?',
      confirmText: 'Reject All',
      cancelText: 'Cancel',
      variant: 'danger',
      onConfirm: async () => {
        try {
          await organizerApi.rejectAll();
          await fetchFolders();
          await fetchStats();
        } catch (e) {
          console.error('Reject all failed:', e);
        } finally {
          confirmModal = null;
        }
      },
      onCancel: () => { confirmModal = null; },
    };
  }

  async function handleMove(id) {
    actionLoading = id;
    try {
      await organizerApi.move(id);
      await fetchFolders();
      await fetchStats();
    } catch (e) {
      console.error('Move failed:', e);
    } finally {
      actionLoading = null;
    }
  }

  async function handleDelete(id) {
    const folder = folders.find(f => f.id === id);
    if (!folder) return;
    confirmModal = {
      title: 'Delete Folder',
      message: `Delete "${folder.name}"? This cannot be undone.`,
      confirmText: 'Delete',
      cancelText: 'Cancel',
      variant: 'danger',
      onConfirm: async () => {
        try {
          await organizerApi.delete(id);
          folders = folders.filter(f => f.id !== id);
          await fetchStats();
        } catch (e) {
          console.error('Delete failed:', e);
        } finally {
          confirmModal = null;
        }
      },
      onCancel: () => { confirmModal = null; },
    };
  }

  // ── Helpers ────────────────────────────────────────────────

  function statusColor(status) {
    switch (status) {
      case 'Scanned': return 'var(--color-text-muted)';
      case 'Confident': return 'var(--color-success)';
      case 'Uncertain': return 'var(--color-warning)';
      case 'Unknown': return 'var(--color-warning)';
      case 'Quarantined': return 'var(--color-danger)';
      case 'Moved': return 'var(--color-accent)';
      default: return 'var(--color-text-muted)';
    }
  }

  function formatSize(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }

  const statusFilterOptions = ['All', 'Scanned', 'Confident', 'Uncertain', 'Unknown', 'Quarantined', 'Moved'];

  function getAnalyzeDisabledTooltip(status) {
    switch (status) {
      case 'Confident': return 'Already analyzed and accepted';
      case 'Uncertain': return 'Uncertain — Preview or Accept first';
      case 'Unknown': return 'Unknown status — Accept or Reject first';
      case 'Quarantined': return 'Quarantined — cannot analyze';
      case 'Moved': return 'Already moved — no action needed';
      default: return 'Analyze only available for scanned folders';
    }
  }

  function getAcceptDisabledTooltip(status) {
    switch (status) {
      case 'Confident': return 'Already accepted';
      case 'Uncertain': return 'Preview to review before accepting';
      case 'Quarantined': return 'Quarantined — cannot accept';
      case 'Moved': return 'Already moved — no action needed';
      default: return 'Accept only available for scanned/uncertain folders';
    }
  }

  function getRejectDisabledTooltip(status) {
    switch (status) {
      case 'Confident': return 'Already accepted — cannot reject';
      case 'Uncertain': return 'Preview to review before rejecting';
      case 'Quarantined': return 'Already quarantined';
      case 'Moved': return 'Already moved — no action needed';
      default: return 'Reject only available for scanned/uncertain folders';
    }
  }

  function getMoveDisabledTooltip(status) {
    switch (status) {
      case 'Scanned': return 'Must analyze or accept first';
      case 'Uncertain': return 'Preview to review before moving';
      case 'Unknown': return 'Unknown — Accept or Reject first';
      case 'Quarantined': return 'Quarantined — cannot move';
      case 'Moved': return 'Already moved — no action needed';
      default: return 'Move only available for confident folders';
    }
  }

  function getPreviewDisabledTooltip(status) {
    switch (status) {
      case 'Moved': return 'Already moved — no preview needed';
      case 'Quarantined': return 'Quarantined — no preview needed';
      default: return 'Preview available for all active folders';
    }
  }
</script>

<div class="organizer">
  <!-- Header -->
  <div class="organizer__header">
    <h2 class="organizer__title">Organizer</h2>
    <div class="organizer__header-actions">
      <button
        class="btn btn--secondary btn--sm"
        onclick={handleAcceptAll}
        disabled={actionLoading !== null}
        title="Accept all confident folders"
      >
        Accept All
      </button>
      <button
        class="btn btn--secondary btn--sm"
        onclick={handleRejectAll}
        disabled={actionLoading !== null}
        title="Reject all unknown folders"
      >
        Reject All
      </button>
    </div>
  </div>

  <!-- Scan section -->
  <div class="organizer__scan">
    <h3 class="organizer__scan-title">Scan Directory</h3>
    <div class="organizer__scan-form">
      <input
        class="input"
        type="text"
        placeholder="/path/to/scan"
        bind:value={scanDirectory}
        disabled={scanLoading}
      />
      <button
        class="btn btn--primary btn--sm"
        onclick={handleScan}
        disabled={scanLoading || !scanDirectory.trim()}
      >
        {#if scanLoading}Scanning...{:else}Scan{/if}
      </button>
    </div>
  </div>

  <!-- Stats -->
  <div class="organizer__stats">
    <div class="stat-card">
      <span class="stat-card__label">Total</span>
      <span class="stat-card__value">{stats['Scanned'] || 0} + {stats['Confident'] || 0} + {stats['Uncertain'] || 0} + {stats['Unknown'] || 0}</span>
    </div>
    <div class="stat-card stat-card--success">
      <span class="stat-card__label">Confident</span>
      <span class="stat-card__value">{stats['Confident'] || 0}</span>
    </div>
    <div class="stat-card stat-card--warning">
      <span class="stat-card__label">Uncertain</span>
      <span class="stat-card__value">{stats['Uncertain'] || 0}</span>
    </div>
    <div class="stat-card stat-card--warning">
      <span class="stat-card__label">Unknown</span>
      <span class="stat-card__value">{stats['Unknown'] || 0}</span>
    </div>
    <div class="stat-card stat-card--danger">
      <span class="stat-card__label">Quarantined</span>
      <span class="stat-card__value">{stats['Quarantined'] || 0}</span>
    </div>
    <div class="stat-card stat-card--accent">
      <span class="stat-card__label">Moved</span>
      <span class="stat-card__value">{stats['Moved'] || 0}</span>
    </div>
  </div>

  <!-- Filter -->
  <div class="organizer__filters">
    {#each statusFilterOptions as status}
      <button
        class="filter-btn"
        class:filter-btn--active={filterStatus === status}
        onclick={() => { filterStatus = status; fetchFolders(); }}
      >
        {status}
      </button>
    {/each}
  </div>

  <!-- Folder list -->
  <div class="organizer__folders">
    {#if foldersLoading}
      <div class="organizer__loading">Loading folders...</div>
    {:else if folders.length === 0}
      <div class="organizer__empty">No folders found. Scan a directory to get started.</div>
    {:else}
      {#each folders as folder (folder.id)}
        <div class="folder-card" class:folder-card--accepted={folder.status === 'Confident'} class:folder-card--rejected={folder.status === 'Unknown'} class:folder-card--moved={folder.status === 'Moved'} class:folder-card--quarantined={folder.status === 'Quarantined'}>
          <div class="folder-card__info">
            <div class="folder-card__header">
              <span class="folder-card__status" style="color: {statusColor(folder.status)}">
                {folder.status}
              </span>
              <span class="folder-card__name">{folder.name}</span>
              {#if folder.media_type}
                <span class="folder-card__media-type">{folder.media_type}</span>
              {/if}
            </div>
            {#if folder.suggested_name}
              <div class="folder-card__title">
                {folder.suggested_name}{#if folder.suggested_year} ({folder.suggested_year}){/if}
              </div>
            {/if}
            <div class="folder-card__meta">
              <span>{folder.file_count} file{folder.file_count !== 1 ? 's' : ''}</span>
              {#if folder.confidence}
                <span>{(folder.confidence * 100).toFixed(0)}% confidence</span>
              {/if}
              {#if folder.tmdb_id}
                <span>TMDB: {folder.tmdb_id}</span>
              {/if}
            </div>
          </div>
          <div class="folder-card__actions">
            <!-- Preview button -->
            {#if folder.status !== 'Moved' && folder.status !== 'Quarantined'}
              <button
                class="btn btn--secondary btn--sm"
                onclick={() => openPreview(folder)}
                disabled={actionLoading !== null}
                title="Preview changes before moving"
              >
                Preview
              </button>
            {:else}
              <Tooltip text={getPreviewDisabledTooltip(folder.status)}>
                <button class="btn btn--secondary btn--sm btn--disabled" disabled>
                  Preview
                </button>
              </Tooltip>
            {/if}

            <!-- Analyze button -->
            {#if folder.status === 'Scanned'}
              <button
                class="btn btn--secondary btn--sm"
                onclick={() => handleAnalyze(folder.id)}
                disabled={actionLoading !== null}
                title="Analyze with metadata search"
              >
                Analyze
              </button>
            {:else}
              <Tooltip text={getAnalyzeDisabledTooltip(folder.status)}>
                <button class="btn btn--secondary btn--sm btn--disabled" disabled>
                  Analyze
                </button>
              </Tooltip>
            {/if}

            <!-- Accept button -->
            {#if folder.status === 'Scanned' || folder.status === 'Uncertain'}
              <button
                class="btn btn--success btn--sm"
                onclick={() => handleAccept(folder.id)}
                disabled={actionLoading !== null}
                title="Accept — mark as confident"
              >
                Accept
              </button>
            {:else}
              <Tooltip text={getAcceptDisabledTooltip(folder.status)}>
                <button class="btn btn--success btn--sm btn--disabled" disabled>
                  Accept
                </button>
              </Tooltip>
            {/if}

            <!-- Reject button -->
            {#if folder.status === 'Scanned' || folder.status === 'Uncertain'}
              <button
                class="btn btn--warning btn--sm"
                onclick={() => handleReject(folder.id)}
                disabled={actionLoading !== null}
                title="Reject — mark as unknown"
              >
                Reject
              </button>
            {:else}
              <Tooltip text={getRejectDisabledTooltip(folder.status)}>
                <button class="btn btn--warning btn--sm btn--disabled" disabled>
                  Reject
                </button>
              </Tooltip>
            {/if}

            <!-- Move button -->
            {#if folder.status === 'Confident'}
              <button
                class="btn btn--primary btn--sm"
                onclick={() => handleMove(folder.id)}
                disabled={actionLoading !== null}
                title="Move to destination"
              >
                Move
              </button>
            {:else}
              <Tooltip text={getMoveDisabledTooltip(folder.status)}>
                <button class="btn btn--primary btn--sm btn--disabled" disabled>
                  Move
                </button>
              </Tooltip>
            {/if}

            <!-- Delete button (always active) -->
            <button
              class="btn btn--danger btn--sm"
              onclick={() => handleDelete(folder.id)}
              title="Delete folder record"
            >
              Delete
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Preview Modal -->
  {#if previewFolder}
    <PreviewModal
      folder={previewFolder}
      onClose={closePreview}
      onAccept={handlePreviewAccept}
      onReject={handlePreviewReject}
    />
  {/if}

  <!-- Confirm Modal -->
  {#if confirmModal}
    <ConfirmModal
      title={confirmModal.title}
      message={confirmModal.message}
      confirmText={confirmModal.confirmText}
      cancelText={confirmModal.cancelText}
      variant={confirmModal.variant}
      onConfirm={confirmModal.onConfirm}
      onCancel={confirmModal.onCancel}
    />
  {/if}
</div>

<style>
  .organizer {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    height: 100%;
  }

  /* ── Header ─────────────────────────────────────────────── */
  .organizer__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .organizer__title {
    font-size: var(--text-xl);
    font-weight: 700;
    margin: 0;
  }

  .organizer__header-actions {
    display: flex;
    gap: var(--space-2);
  }

  /* ── Scan ───────────────────────────────────────────────── */
  .organizer__scan {
    padding: var(--space-4);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
  }

  .organizer__scan-title {
    font-size: var(--text-sm);
    font-weight: 600;
    margin: 0 0 var(--space-3) 0;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .organizer__scan-form {
    display: flex;
    gap: var(--space-2);
  }

  .organizer__scan-form .input {
    flex: 1;
  }

  /* ── Stats ──────────────────────────────────────────────── */
  .organizer__stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(100px, 1fr));
    gap: var(--space-2);
  }

  .stat-card {
    padding: var(--space-3);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    text-align: center;
  }

  .stat-card--success { border-color: var(--color-success); }
  .stat-card--warning { border-color: var(--color-warning); }
  .stat-card--danger { border-color: var(--color-danger); }
  .stat-card--accent { border-color: var(--color-accent); }

  .stat-card__label {
    display: block;
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--space-1);
  }

  .stat-card__value {
    font-size: var(--text-lg);
    font-weight: 700;
  }

  .stat-card--success .stat-card__value { color: var(--color-success); }
  .stat-card--warning .stat-card__value { color: var(--color-warning); }
  .stat-card--danger .stat-card__value { color: var(--color-danger); }
  .stat-card--accent .stat-card__value { color: var(--color-accent); }

  /* ── Filters ────────────────────────────────────────────── */
  .organizer__filters {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .filter-btn {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full, 9999px);
    background: var(--color-bg-secondary);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .filter-btn:hover {
    background: var(--color-bg-tertiary);
    color: var(--color-text-primary);
  }

  .filter-btn--active {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: white;
  }

  /* ── Folders ────────────────────────────────────────────── */
  .organizer__folders {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    flex: 1;
    overflow-y: auto;
  }

  .organizer__loading,
  .organizer__empty {
    text-align: center;
    padding: var(--space-8) 0;
    color: var(--color-text-muted);
    font-size: var(--text-base);
  }

  /* ── Folder Card ────────────────────────────────────────── */
  .folder-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-4);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    gap: var(--space-3);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .folder-card--accepted {
    border-color: var(--color-success);
    background: var(--color-success-muted);
  }

  .folder-card--rejected {
    border-color: var(--color-warning);
    background: var(--color-warning-muted);
  }

  .folder-card--moved {
    border-color: var(--color-accent);
    background: var(--color-accent-muted);
    opacity: 0.75;
  }

  .folder-card--quarantined {
    border-color: var(--color-danger);
    background: var(--color-danger-muted);
  }

  .folder-card__info {
    flex: 1;
    min-width: 0;
  }

  .folder-card__header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .folder-card__status {
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .folder-card__name {
    font-weight: 600;
    font-size: var(--text-base);
  }

  .folder-card__media-type {
    font-size: var(--text-xs);
    padding: var(--space-1) var(--space-2);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
  }

  .folder-card__title {
    font-size: var(--text-sm);
    color: var(--color-primary);
    margin-top: var(--space-1);
  }

  .folder-card__meta {
    display: flex;
    gap: var(--space-3);
    margin-top: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .folder-card__actions {
    display: flex;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  /* ── Disabled Buttons ──────────────────────────────────── */
  .btn--disabled {
    opacity: 0.4;
    cursor: not-allowed;
    pointer-events: none;
  }

  /* ── Responsive ─────────────────────────────────────────── */
  @media (max-width: 768px) {
    .folder-card {
      flex-direction: column;
      align-items: flex-start;
    }

    .folder-card__actions {
      width: 100%;
      justify-content: flex-start;
      flex-wrap: wrap;
    }

    .organizer__stats {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
