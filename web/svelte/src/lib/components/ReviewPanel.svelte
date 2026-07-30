<script>
  import { onMount } from 'svelte';
  import { reviewApi } from '$lib/api';

  let { open, onDriveClick } = $props();

  /** @type {string | null} */
  let error = $state(null);
  let items = $state([]);
  let loading = $state(false);
  let processing = $state(null);

  async function fetchReviewQueue() {
    loading = true;
    error = null;
    try {
      const data = await reviewApi.list();
      items = data;
    } catch (e) {
      const msg = e && typeof e === 'object' && 'message' in e ? e.message : 'Failed to load review queue';
      error = /** @type {string} */ (msg);
      console.error('Failed to load review queue:', e);
    } finally {
      loading = false;
    }
  }

  /** @param {string} id */
  async function handleAccept(id) {
    processing = id;
    try {
      await reviewApi.accept(id);
      await fetchReviewQueue();
    } catch (e) {
      console.error('Accept failed:', e);
    } finally {
      processing = null;
    }
  }

  /** @param {string} id */
  async function handleReject(id) {
    processing = id;
    try {
      await reviewApi.reject(id);
      await fetchReviewQueue();
    } catch (e) {
      console.error('Reject failed:', e);
    } finally {
      processing = null;
    }
  }

  /** @param {{ canonical_title: string | undefined, suggested_title: string | undefined, filename: string }} item */
  function displayTitle(item) {
    return item.canonical_title || item.suggested_title || item.filename;
  }

  /** @param {number | undefined} confidence */
  function confidenceBadge(confidence) {
    if (!confidence) return null;
    const pct = Math.round(confidence * 100);
    if (pct >= 80) return 'success';
    if (pct >= 50) return 'warning';
    return 'default';
  }

  onMount(() => {
    fetchReviewQueue();
  });
</script>

<div class="panel" class:open>
  <div class="panel__header" onclick={() => onDriveClick(null)}>
    <span class="panel__title">Review</span>
    <span class="panel__toggle">
      {#if open}▼{:else}▶{/if}
    </span>
  </div>

  {#if open}
    <div class="panel__body">
      {#if loading}
        <div class="panel__loading">
          <div class="spinner"></div>
          <span>Loading review queue...</span>
        </div>
      {:else if error}
        <div class="panel__error">
          <span class="text-error">{error}</span>
          <button class="btn btn--sm btn--secondary mt-2" onclick={fetchReviewQueue}>Retry</button>
        </div>
      {:else if items.length === 0}
        <div class="panel__empty">
          <span class="empty__icon">✓</span>
          <div class="empty__title">None</div>
          <div class="empty__description">All items reviewed</div>
        </div>
      {:else}
        <div class="review-list">
          {#each items as item}
            <div class="card card--compact review-item">
              <div class="review-item__header">
                <div class="review-item__title" onclick={() => onDriveClick(item)} style="cursor: pointer;">
                  <span class="line-clamp-1">{displayTitle(item)}</span>
                </div>
                {#if item.confidence}
                  <span class="badge badge--{confidenceBadge(item.confidence)}">
                    {Math.round(item.confidence * 100)}%
                  </span>
                {/if}
              </div>
              <div class="review-item__meta">
                <span class="text-xs text-muted">{item.media_type}</span>
                {#if item.series_name}
                  <span class="text-xs text-muted">· {item.series_name}</span>
                {/if}
                {#if item.season_number}
                  <span class="text-xs text-muted">S{item.season_number}E{item.episode_number}</span>
                {/if}
              </div>
              <div class="review-item__actions">
                <button
                  class="btn btn--sm btn--secondary"
                  disabled={processing === item.id}
                  onclick={() => handleAccept(item.id)}
                >
                  Accept
                </button>
                <button
                  class="btn btn--sm btn--danger"
                  disabled={processing === item.id}
                  onclick={() => handleReject(item.id)}
                >
                  Reject
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .panel {
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    overflow: hidden;
  }

  .panel.open {
    border-color: var(--color-border-hover);
  }

  .panel__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-4) var(--space-5);
    cursor: pointer;
    user-select: none;
    transition: background var(--transition-fast);
  }

  .panel__header:hover {
    background: var(--color-bg-tertiary);
  }

  .panel__title {
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }

  .panel__toggle {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .panel__body {
    padding: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  .panel__loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-8) 0;
    color: var(--color-text-muted);
  }

  .panel__loading .spinner {
    width: 24px;
    height: 24px;
  }

  .panel__error {
    text-align: center;
    padding: var(--space-4) 0;
  }

  .panel__empty {
    text-align: center;
    padding: var(--space-8) 0;
  }

  .review-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .review-item {
    margin-bottom: 0;
  }

  .review-item__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .review-item__title {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--color-text-primary);
    flex: 1;
    min-width: 0;
  }

  .review-item__meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-bottom: var(--space-3);
  }

  .review-item__actions {
    display: flex;
    gap: var(--space-2);
  }

  @media (max-width: 768px) {
    .panel__body {
      padding: var(--space-3);
    }
  }
</style>
