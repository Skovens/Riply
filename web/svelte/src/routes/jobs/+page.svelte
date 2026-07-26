<script>
  import { onMount, onDestroy } from 'svelte';
  import { jobsApi } from '$lib/api';
  import { Job } from '$lib/types';

  let jobs = $state([]);
  let loading = $state(true);
  let pollInterval = null;

  onMount(() => {
    loadJobs();
    pollInterval = setInterval(loadJobs, 2000);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
  });

  async function loadJobs() {
    try {
      jobs = await jobsApi.list();
    } catch (e) {
      console.error('Failed to load jobs:', e);
    } finally {
      loading = false;
    }
  }

  async function handleCancel(id) {
    try {
      await jobsApi.cancel(id);
      await loadJobs();
    } catch (e) {
      console.error('Failed to cancel job:', e);
    }
  }

  function getStatusClass(status) {
    const classes = {
      Queued: 'default',
      Running: 'accent',
      Complete: 'success',
      Failed: 'error',
      Cancelled: 'warning',
    };
    return classes[status] || 'default';
  }

  function getIcon(status) {
    const icons = {
      Queued: '&#9202;',
      Running: '&#9881;',
      Complete: '&#10003;',
      Failed: '&#10007;',
      Cancelled: '&#9898;',
    };
    return icons[status] || '&#9881;';
  }

  function getProgress(job) {
    if (!job.progress_total || job.progress_total === 0) return 0;
    return Math.round((job.progress_processed / job.progress_total) * 100);
  }

  const hasActive = $derived(jobs.some(j => j.status === 'Queued' || j.status === 'Running'));
</script>

<section class="section">
  <div class="section__header">
    <h2 class="section__title">Jobs</h2>
    <button class="btn btn--secondary btn--sm" onclick={loadJobs}>Refresh</button>
  </div>

  {#if loading}
    <div class="empty">
      <div class="spinner"></div>
    </div>
  {:else if jobs.length === 0}
    <div class="empty">
      <div class="empty__icon">&#9881;</div>
      <div class="empty__title">No jobs yet</div>
      <div class="empty__description">Start a scan, analyze, or move from the header buttons</div>
    </div>
  {:else}
    <div class="jobs-list">
      {#each jobs as job}
        <div class="card card--compact" style="margin-bottom: var(--space-3);">
          <div class="flex items-center justify-between" style="margin-bottom: {job.progress_total > 0 ? 'var(--space-3)' : '0'};">
            <div class="flex items-center gap-3">
              <span class="job-icon" style="white-space: nowrap;">{@html getIcon(job.status)}</span>
              <div>
                <div class="font-medium">{job.kind}</div>
                <div class="text-sm text-muted">Job {job.id}</div>
              </div>
            </div>
            <div class="flex items-center gap-2">
              <span class="badge badge--{getStatusClass(job.status)}">{job.status}</span>
              {#if job.status === 'Queued' || job.status === 'Running'}
                <button class="btn btn--danger btn--sm" onclick={() => handleCancel(job.id)}>Cancel</button>
              {/if}
            </div>
          </div>
          {#if job.progress_total > 0}
            <div class="progress">
              <div class="progress__bar" style="width: {getProgress(job)}%"></div>
            </div>
            <div class="text-sm text-muted" style="margin-top: var(--space-2);">
              {job.progress_processed} / {job.progress_total} ({getProgress(job)}%)
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>
