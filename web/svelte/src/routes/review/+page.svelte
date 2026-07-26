<script>
  import { onMount } from 'svelte';
  import { reviewApi } from '$lib/api';
  import { ReviewItem } from '$lib/types';

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
    try {
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
            <div class="font-medium" style="margin-bottom: var(--space-1);">
              {file.canonical_title || file.suggested_title || file.filename}
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
