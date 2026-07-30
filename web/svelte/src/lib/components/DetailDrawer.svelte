<script>
  import { onMount, onDestroy } from 'svelte';
  import { searchApi, filesApi } from '$lib/api';
  import { FileRecord, SearchResult } from '$lib/types';

  let { file, open, onClose, onConfirm, onReject } = $props();

  // ── Form state ──────────────────────────────────────────────
  let title = $state('');
  let year = $state(null);
  let media_type = $state('Movie');
  let series_name = $state('');
  let season_number = $state(null);
  let episode_number = $state(null);

  // ── Search state ────────────────────────────────────────────
  let searchQuery = $state('');
  let searchResults = $state([]);
  let searchLoading = $state(false);
  let searchTimeout = $state(null);

  // ── Helpers ─────────────────────────────────────────────────

  function formatDuration(seconds) {
    if (!seconds || seconds <= 0) return '-';
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function formatSize(bytes) {
    if (!bytes) return '-';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let size = bytes;
    while (size >= 1024 && i < units.length - 1) {
      size /= 1024;
      i++;
    }
    return `${size.toFixed(1)} ${units[i]}`;
  }

  function statusBadge(status) {
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

  function isTv() {
    return media_type === 'Tv';
  }

  // ── Keyboard ────────────────────────────────────────────────

  function handleKeydown(e) {
    if (e.key === 'Escape') {
      close();
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeydown);
  });

  onDestroy(() => {
    document.removeEventListener('keydown', handleKeydown);
  });

  // ── Form reset ──────────────────────────────────────────────

  $effect(() => {
    if (file) {
      title = file.canonical_title || file.suggested_title || '';
      year = file.canonical_year || file.suggested_year || null;
      media_type = file.media_type || 'Movie';
      series_name = file.series_name || '';
      season_number = file.season_number || null;
      episode_number = file.episode_number || null;
      searchQuery = '';
      searchResults = [];
    }
  });

  // ── Search ──────────────────────────────────────────────────

  async function handleSearch() {
    if (searchTimeout) clearTimeout(searchTimeout);
    if (!searchQuery.trim()) {
      searchResults = [];
      searchLoading = false;
      return;
    }
    searchTimeout = setTimeout(async () => {
      searchLoading = true;
      try {
        const results = await searchApi.search(
          searchQuery.trim(),
          media_type === 'Tv' ? 'tv' : 'movie',
        );
        searchResults = results;
      } catch (e) {
        console.error('Search failed:', e);
        searchResults = [];
      } finally {
        searchLoading = false;
      }
    }, 300);
  }

  function selectResult(result) {
    title = result.title;
    year = result.year || null;
    media_type = result.is_tv ? 'Tv' : 'Movie';
    if (result.is_tv) {
      series_name = result.series_name || '';
      season_number = result.season_number || null;
      episode_number = result.episode_number || null;
    } else {
      series_name = '';
      season_number = null;
      episode_number = null;
    }
    searchResults = [];
    searchQuery = '';
  }

  // ── Actions ─────────────────────────────────────────────────

  function handleConfirm() {
    const data = {};
    if (title) data.canonical_title = title;
    if (year) data.canonical_year = year;
    data.media_type = media_type;
    if (isTv()) {
      if (series_name) data.series_name = series_name;
      if (season_number) data.season_number = season_number;
      if (episode_number) data.episode_number = episode_number;
    }
    onConfirm(file.id, data);
  }

  function handleReject() {
    onReject(file.id);
  }

  function close() {
    title = '';
    year = null;
    media_type = 'Movie';
    series_name = '';
    season_number = null;
    episode_number = null;
    searchQuery = '';
    searchResults = [];
    onClose();
  }
</script>

{#if file}
  <div class="modal-overlay" onclick={close}>
    <div class="detail-drawer" onclick={(e) => e.stopPropagation()}>
      <!-- Header -->
      <div class="modal__header">
        <h3 class="modal__title">
          {#if file.status === 'Matched'}Matched File{:else if file.status === 'NeedsReview'}Review Required{:else if file.status === 'Failed'}Match Failed{:else}File Details{/if}
        </h3>
        <button class="modal__close" onclick={close}>&times;</button>
      </div>

      <div class="modal__body">
        <!-- Title / Year / Type row -->
        <div class="flex gap-2" style="margin-bottom: var(--space-4);">
          <input
            class="select"
            placeholder="Title"
            bind:value={title}
            style="flex: 1;"
          />
          <input
            class="select"
            placeholder="Year"
            type="number"
            bind:value={year}
            style="width: 80px;"
          />
          <select class="select" bind:value={media_type}>
            <option value="Movie">Movie</option>
            <option value="Tv">TV</option>
          </select>
        </div>

        <!-- Search bar -->
        <div style="position: relative; margin-bottom: var(--space-4);">
          <input
            class="select w-full"
            placeholder="Search for title..."
            bind:value={searchQuery}
            oninput={handleSearch}
            style="padding-right: 2.5rem;"
          />
          {#if searchLoading}
            <div class="spinner" style="position: absolute; right: var(--space-2); top: 50%; transform: translateY(-50%); width: 16px; height: 16px;"></div>
          {/if}
          {#if searchResults.length > 0}
            <div class="search-results">
              {#each searchResults as result}
                <div class="search-result-item" onclick={() => selectResult(result)}>
                  <span class="search-result-item__title">{result.title}</span>
                  {#if result.year}
                    <span class="search-result-item__year">{result.year}</span>
                  {/if}
                  <span class="search-result-item__type">{result.is_tv ? 'TV' : 'Movie'}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Matched suggestion -->
        {#if file.status === 'Matched' && file.canonical_title && file.confidence}
          <div class="card card--compact" style="margin-bottom: var(--space-4);">
            <div class="flex items-center justify-between">
              <div>
                <div class="text-sm font-semibold">
                  {file.canonical_title} ({file.canonical_year})
                </div>
                <div class="text-sm text-muted">
                  Confidence: {Math.round(file.confidence * 100)}%
                </div>
              </div>
              <span class="badge badge--success">{Math.round(file.confidence * 100)}%</span>
            </div>
            <div class="flex gap-2" style="margin-top: var(--space-3);">
              <button
                class="btn btn--primary btn--sm"
                onclick={() => {
                  title = file.canonical_title;
                  year = file.canonical_year;
                  media_type = file.media_type;
                }}
              >
                Select this
              </button>
              <button
                class="btn btn--secondary btn--sm"
                onclick={() => {
                  searchQuery = title;
                  handleSearch();
                }}
              >
                Browse alternatives
              </button>
            </div>
          </div>
        {/if}

        <!-- NeedsReview suggestion -->
        {#if file.status === 'NeedsReview' && file.suggested_title}
          <div class="card card--compact" style="margin-bottom: var(--space-4);">
            <div class="text-sm text-muted">
              Suggested: {file.suggested_title}
              {#if file.suggested_year} ({file.suggested_year}){/if}
            </div>
            {#if file.confidence}
              <div class="text-sm text-muted" style="margin-top: var(--space-1);">
                Match confidence: {Math.round(file.confidence * 100)}%
              </div>
            {/if}
          </div>
        {/if}

        <!-- TV fields -->
        {#if isTv()}
          <div style="margin-bottom: var(--space-4);">
            <div class="flex gap-2" style="margin-bottom: var(--space-2);">
              <input
                class="select"
                placeholder="Series name"
                bind:value={series_name}
                style="flex: 1;"
              />
              <input
                class="select"
                placeholder="Season"
                type="number"
                bind:value={season_number}
                style="width: 80px;"
              />
              <input
                class="select"
                placeholder="Episode"
                type="number"
                bind:value={episode_number}
                style="width: 80px;"
              />
            </div>
          </div>
        {/if}

        <!-- File details -->
        <div class="card card--compact" style="margin-bottom: var(--space-4);">
          <div style="margin-bottom: var(--space-2);">
            <div class="text-xs text-muted" style="margin-bottom: var(--space-1);">FILENAME</div>
            <div class="truncate" title={file.filename}>{file.filename}</div>
          </div>
          <div style="margin-bottom: var(--space-2);">
            <div class="text-xs text-muted" style="margin-bottom: var(--space-1);">PATH</div>
            <div class="truncate" title={file.path}>{file.path}</div>
          </div>
          <div class="flex gap-4">
            <div>
              <div class="text-xs text-muted" style="margin-bottom: var(--space-1);">SIZE</div>
              <div class="text-sm">{formatSize(file.size)}</div>
            </div>
            <div>
              <div class="text-xs text-muted" style="margin-bottom: var(--space-1);">DURATION</div>
              <div class="text-sm">{formatDuration(file.duration)}</div>
            </div>
          </div>
          <div style="margin-top: var(--space-3);">
            <div class="text-xs text-muted" style="margin-bottom: var(--space-1);">STATUS</div>
            <span class="badge badge--{statusBadge(file.status)}">{file.status}</span>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="modal__footer">
        <button class="btn btn--danger" onclick={handleReject}>Reject</button>
        <button class="btn btn--primary" onclick={handleConfirm}>Confirm</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .detail-drawer {
    position: fixed;
    top: 0;
    right: 0;
    width: 480px;
    max-width: 100vw;
    height: 100vh;
    background: var(--color-bg-secondary);
    border-left: 1px solid var(--color-border);
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    z-index: calc(var(--z-modal) + 1);
    animation: slideIn var(--transition-normal) ease-out;
  }

  @keyframes slideIn {
    from {
      transform: translateX(100%);
    }
    to {
      transform: translateX(0);
    }
  }

  .search-results {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    margin-top: var(--space-1);
    max-height: 240px;
    overflow-y: auto;
    z-index: 10;
  }

  .search-result-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .search-result-item:hover {
    background: var(--color-bg-tertiary);
  }

  .search-result-item__title {
    flex: 1;
    font-size: var(--text-sm);
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .search-result-item__year {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .search-result-item__type {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  @media (max-width: 512px) {
    .detail-drawer {
      width: 100vw;
    }
  }
</style>
