<script>
  // Reusable media-naming modal (ADR-022).
  // Emits a chosen `MediaNamingChoice` via `onConfirm`; `onClose` fires on cancel.
  import { onMount, onDestroy } from 'svelte';
  import { searchApi } from '$lib/api';
  /** @typedef {import('$lib/types').SearchResult} SearchResult */

  let { initialTitle = '', mediaType = 'movie', suggest, onConfirm, onClose } = $props();

  /** @type {string} */
  let query = $state(initialTitle);
  /** @type {string} */
  let title = $state(initialTitle);
  /** @type {number | null} */
  let year = $state(null);
  /** @type {'Movie' | 'Tv'} */
  let media_type = /** @type {const} */ (mediaType === 'tv' ? 'Tv' : 'Movie');
  /** @type {string} */
  let series_name = $state('');
  /** @type {number | null} */
  let season_number = $state(null);
  /** @type {number | null} */
  let episode_number = $state(null);
  /** @type {SearchResult[]} */
  let results = /** @type {SearchResult[]} */ ($state([]));
  /** @type {boolean} */
  let loading = $state(false);
  /** @type {number | null} */
  let searchTimeout = $state(null);
  /** @type {number} */
  let selectedIndex = $state(-1);

  function isTv() {
    return media_type === 'Tv';
  }

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    if (e.key === 'Escape') {
      cancel();
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeydown);
    // Auto-lookup becomes a prefill: fills search box as default suggestion
    if (suggest && !initialTitle) {
      suggest().then((/** @type {{title: string, year?: number} | null} */ s) => {
        if (s && s.title) {
          query = s.title;
          title = s.title;
          if (s.year) year = s.year;
        }
      }).catch(() => {});
    }
  });

  onDestroy(() => {
    document.removeEventListener('keydown', handleKeydown);
  });

  function handleSearch() {
    if (searchTimeout) clearTimeout(searchTimeout);
    if (!query.trim()) {
      results = [];
      selectedIndex = -1;
      return;
    }
    searchTimeout = setTimeout(async () => {
      loading = true;
      try {
        const found = await searchApi.search(query.trim(), isTv() ? 'tv' : 'movie');
        results = /** @type {SearchResult[]} */ (found);
        selectedIndex = -1;
      } catch (/** @type {unknown} */ e) {
        console.error('Search failed:', e);
        results = [];
      } finally {
        loading = false;
      }
    }, 300);
  }

  /** @param {SearchResult} result */
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
    query = result.title;
    results = [];
    selectedIndex = -1;
  }

  /** @param {{title: string, year?: number, media_type: 'Movie' | 'Tv', series_name?: string, season_number?: number, episode_number?: number}} choice */
  function confirm(choice) {
    const finalTitle = title.trim() || query.trim();
    if (!finalTitle) return;
    onConfirm({
      title: finalTitle,
      year: year || undefined,
      media_type,
      series_name: isTv() && series_name ? series_name : undefined,
      season_number: isTv() ? season_number || undefined : undefined,
      episode_number: isTv() ? episode_number || undefined : undefined,
    });
  }

  function cancel() {
    onClose();
  }
</script>

<div class="modal-overlay" onclick={cancel}>
  <div class="naming-modal" onclick={(e) => e.stopPropagation()}>
    <div class="modal__header">
      <h3 class="modal__title">Name media</h3>
      <button class="modal__close" onclick={cancel}>&times;</button>
    </div>

    <div class="modal__body">
      <!-- Search box -->
      <div style="position: relative; margin-bottom: var(--space-4);">
        <input
          class="select w-full"
          placeholder="Search for a title or type a custom name..."
          bind:value={query}
          oninput={() => {
            title = query;
            handleSearch();
          }}
          style="padding-right: 2.5rem;"
        />
        {#if loading}
          <div class="spinner" style="position: absolute; right: var(--space-2); top: 50%; transform: translateY(-50%); width: 16px; height: 16px;"></div>
        {/if}
        {#if results.length > 0}
          <div class="naming-results">
            {#each results as result, i}
              <div
                class="naming-result"
                class:naming-result--active={i === selectedIndex}
                onclick={() => selectResult(result)}
              >
                <span class="naming-result__title">{result.title}</span>
                {#if result.year}
                  <span class="naming-result__year">{result.year}</span>
                {/if}
                <span class="naming-result__type">{result.is_tv ? 'TV' : 'Movie'}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Custom-title fallback -->
      <div class="flex gap-2" style="margin-bottom: var(--space-4);">
        <input class="select" placeholder="Title" bind:value={title} style="flex: 1;" />
        <input
          class="select"
          placeholder="Year"
          type="number"
          bind:value={year}
          style="width: 90px;"
        />
        <select class="select" bind:value={media_type}>
          <option value="Movie">Movie</option>
          <option value="Tv">TV</option>
        </select>
      </div>

      <!-- TV fields -->
      {#if isTv()}
        <div class="flex gap-2" style="margin-bottom: var(--space-4);">
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
            style="width: 90px;"
          />
          <input
            class="select"
            placeholder="Episode"
            type="number"
            bind:value={episode_number}
            style="width: 90px;"
          />
        </div>
      {/if}
    </div>

    <div class="modal__footer">
      <button class="btn btn--secondary" onclick={cancel}>Cancel</button>
      <button class="btn btn--primary" onclick={confirm} disabled={!(title.trim() || query.trim())}>
        Apply
      </button>
    </div>
  </div>
</div>

<style>
  .naming-modal {
    width: 480px;
    max-width: calc(100vw - var(--space-6));
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    z-index: calc(var(--z-modal) + 2);
    animation: namingIn var(--transition-normal) ease-out;
  }

  @keyframes namingIn {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  .naming-results {
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

  .naming-result {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .naming-result:hover,
  .naming-result--active {
    background: var(--color-bg-tertiary);
  }

  .naming-result__title {
    flex: 1;
    font-size: var(--text-sm);
    color: var(--color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .naming-result__year {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .naming-result__type {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
</style>
