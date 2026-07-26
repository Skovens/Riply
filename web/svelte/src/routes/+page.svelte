<script>
  import { onMount } from 'svelte';
  import { filesApi, reviewApi, moveApi, ripperApi } from '$lib/api';
  import { FileRecord, ReviewItem } from '$lib/types';

  let stats = $state({ scanned: 0, matched: 0, review: 0, confirmed: 0, moved: 0 });
  let drives = $state([]);
  let recentFiles = $state([]);
  let reviewQueue = $state([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      const [scanned, matched, reviewRes, moveRes] = await Promise.all([
        filesApi.list({ status: 'Scanned', per_page: 1 }),
        filesApi.list({ status: 'Matched', per_page: 1 }),
        reviewApi.list(),
        moveApi.status(),
      ]);

      stats.scanned = scanned.total || 0;
      stats.matched = matched.total || 0;
      stats.review = reviewRes.length || 0;
      stats.confirmed = moveRes.confirmed_count || 0;
      stats.moved = moveRes.moved_count || 0;
    } catch (e) {
      console.error('Failed to load stats:', e);
    }

    await Promise.all([loadDrives(), loadRecentFiles(), loadReviewQueue()]);
    loading = false;
  });

  async function loadDrives() {
    try {
      const result = await ripperApi.listDrives();
      drives = result || [];
    } catch (e) {
      console.error('Failed to load drives:', e);
    }
  }

  async function loadRecentFiles() {
    try {
      const result = await filesApi.list({ per_page: 5 });
      recentFiles = result.items || [];
    } catch (e) {
      console.error('Failed to load files:', e);
    }
  }

  async function loadReviewQueue() {
    try {
      const queue = await reviewApi.list();
      reviewQueue = queue.slice(0, 5);
    } catch (e) {
      console.error('Failed to load review:', e);
    }
  }

  async function handleAccept(id) {
    try {
      await reviewApi.accept(id);
      await loadReviewQueue();
    } catch (e) {
      console.error('Failed to accept:', e);
    }
  }

  async function handleReject(id) {
    try {
      await reviewApi.reject(id);
      await loadReviewQueue();
    } catch (e) {
      console.error('Failed to reject:', e);
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
</script>

{#if loading}
  <div class="empty" style="padding-top: 100px;">
    <div class="spinner" style="margin: 0 auto var(--space-4);"></div>
    <div class="empty__title">Loading...</div>
  </div>
{:else}
  <section class="section">
    <div class="section__header">
      <h2 class="section__title">Overview</h2>
    </div>
    <div class="grid grid--5" id="stats-grid">
      <div class="card stat">
        <div class="stat__value">{stats.scanned}</div>
        <div class="stat__label">Scanned</div>
      </div>
      <div class="card stat stat--accent">
        <div class="stat__value">{stats.matched}</div>
        <div class="stat__label">Matched</div>
      </div>
      <div class="card stat stat--warning">
        <div class="stat__value">{stats.review}</div>
        <div class="stat__label">Needs Review</div>
      </div>
      <div class="card stat stat--success">
        <div class="stat__value">{stats.confirmed}</div>
        <div class="stat__label">Confirmed</div>
      </div>
      <div class="card stat stat--accent">
        <div class="stat__value">{stats.moved}</div>
        <div class="stat__label">Moved</div>
      </div>
    </div>
  </section>

  <div class="grid grid--2">
    <section class="section">
      <div class="section__header">
        <h3 class="section__title">Drives</h3>
        <button class="btn btn--secondary btn--sm" onclick={loadDrives}>Refresh</button>
      </div>
      {#if drives.length === 0}
        <div class="empty">
          <div class="empty__icon">&#128190;</div>
          <div class="empty__title">No drives found</div>
          <div class="empty__description">Insert a disc or check your drive connection</div>
        </div>
      {:else}
        {#each drives as drive}
          <div class="card card--compact drive">
            <div class="drive__icon">&#128191;</div>
            <div class="drive__info">
              <div class="drive__name">{drive.name}</div>
              <div class="drive__disc">{drive.disc_name || 'No disc'}</div>
            </div>
            <div class="drive__actions">
              <span class="badge badge--{drive.state === 'Ready' ? 'success' : 'default'}">
                {drive.state}
              </span>
            </div>
          </div>
        {/each}
      {/if}
    </section>

    <section class="section">
      <div class="section__header">
        <h3 class="section__title">Recent Files</h3>
        <a href="/files" class="btn btn--secondary btn--sm">View All</a>
      </div>
      {#if recentFiles.length === 0}
        <div class="empty">
          <div class="empty__icon">&#128193;</div>
          <div class="empty__title">No files yet</div>
          <div class="empty__description">Run a scan to get started</div>
        </div>
      {:else}
        <div class="table" style="overflow-x: auto;">
          <table>
            <thead>
              <tr>
                <th>Filename</th>
                <th>Type</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each recentFiles as file}
                <tr>
                  <td class="truncate" style="max-width: 200px;">{file.filename}</td>
                  <td><span class="badge badge--default">{file.media_type}</span></td>
                  <td><span class="badge badge--{statusColor(file.status)}">{file.status}</span></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  </div>

  <section class="section">
    <div class="section__header">
      <h3 class="section__title">Review Queue</h3>
      <span class="badge badge--warning">{reviewQueue.length}</span>
    </div>
    {#if reviewQueue.length === 0}
      <div class="empty">
        <div class="empty__icon">&#10003;</div>
        <div class="empty__title">All caught up!</div>
        <div class="empty__description">No files need review</div>
      </div>
    {:else}
      {#each reviewQueue as file}
        <div class="card card--compact" style="margin-bottom: var(--space-2);">
          <div class="flex items-center justify-between">
            <div class="flex-1 min-w-0">
              <div class="font-medium truncate">{file.canonical_title || file.filename}</div>
              <div class="text-sm text-muted">{file.filename}</div>
            </div>
            <div class="flex gap-2">
              <button class="btn btn--primary btn--sm" onclick={() => handleAccept(file.id)}>Accept</button>
              <button class="btn btn--danger btn--sm" onclick={() => handleReject(file.id)}>Reject</button>
            </div>
          </div>
        </div>
      {/each}
    {/if}
  </section>
{/if}
