<script>
  import { onMount, onDestroy } from 'svelte';
  import { organizerApi } from '$lib/api';
  import { FolderRecord } from '$lib/types';

  let { folder, onClose, onAccept, onReject } = $props();

  // ── State ────────────────────────────────────────────────────
  let loading = $state(false);
  let error = $state(null);
  let destination = $state('');
  let expanded = $state(false);

  // ── Keyboard ─────────────────────────────────────────────────

  function handleKeydown(e) {
    if (e.key === 'Escape') {
      close();
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeydown);
    fetchPreview();
  });

  onDestroy(() => {
    document.removeEventListener('keydown', handleKeydown);
  });

  // ── Data ─────────────────────────────────────────────────────

  async function fetchPreview() {
    loading = true;
    error = null;
    try {
      const result = await organizerApi.previewMove(folder.id);
      destination = result.destination;
    } catch (e) {
      const msg = e && typeof e === 'object' && 'message' in e ? e.message : 'Failed to load preview';
      error = /** @type {string} */ (msg);
    } finally {
      loading = false;
    }
  }

  // ── Helpers ──────────────────────────────────────────────────

  /** Compute the destination path for a file within this folder */
  function fileDestination(filename) {
    if (folder.media_type === 'Tv') {
      return `${destination}${filename}`;
    }
    return `${destination}${filename}`;
  }

  /** Compute the destination for extra files (moved to Extras/ subfolder) */
  function extraDestination(filename) {
    return `${destination}Extras/${filename}`;
  }

  function formatConfidence(confidence) {
    if (!confidence) return null;
    const pct = Math.round(confidence * 100);
    if (pct >= 90) return { label: 'High', color: 'success' };
    if (pct >= 70) return { label: 'Medium', color: 'warning' };
    return { label: 'Low', color: 'error' };
  }

  // ── Actions ──────────────────────────────────────────────────

  function close() {
    onClose();
  }

  async function handleAccept() {
    try {
      await onAccept(folder.id);
    } catch (e) {
      console.error('Accept failed:', e);
    }
  }

  async function handleReject() {
    try {
      await onReject(folder.id);
    } catch (e) {
      console.error('Reject failed:', e);
    }
  }

  // ── File list ────────────────────────────────────────────────

  let fileDestinations = $derived([
    folder.main_file ? {
      name: folder.main_file,
      destination: fileDestination(folder.main_file),
      type: 'main',
    } : null,
    ...folder.extra_files.map(extra => ({
      name: extra,
      destination: extraDestination(extra),
      type: 'extra',
    }))
  ].filter(Boolean));

</script>

