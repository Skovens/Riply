<script>
  import { onMount, onDestroy } from 'svelte';
  import { ripperApi, configApi } from '$lib/api';
  import { connectSSE } from '$lib/api/sse';
  import { DriveInfo, InstallationStatus } from '$lib/types';

  let drives = $state([]);
  let status = $state(null);
  let config = $state(null);
  let autoripEnabled = $state(false);
  let excludedDrives = $state(new Set());
  let loading = $state(true);
  let ripStatus = $state({});
  let lastProgressTime = $state({});
  let pollInterval = null;
  let disconnectSSE = null;

  onMount(() => {
    loadStatus();
    loadConfig();
    loadDrives();
    startPolling();
    disconnectSSE = connectSSE(handleSSE);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (disconnectSSE) disconnectSSE();
  });

  function startPolling() {
    if (pollInterval) clearInterval(pollInterval);
    pollInterval = setInterval(loadDrives, 5000);
  }

  function stopPolling() {
    if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }

  function handleSSE(data) {
    if (!data.startsWith('rip:')) return;

    const parts = data.split(':');
    const action = parts[1];
    const index = parseInt(parts[2]);

    if (action === 'starting') {
      updateDriveState(index, 'Ripping');
      ripStatus[index] = { progress: 0, eta: null };
      lastProgressTime[index] = Date.now();
    } else if (action === 'progress') {
      const pct = parseInt(parts[3]);
      const eta = parts.length > 4 ? parseInt(parts[4]) : null;
      ripStatus[index] = { progress: pct, eta: eta || null };
      lastProgressTime[index] = Date.now();
      updateDriveState(index, 'Ripping');
    } else if (action === 'complete') {
      delete ripStatus[index];
      delete lastProgressTime[index];
      updateDriveState(index, 'Complete');
      loadDrives();
    } else if (action === 'error') {
      delete ripStatus[index];
      delete lastProgressTime[index];
      updateDriveState(index, 'Error');
      loadDrives();
    } else if (action === 'cancelled') {
      delete ripStatus[index];
      delete lastProgressTime[index];
      loadDrives();
    }
  }

  function updateDriveState(index, state) {
    drives = drives.map(d =>
      d.index === index ? { ...d, state } : d
    );
  }

  async function loadStatus() {
    try {
      status = await ripperApi.getStatus();
    } catch (e) {
      console.error('Failed to load installation status:', e);
    }
  }

  async function loadConfig() {
    try {
      config = await configApi.get();
      if (config?.ripping?.autorip_excluded_drives) {
        excludedDrives = new Set(config.ripping.autorip_excluded_drives);
      }
      if (config?.ripping?.autorip_enabled !== undefined) {
        autoripEnabled = config.ripping.autorip_enabled;
      }
    } catch (e) {
      console.error('Failed to load config:', e);
    }
  }

  async function toggleAutoripDrive(index) {
    if (excludedDrives.has(index)) {
      excludedDrives.delete(index);
    } else {
      excludedDrives.add(index);
    }
    try {
      await configApi.update({
        autorip_excluded_drives: [...excludedDrives],
      });
    } catch (e) {
      console.error('Failed to update autorip settings:', e);
      if (excludedDrives.has(index)) {
        excludedDrives.delete(index);
      } else {
        excludedDrives.add(index);
      }
    }
  }

  async function loadDrives() {
    try {
      const newDrives = await ripperApi.listDrives();
      drives = newDrives;
    } catch (e) {
      console.error('Failed to load drives:', e);
    } finally {
      loading = false;
    }
  }

  async function handleStart(index) {
    try {
      await ripperApi.start(index);
      stopPolling();
    } catch (e) {
      console.error('Failed to start rip:', e);
    }
  }

  async function handleCancel(index) {
    try {
      await ripperApi.cancel(index);
      loadDrives();
      startPolling();
    } catch (e) {
      console.error('Failed to cancel rip:', e);
    }
  }

  function getStateColor(state) {
    const colors = {
      Ready: 'success',
      Loading: 'accent',
      Ripping: 'accent',
      Complete: 'success',
      Error: 'error',
      Empty: 'default',
    };
    return colors[state] || 'default';
  }

  function missingLabel(component) {
    const labels = {
      Flatpak: 'Flatpak runtime',
      Flathub: 'Flathub remote',
      MakeMKV: 'MakeMKV',
      CdromGroup: 'cdrom group membership',
    };
    return labels[component] || component;
  }

  const hasActive = $derived(drives.some(d => d.state === 'Loading' || d.state === 'Ripping'));

  function formatRemaining(eta) {
    if (!eta || eta <= 0) return '';
    const m = Math.floor(eta / 60);
    const s = eta % 60;
    if (m > 0) return `~${m}m ${s}s left`;
    return `~${s}s left`;
  }

  function timeSince(ts) {
    if (!ts) return '';
    const secs = Math.floor((Date.now() - ts) / 1000);
    if (secs < 10) return '';
    if (secs < 60) return `${secs}s ago`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    if (m > 0 && s > 0) return `${m}m ${s}s ago`;
    if (m > 0) return `${m}m ago`;
    return `${secs}s ago`;
  }
