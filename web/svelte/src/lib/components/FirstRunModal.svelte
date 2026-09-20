<script>
  import { onMount } from 'svelte';
  import { configApi } from '$lib/api';
  /** @type {import('$lib/types').Config | null} */
  let config = $state(null);
  let scan_directory = $state('');
  let movies_directory = $state('');
  let tv_directory = $state('');
  let saving = $state(false);
  let error = $state('');

  let { onConfirm, onClose } = $props();

  onMount(async () => {
    try {
      config = await configApi.get();
      scan_directory = config.scan_directory || '';
      movies_directory = config.movies_directory || '';
      tv_directory = config.tv_directory || '';
    } catch (/** @type {unknown} */ e) {
      console.error('Failed to load config:', e);
      error = 'Failed to load configuration.';
    }
  });

  function cancel() {
    onClose();
  }

  async function save() {
    if (!scan_directory.trim() || !movies_directory.trim() || !tv_directory.trim()) {
      error = 'All three paths are required.';
      return;
    }
    saving = true;
    error = '';
    try {
      await configApi.update({
        scan_directory: scan_directory.trim(),
        movies_directory: movies_directory.trim(),
        tv_directory: tv_directory.trim(),
      });
      onConfirm();
    } catch (/** @type {unknown} */ e) {
      console.error('Save failed:', e);
      error = 'Failed to save configuration.';
    } finally {
      saving = false;
    }
  }
</script>

<div class="modal-overlay" onclick={cancel}>
  <div class="first-run-modal" onclick={(e) => e.stopPropagation()}>
    <div class="modal__header">
      <h3 class="modal__title">Welcome to Riply</h3>
      <button class="modal__close" onclick={cancel}>&times;</button>
    </div>

    <div class="modal__body">
      <p style="color: var(--color-text-secondary); margin-bottom: var(--space-4);">
        Riply is ready to go. Configure your directories to get started:
      </p>

      {#if error}
        <div class="banner banner--warning" style="margin-bottom: var(--space-4);">
          <span class="banner__icon">⚠</span>
          <div class="banner__content">
            <div class="banner__title">Configuration Error</div>
            <div class="banner__text">{error}</div>
          </div>
        </div>
      {/if}

      <div style="display: flex; flex-direction: column; gap: var(--space-4);">
        <div>
          <label class="settings__label" for="scan-dir">Scan Directory</label>
          <input
            id="scan-dir"
            class="settings__input"
            placeholder="/path/to/scan"
            bind:value={scan_directory}
          />
        </div>

        <div>
          <label class="settings__label" for="movies-dir">Movies Directory</label>
          <input
            id="movies-dir"
            class="settings__input"
            placeholder="/path/to/movies"
            bind:value={movies_directory}
          />
        </div>

        <div>
          <label class="settings__label" for="tv-dir">TV Directory</label>
          <input
            id="tv-dir"
            class="settings__input"
            placeholder="/path/to/tv"
            bind:value={tv_directory}
          />
        </div>
      </div>
    </div>

    <div class="modal__footer">
      <button class="btn btn--secondary" onclick={cancel} disabled={saving}>
        Skip for now
      </button>
      <button class="btn btn--primary" onclick={save} disabled={saving}>
        {#if saving}
          Saving...
        {:else}
          Save & Continue
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .first-run-modal {
    width: 520px;
    max-width: calc(100vw - var(--space-6));
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-lg);
    z-index: calc(var(--z-modal) + 2);
    animation: firstRunIn var(--transition-normal) ease-out;
  }

  @keyframes firstRunIn {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>
