<script>
  let { text, content } = $props();

  let visible = $state(false);
  let tooltipRef = /** @type {import('svelte').HTMLElement} */ (null);
  let anchorRef = /** @type {import('svelte').HTMLElement} */ (null);
  let position = $state({ top: 0, left: 0 });

  function show() {
    visible = true;
    if (tooltipRef && anchorRef) {
      const rect = anchorRef.getBoundingClientRect();
      const tooltipRect = tooltipRef.getBoundingClientRect();
      position = {
        top: rect.top - tooltipRect.height - 8,
        left: rect.left + rect.width / 2 - tooltipRect.width / 2,
      };
    }
  }

  function hide() {
    visible = false;
  }

  function toggle() {
    visible = !visible;
    if (visible) {
      show();
    }
  }
</script>

<div
  class="tooltip-anchor"
  bind:this={anchorRef}
  onmouseenter={show}
  onmouseleave={hide}
  onfocus={show}
  onblur={hide}
>
  {#if content}
    {@render content()}
  {/if}
</div>

{#if visible}
  <div
    class="tooltip"
    bind:this={tooltipRef}
    style="top: {position.top}px; left: {position.left}px;"
    role="tooltip"
  >
    {text}
  </div>
{/if}

<style>
  .tooltip-anchor {
    display: inline;
  }

  .tooltip {
    position: fixed;
    z-index: 9999;
    padding: var(--space-1) var(--space-2);
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
    color: var(--color-text-primary);
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    animation: tooltip-fade-in var(--transition-fast) ease;
  }

  @keyframes tooltip-fade-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
