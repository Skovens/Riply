<script>
  import { onMount } from 'svelte';
  import { reviewApi, filesApi } from '$lib/api';
  import { FileRecord } from '$lib/types';

  let reviewQueue = $state([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      reviewQueue = await reviewApi.list();
    } catch (e) {
      console.error('Failed to load review:', e);
    } finally {
      loading = false;
    }
  });

  async function handleAccept(id) {
    const file = reviewQueue.find(f => f.id === id);
    if (!file) return;

    try {
      const updates = {
        canonical_title: file.canonical_title,
        canonical_year: file.canonical_year,
      };

      if (file.media_type === 'Tv') {
        updates.series_name = file.series_name;
        updates.season_number = file.season_number;
        updates.episode_number = file.episode_number;
        updates.episode_title = file.episode_title;
      }

      const hasChanges = Object.values(updates).some(v => v !== undefined && v !== null && v !== '');
      if (hasChanges) {
        await filesApi.update(id, updates);
      }

      await reviewApi.accept(id);
      reviewQueue = reviewQueue.filter(item => item.id !== id);
    } catch (e) {
      console.error('Failed to accept:', e);
    }
  }

  async function handleReject(id) {
    try {
      await reviewApi.reject(id);
      reviewQueue = reviewQueue.filter(item => item.id !== id);
    } catch (e) {
      console.error('Failed to reject:', e);
    }
  }

  function isTv(file) {
    return file.media_type === 'Tv';
  }
</script>

<section class="section">
  <div class="section__header">
    <h2 class="section__title">Review Queue</h2>
    <span class="badge badge--warning">{reviewQueue.length}</span>
  </div>

  {#if loading}
    <div class="empty"><div class="spinner"></div></div>
  {:else if reviewQueue.length === 0}
    <div class="empty">
      <div class="empty__icon">&#10003;</div>
      <div class="empty__title">All caught up!</div>
      <div class="empty__description">No files need review</div>
    </div>
  {:else}
    {#each reviewQueue as file}
      <div class="card card--compact" style="margin-bottom: var(--space-3);">
        <div class="flex items-start justify-between gap-4">
          <div class="flex-1 min-w-0">
            <div class="review-edit" style="margin-bottom: var(--space-2);">
              <input
                class="review-edit__input"
                bind:value={file.canonical_title}
                placeholder="Title"
                style="width: 100%;"
              />
              <div class="flex gap-2" style="margin-top: var(--space-1);">
                <input
                  class="review-edit__num"
                  bind:value={file.canonical_year}
                  placeholder="Year"
                  type="number"
                  style="width: 80px;"
                />
                {#if isTv(file)}
                  <input
                    class="review-edit__input"
                    bind:value={file.series_name}
                    placeholder="Series"
                    style="flex: 1;"
                  />
                  <input
                    class="review-edit__num"
                    bind:value={file.season_number}
                    placeholder="S"
                    type="number"
                    style="width: 60px;"
                  />
                  <input
                    class="review-edit__num"
                    bind:value={file.episode_number}
                    placeholder="E"
                    type="number"
                    style="width: 60px;"
                  />
                {/if}
              </div>
            </div>
            <div class="text-sm text-muted" style="margin-bottom: var(--space-2);">
              {file.filename}
            </div>
            <div class="flex gap-2" style="flex-wrap: wrap;">
              <span class="badge badge--default">{file.media_type}</span>
              {#if file.suggested_year}
                <span class="badge badge--default">{file.suggested_year}</span>
              {/if}
              {#if file.confidence}
                <span class="badge badge--default">{Math.round(file.confidence * 100)}% match</span>
              {/if}
            </div>
          </div>
          <div class="flex gap-2" style="flex-shrink: 0;">
            <button class="btn btn--primary btn--sm" onclick={() => handleAccept(file.id)}>Accept</button>
            <button class="btn btn--danger btn--sm" onclick={() => handleReject(file.id)}>Reject</button>
          </div>
        </div>
      </div>
    {/each}
  {/if}
</section>

<style>
  .review-edit__input {
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    padding: var(--space-1) var(--space-2);
    transition: border-color var(--transition-fast);
  }

  .review-edit__input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .review-edit__num {
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    padding: var(--space-1) var(--space-2);
    text-align: center;
    transition: border-color var(--transition-fast);
  }

  .review-edit__num:focus {
    outline: none;
    border-color: var(--accent);
  }
</style>
