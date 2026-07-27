<script>
  import { onMount, onDestroy } from 'svelte';
  import { ripperApi } from '$lib/api';
  import { DriveInfo, InstallationStatus } from '$lib/types';

  let drives = $state([]);
  let status = $state(null);
  let loading = $state(true);
  let pollInterval = null;

  onMount(() => {
    loadStatus();
    loadDrives();
    pollInterval = setInterval(loadDrives, 3000);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
  });

  async function loadStatus() {
    try {
      status = await ripperApi.getStatus();
    } catch (e) {
      console.error('Failed to load installation status:', e);
    }
  }

  async function loadDrives() {
    try {
      drives = await ripperApi.listDrives();
    } catch (e) {
      console.error('Failed to load drives:', e);
    } finally {
      loading = false;
    }
  }

  async function handleStart(index) {
    try {
      await ripperApi.start(index, '');
      await loadDrives();
    } catch (e) {
      console.error('Failed to start rip:', e);
    }
  }

  async function handleCancel(index) {
    try {
      await ripperApi.cancel(index);
      await loadDrives();
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
            </div>
            <div class="drive__actions flex gap-2">
              <span class="badge badge--{getStateColor(drive.state)}">{drive.state}</span>
              {#if drive.state === 'Ready' && drive.disc_name}
                <button class="btn btn--primary btn--sm" onclick={() => handleStart(drive.index)}>Rip</button>
              {/if}
              {#if drive.state === 'Loading' || drive.state === 'Ripping'}
                <button class="btn btn--danger btn--sm" onclick={() => handleCancel(drive.index)}>Cancel</button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
