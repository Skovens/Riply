<script>
  import { goto } from '$app/navigation';

  let { scanned = 0, matched = 0, review = 0, confirmed = 0, moved = 0 } = $props();

  /** @type {{ label: string, status: string, count: number, color: string }[]} */
  const stages = [
    { label: 'Scanned', status: 'scanned', count: scanned, color: 'accent' },
    { label: 'Matched', status: 'matched', count: matched, color: 'default' },
    { label: 'Needs Review', status: 'needsReview', count: review, color: 'warning' },
    { label: 'Confirmed', status: 'confirmed', count: confirmed, color: 'success' },
    { label: 'Moved', status: 'moved', count: moved, color: 'success' },
  ];

  /** @param {string} status */
  function navigate(status) {
    goto(`/files?status=${status}`);
  }
</script>

<div class="pipeline-progress">
  <div class="pipeline-progress__card">
    {#each stages as stage, i}
      <button
        class="pipeline-progress__stage"
        class:empty={stage.count === 0}
        onclick={() => navigate(stage.status)}
        title="View {stage.label} files"
      >
        <span class="pipeline-progress__label">{stage.label}</span>
        <span class="pipeline-progress__count badge badge--{stage.color}">
          {stage.count}
        </span>
      </button>
      {#if i < stages.length - 1}
        <span class="pipeline-progress__arrow" aria-hidden="true">→</span>
      {/if}
    {/each}
  </div>
</div>

<style>
  .pipeline-progress {
    margin-bottom: var(--space-4);
  }

  .pipeline-progress__card {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-4);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .pipeline-progress__card:hover {
    border-color: var(--color-border-hover);
    box-shadow: var(--shadow-md);
  }

  .pipeline-progress__stage {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: all var(--transition-fast);
    font-size: var(--text-sm);
    color: var(--color-text-primary);
  }

  .pipeline-progress__stage:hover {
    background: var(--color-bg-tertiary);
    border-color: var(--color-border);
  }

  .pipeline-progress__stage.empty {
    opacity: 0.4;
    cursor: default;
  }

  .pipeline-progress__stage.empty:hover {
    background: transparent;
    border-color: transparent;
  }

  .pipeline-progress__label {
    font-weight: 500;
  }

  .pipeline-progress__count {
    font-size: var(--text-xs);
  }

  .pipeline-progress__arrow {
    color: var(--color-text-muted);
    font-size: var(--text-lg);
    user-select: none;
  }
</style>