</script>

{#if status && !status.installed}
  <div class="banner banner--warning" style="margin-bottom: var(--space-4);">
    <div class="banner__icon">&#9888;</div>
    <div class="banner__content">
      <div class="banner__title">MakeMKV is not installed</div>
      <div class="banner__text">
        Run <code>riply setup</code> in your terminal to install MakeMKV via Flatpak.
      </div>
      {#if status.missing.length > 0}
        <div class="banner__details">
          Missing: {status.missing.map(missingLabel).join(', ')}
        </div>
      {/if}
      {#if !status.drive_access_ok}
        <div class="banner__details banner__details--warn">
          Drive access requires cdrom group membership. The setup command can add this for you.
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if config && !config.rip_output_directory}
  <div class="banner banner--warning" style="margin-bottom: var(--space-4);">
    <div class="banner__icon">&#9888;</div>
    <div class="banner__content">
      <div class="banner__title">Rip output directory not set</div>
      <div class="banner__text">
        Go to <a href="/settings">Settings → Ripping</a> to configure where ripped files are saved.
      </div>
    </div>
  </div>
{/if}

<section class="section">
  <div class="section__header">
    <h2 class="section__title">Disc Drives</h2>
    <button class="btn btn--secondary btn--sm" onclick={loadDrives}>Refresh</button>
  </div>

  {#if loading}
    <div class="empty"><div class="spinner"></div></div>
  {:else if drives.length === 0}
    <div class="empty">
      <div class="empty__icon">&#128190;</div>
      <div class="empty__title">No drives found</div>
      <div class="empty__description">
        {#if status && !status.installed}
          Install MakeMKV with <code>riply setup</code> to detect drives.
        {:else}
          Insert a disc or check your drive connection.
        {/if}
      </div>
    </div>
  {:else}
    <div class="drives-list">
      {#each drives as drive}
        <div class="card card--compact" style="margin-bottom: var(--space-3);">
          <div class="drive">
            <div class="drive__icon">&#128191;</div>
            <div class="drive__info">
              <div class="drive__name">{drive.name}</div>
              <div class="drive__disc">{drive.disc_name || 'No disc inserted'}</div>
              {#if ripStatus[drive.index]}
                <div class="progress" style="margin-top: var(--space-2);">
                  <div class="progress__bar" style="width: {ripStatus[drive.index].progress}%"></div>
                </div>
                <div class="text-sm text-muted" style="margin-top: var(--space-1);">
                  {ripStatus[drive.index].progress}%
                  {#if formatRemaining(ripStatus[drive.index].eta)}
                    — {formatRemaining(ripStatus[drive.index].eta)}
                  {/if}
                  {#if timeSince(lastProgressTime[drive.index])}
                    — {timeSince(lastProgressTime[drive.index])}
                  {/if}
                </div>
              {/if}
            </div>
            <div class="drive__actions flex gap-2">
              <span class="badge badge--{getStateColor(drive.state)}">{drive.state}</span>
              {#if drive.state === 'Ready' && drive.disc_name}
                <button class="btn btn--primary btn--sm" onclick={() => handleStart(drive.index)}>Rip</button>
              {/if}
              {#if drive.state === 'Loading' || drive.state === 'Ripping'}
                <button class="btn btn--danger btn--sm" onclick={() => handleCancel(drive.index)}>Cancel</button>
              {/if}
              {#if autoripEnabled && drive.disc_name}
                <button
                  class="btn btn--sm"
                  style="background-color: {excludedDrives.has(drive.index) ? 'var(--color-muted)' : 'var(--color-primary)'}; color: white;"
                  onclick={() => toggleAutoripDrive(drive.index)}
                >
                  {excludedDrives.has(drive.index) ? '⊘' : '⊙'}
                </button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
