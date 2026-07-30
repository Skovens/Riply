<script>
  import { onMount, onDestroy } from 'svelte';
  import { jobsApi } from '$lib/api';

  let { open } = $props();

  /** @type {string | null} */
  let error = $state(null);
  let jobs = $state([]);
  let loading = $state(false);
  let intervalId = $state(0);

  const REFRESH_INTERVAL = 2000;

  async function fetchJobs() {
    loading = true;
    error = null;
    try {
      const data = await jobsApi.list();
      jobs = data.filter((j) => j.status === 'Queued' || j.status === 'Running');
    } catch (e) {
      const msg = e && typeof e === 'object' && 'message' in e ? e.message : 'Failed to load jobs';
      error = /** @type {string} */ (msg);
      console.error('Failed to load jobs:', e);
    } finally {
      loading = false;
    }
  }

  /** @param {string} id */
  async function cancelJob(id) {
    try {
      await jobsApi.cancel(id);
    } catch (e) {
      console.error('Cancel job failed:', e);
    } finally {
      await fetchJobs();
    }
  }

  /** @param {string} status */
  function statusBadge(status) {
    const map = {
      Queued: 'default',
      Running: 'accent',
      Complete: 'success',
      Failed: 'error',
    };
    return /** @type {keyof typeof map} */ (map[status]) || 'default';
  }

  /** @param {string} kind */
  function jobIcon(kind) {
    const k = kind.toLowerCase();
    if (k.includes('scan')) return '🔍';
    if (k.includes('analyze') || k.includes('match')) return '🧠';
    if (k.includes('move') || k.includes('rip')) return '💿';
    if (k.includes('review')) return '👀';
    return '📋';
  }

  /** @param {{ progress_total: number, progress_processed: number }} job */
  function progressPercent(job) {
    if (!job.progress_total || job.progress_total <= 0) return 0;
    return Math.round((job.progress_processed / job.progress_total) * 100);
  }

  /** @param {{ progress_processed: number, progress_total: number }} job */
  function isComplete(job) {
    return job.progress_processed >= job.progress_total && job.progress_total > 0;
  }

  function startPolling() {
    fetchJobs();
    intervalId = setInterval(fetchJobs, REFRESH_INTERVAL);
  }

  function stopPolling() {
    if (intervalId) {
      clearInterval(intervalId);
      intervalId = 0;
    }
  }

  onMount(() => {
    startPolling();
  });

  onDestroy(() => {
    stopPolling();
  });
</script>

<div class="panel" class:open>
  <div class="panel__header" onclick={() => {}}>
    <span class="panel__title">Jobs</span>
    <span class="panel__toggle">
      {#if open}▼{:else}▶{/if}
    </span>
  </div>

  {#if open}
    <div class="panel__body">
      {#if loading && jobs.length === 0}
        <div class="panel__loading">
          <div class="spinner"></div>
          <span>Loading...</span>
        </div>
      {:else if error}
        <div class="panel__error">
          <span class="text-error">{error}</span>
          <button class="btn btn--sm btn--secondary mt-2" onclick={fetchJobs}>Retry</button>
        </div>
      {:else if jobs.length === 0}
        <div class="panel__empty">
          <span class="empty__icon">✓</span>
          <div class="empty__title">No active jobs</div>
          <div class="empty__description">All jobs completed</div>
        </div>
      {:else}
        <div class="jobs-list">
          {#each jobs as job}
            <div class="card card--compact job-card">
              <div class="job-card__header">
                <div class="flex items-center gap-2">
                  <span class="job-icon">{jobIcon(job.kind)}</span>
                  <span class="text-sm font-medium">{job.kind}</span>
                </div>
                <span class="badge badge--{statusBadge(job.status)}">{job.status}</span>
              </div>
              <div class="job-card__progress">
                <div class="progress">
                  <div
                    class="progress__bar"
                    class:progress__bar--done={isComplete(job)}
                    style="width: {progressPercent(job)}%;"
                  ></div>
                </div>
                <span class="text-xs text-muted" style="margin-top: var(--space-1); display: block;">
                  {progressPercent(job)}%
                </span>
              </div>
              <div class="job-card__actions">
                {#if job.status === 'Running'}
                  <button
                    class="btn btn--sm btn--danger"
                    onclick={() => cancelJob(job.id)}
                  >
                    Cancel
                  </button>
                {/if}
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

  .job-card {
    margin-bottom: var(--space-3);
  }

  .job-card:last-child {
    margin-bottom: 0;
  }

  .job-card__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-3);
  }

  .job-card__progress {
    margin-bottom: var(--space-3);
  }

  .job-card__actions {
    display: flex;
    justify-content: flex-end;
  }

  @media (max-width: 768px) {
    .panel__body {
      padding: var(--space-3);
    }
  }
</style>