{#if folder}
  <div class="preview-modal-overlay" onclick={close}>
    <div class="preview-modal" onclick={(e) => e.stopPropagation()}>
      <!-- Header -->
      <div class="preview-modal__header">
        <div>
          <h3 class="preview-modal__title">Preview Changes</h3>
          <p class="preview-modal__subtitle">Review before moving</p>
        </div>
        <button class="preview-modal__close" onclick={close}>&times;</button>
      </div>

      <!-- Body -->
      <div class="preview-modal__body">
        {#if loading}
          <div class="preview-modal__loading">
            <div class="spinner"></div>
            <span>Calculating destination...</span>
          </div>
        {:else if error}
          <div class="preview-modal__error">
            <span>{error}</span>
            <button class="btn btn--sm btn--secondary" onclick={fetchPreview}>Retry</button>
          </div>
        {:else}
          <!-- Basic info -->
          <div class="preview-modal__info">
            <div class="preview-modal__info-row">
              <span class="preview-modal__label">Source</span>
              <span class="preview-modal__value preview-modal__value--mono" title={folder.path}>{folder.path}</span>
            </div>
            <div class="preview-modal__arrow">↓</div>
            <div class="preview-modal__info-row">
              <span class="preview-modal__label">Destination</span>
              <span class="preview-modal__value preview-modal__value--mono" title={destination}>{destination}</span>
            </div>
          </div>

          <!-- Suggested title -->
          {#if folder.suggested_name}
            <div class="preview-modal__suggestion">
              <span class="preview-modal__suggestion-label">Suggested name:</span>
              <span class="preview-modal__suggestion-value">
                {folder.suggested_name}{#if folder.suggested_year} ({folder.suggested_year}){/if}
              </span>
            </div>
          {/if}

          <!-- Confidence -->
          {#if folder.confidence}
            {#if folder.confidence >= 0.9}
              <div class="preview-modal__confidence">
                <span class="preview-modal__confidence-label">Confidence:</span>
                <span class="badge badge--success">High ({Math.round(folder.confidence * 100)}%)</span>
              </div>
            {:else if folder.confidence >= 0.7}
              <div class="preview-modal__confidence">
                <span class="preview-modal__confidence-label">Confidence:</span>
                <span class="badge badge--warning">Medium ({Math.round(folder.confidence * 100)}%)</span>
              </div>
            {:else}
              <div class="preview-modal__confidence">
                <span class="preview-modal__confidence-label">Confidence:</span>
                <span class="badge badge--error">Low ({Math.round(folder.confidence * 100)}%)</span>
              </div>
            {/if}
          {/if}

          <!-- File count -->
          <div class="preview-modal__file-count">
            {folder.file_count} file{folder.file_count !== 1 ? 's' : ''} to move
          </div>

          <!-- Expandable file list -->
          {#if folder.main_file || folder.extra_files.length > 0}
            <button
              class="preview-modal__expand-btn"
              onclick={() => expanded = !expanded}
            >
              <span class="preview-modal__expand-icon" class:expanded={expanded}>▾</span>
              <span>{expanded ? 'Hide' : 'Show'} file details</span>
            </button>

            {#if expanded}
              <div class="preview-modal__file-list">
                {#if folder.main_file}
                  <div class="preview-modal__file-item">
                    <span class="preview-modal__file-icon">🎬</span>
                    <span class="preview-modal__file-name">{folder.main_file}</span>
                    <span class="preview-modal__file-arrow">→</span>
                    <span class="preview-modal__file-dest" title={fileDestination(folder.main_file)}>
                      {fileDestination(folder.main_file)}
                    </span>
                  </div>
                {/if}
                {#each folder.extra_files as extra}
                  <div class="preview-modal__file-item preview-modal__file-item--extra">
                    <span class="preview-modal__file-icon">📎</span>
                    <span class="preview-modal__file-name">{extra}</span>
                    <span class="preview-modal__file-arrow">→</span>
                    <span class="preview-modal__file-dest" title={extraDestination(extra)}>
                      Extras/{extra}
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
          {/if}
        {/if}
      </div>

      <!-- Footer -->
      <div class="preview-modal__footer">
        <button class="btn btn--secondary" onclick={close}>Cancel</button>
        <button class="btn btn--warning" onclick={handleReject} disabled={loading}>
          Reject
        </button>
        <button class="btn btn--primary" onclick={handleAccept} disabled={loading}>
          Accept
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .preview-modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--z-modal);
    padding: var(--space-4);
    animation: fadeIn 150ms ease-out;
  }

  .preview-modal {
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    width: 100%;
    max-width: 640px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-lg);
    animation: scaleIn 200ms ease-out;
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes scaleIn {
    from {
      opacity: 0;
      transform: scale(0.95) translateY(10px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  /* ── Header ─────────────────────────────────────────────── */
  .preview-modal__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-border);
  }

  .preview-modal__title {
    font-size: var(--text-lg);
    font-weight: 600;
    margin: 0;
  }

  .preview-modal__subtitle {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
    margin: var(--space-1) 0 0;
  }

  .preview-modal__close {
    background: none;
    border: none;
    font-size: var(--text-2xl);
    color: var(--color-text-muted);
    cursor: pointer;
    padding: var(--space-1);
    line-height: 1;
    flex-shrink: 0;
  }

  .preview-modal__close:hover {
    color: var(--color-text-primary);
  }

  /* ── Body ───────────────────────────────────────────────── */
  .preview-modal__body {
    padding: var(--space-5);
    overflow-y: auto;
    flex: 1;
  }

  /* ── Loading / Error ────────────────────────────────────── */
  .preview-modal__loading,
  .preview-modal__error {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-8) 0;
    text-align: center;
  }

  .preview-modal__loading .spinner {
    width: 24px;
    height: 24px;
  }

  .preview-modal__error .btn {
    margin-top: var(--space-2);
  }

  /* ── Info (source → destination) ────────────────────────── */
  .preview-modal__info {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-4);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-lg);
    margin-bottom: var(--space-4);
  }

  .preview-modal__info-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
  }

  .preview-modal__label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    min-width: 80px;
  }

  .preview-modal__value {
    font-size: var(--text-sm);
    color: var(--color-text-primary);
    word-break: break-all;
  }

  .preview-modal__value--mono {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
  }

  .preview-modal__arrow {
    font-size: var(--text-lg);
    color: var(--color-text-muted);
  }

  /* ── Suggestion ─────────────────────────────────────────── */
  .preview-modal__suggestion {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }

  .preview-modal__suggestion-label {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  .preview-modal__suggestion-value {
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-primary);
  }

  /* ── Confidence ─────────────────────────────────────────── */
  .preview-modal__confidence {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-3);
  }

  .preview-modal__confidence-label {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  /* ── File count ─────────────────────────────────────────── */
  .preview-modal__file-count {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
    margin-bottom: var(--space-3);
  }

  /* ── Expand button ──────────────────────────────────────── */
  .preview-modal__expand-btn {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .preview-modal__expand-btn:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text-primary);
  }

  .preview-modal__expand-icon {
    display: inline-block;
    transition: transform var(--transition-fast);
    font-size: var(--text-xs);
  }

  .preview-modal__expand-icon.expanded {
    transform: rotate(180deg);
  }

  /* ── File list ──────────────────────────────────────────── */
  .preview-modal__file-list {
    margin-top: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .preview-modal__file-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
  }

  .preview-modal__file-icon {
    flex-shrink: 0;
  }

  .preview-modal__file-name {
    flex: 1;
    min-width: 0;
    color: var(--color-text-primary);
    font-family: var(--font-mono);
  }

  .preview-modal__file-arrow {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .preview-modal__file-dest {
    color: var(--color-accent);
    font-family: var(--font-mono);
    text-align: right;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 240px;
  }

  .preview-modal__file-item--extra {
    padding-left: calc(var(--space-3) * 2);
  }

  /* ── Footer ─────────────────────────────────────────────── */
  .preview-modal__footer {
    padding: var(--space-4) var(--space-5);
    border-top: 1px solid var(--color-border);
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  /* ── Responsive ─────────────────────────────────────────── */
  @media (max-width: 512px) {
    .preview-modal {
      max-width: 100vw;
      margin: var(--space-2);
      max-height: calc(100vh - var(--space-4));
    }

    .preview-modal__body {
      padding: var(--space-4);
    }

    .preview-modal__file-dest {
      max-width: 120px;
    }
  }
</style>
