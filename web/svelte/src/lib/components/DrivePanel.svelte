<script>
  import { onMount } from 'svelte';
  import { ripperApi } from '$lib/api';
  import { sseApi } from '$lib/api/sse';

  let { open, onDriveClick } = $props();

  /** @type {string | null} */
  let error = $state(null);
  let drives = $state([]);
  let loading = $state(false);
  let autoripDrives = $state([]);
  let rippingDriveIndex = $state(null);
  let eventSourceAvailable = $state(false);

  async function fetchDrives() {
    loading = true;
    error = null;
    try {
      const [drivesData, autorip] = await Promise.all([
        ripperApi.listDrives(),
        ripperApi.getAutoripStatus(),
      ]);
      drives = drivesData;
      autoripDrives = autorip;
    } catch (e) {
      const msg = e && typeof e === 'object' && 'message' in e ? e.message : 'Failed to load drives';
      error = /** @type {string} */ (msg);
      console.error('Failed to load drives:', e);
    } finally {
      loading = false;
    }
  }

  /** Refresh drives for a specific drive index */
  async function refreshDrive(index) {
    // Trigger a full drive list refresh (server re-scans all drives)
    await fetchDrives();
  }

  /** @param {number} index */
  async function toggleAutorip(index) {
    if (!eventSourceAvailable) {
      console.warn('Auto-rip disabled: no event source available');
      return;
    }
    try {
      autoripDrives = await ripperApi.toggleAutorip(index);
    } catch (e) {
      console.error('Failed to toggle auto-rip:', e);
    }
  }

  // Fetch event source status on mount
  async function fetchEventSourceStatus() {
    try {
      const status = await ripperApi.getEventSourceStatus();
      eventSourceAvailable = status.available;
    } catch (e) {
      console.error('Failed to fetch event source status:', e);
      eventSourceAvailable = false;
    }
  }

  // Subscribe to SSE events for auto-refresh
  onMount(() => {
    fetchDrives();
    fetchEventSourceStatus();
    sseApi.subscribe((event) => {
      if (event.kind === 'drive' && event.action === 'updated') {
        fetchDrives();
      }
      if (event.kind === 'rip' && (event.action === 'complete' || event.action === 'message')) {
        fetchDrives();
      }
    });
  });

  /** @param {number} index */
  async function startRip(index) {
    rippingDriveIndex = index;
    try {
      await ripperApi.start(index, '');
    } catch (e) {
      console.error('Rip failed:', e);
    } finally {
      rippingDriveIndex = null;
      await fetchDrives();
    }
  }

  /** @param {number} index */
  async function cancelRip(index) {
    try {
      await ripperApi.cancel(index);
    } catch (e) {
      console.error('Cancel failed:', e);
    } finally {
      await fetchDrives();
    }
  }

  /** @param {string} state */
  function driveBadgeState(state) {
    const map = {
      Empty: 'default',
      Loading: 'accent',
      Ready: 'success',
      Ripping: 'warning',
      Complete: 'success',
      Error: 'error',
    };
    return /** @type {keyof typeof map} */ (map[state]) || 'default';
  }

  /** @param {string} state */
  function driveIcon(state) {
    if (state === 'Loading' || state === 'Ripping') return '⏳';
    if (state === 'Ready') return '💿';
    if (state === 'Empty') return '⬜';
    if (state === 'Error') return '⚠️';
    if (state === 'Complete') return '✅';
    return '💿';
  }

  /** @param {{ state: string, disc_name: string | undefined, index: number }} drive */
  function canRip(drive) {
    return drive.state === 'Ready' && drive.disc_name && !driving(drive);
  }

  /** @param {{ state: string }} drive */
  function isDriving(drive) {
    return drive.state === 'Loading' || drive.state === 'Ripping';
  }

  /** @param {{ index: number }} drive */
  function driving(drive) {
    return rippingDriveIndex !== null && rippingDriveIndex === drive.index;
  }

  onMount(() => {
    fetchDrives();
  });
</script>

<div class="panel" class:open>
  <div class="panel__header" onclick={() => onDriveClick(null)}>
    <span class="panel__title">Drives</span>
    <span class="panel__toggle">
      {#if open}▼{:else}▶{/if}
    </span>
  </div>

  {#if open}
    <div class="panel__body">
      {#if loading}
        <div class="panel__loading">
          <div class="spinner"></div>
          <span>Querying drives...</span>
        </div>
      {:else if error}
        <div class="panel__error">
          <span class="text-error">{error}</span>
          <button class="btn btn--sm btn--secondary mt-2" onclick={fetchDrives}>Retry</button>
        </div>
      {:else if drives.length === 0}
        <div class="panel__empty">
          <span class="empty__icon">💿</span>
          <div class="empty__title">No drives found</div>
          <div class="empty__description">Connect a drive with disc to begin</div>
        </div>
      {:else}
        <div class="drives-list">
          {#each drives as drive}
            <div class="card card--compact drive-card">
              <div class="drive">
                <div class="drive__icon">{driveIcon(drive.state)}</div>
                <div class="drive__info">
                  <div class="drive__name">{drive.name}</div>
                  {#if drive.disc_name}
                    <div class="drive__disc" title={drive.disc_name}>{drive.disc_name}</div>
                  {:else}
                    <div class="drive__disc">No disc</div>
                  {/if}
                </div>
                <span class="badge badge--{driveBadgeState(drive.state)}">{drive.state}</span>
              </div>
              <div class="drive__actions">
                <div class="drive__action-row">
                  {#if isDriving(drive)}
                    <button
                      class="btn btn--danger btn--sm"
                      onclick={() => cancelRip(drive.index)}
                    >
                      Cancel
                    </button>
                  {:else if canRip(drive)}
                    <button
                      class="btn btn--primary btn--sm"
                      onclick={() => startRip(drive.index)}
                    >
                      Rip
                    </button>
                  {/if}
                  <button
                    class="btn btn--sm btn--secondary"
                    onclick={() => refreshDrive(drive.index)}
                    title="Refresh this drive"
                  >
                    ↻
                  </button>
                </div>
                <label class="toggle autorip-toggle" title={eventSourceAvailable ? "Auto-rip discs inserted in this drive" : "Auto-rip requires event source (udev/inotify)"}>
                  <input
                    type="checkbox"
                    checked={eventSourceAvailable && autoripDrives.includes(drive.index)}
                    disabled={!eventSourceAvailable}
                    onclick={() => toggleAutorip(drive.index)}
                  />
                  <span class="toggle__track"></span>
                </label>
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

  .drive-card {
    margin-bottom: var(--space-3);
  }

  .drive-card:last-child {
    margin-bottom: 0;
  }

  .drive__actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: var(--space-3);
    gap: var(--space-3);
  }

  .drive__action-row {
    display: flex;
    gap: var(--space-2);
  }

  .autorip-toggle {
    margin: 0;
  }

  @media (max-width: 768px) {
    .panel__body {
      padding: var(--space-3);
    }
  }
</style>
