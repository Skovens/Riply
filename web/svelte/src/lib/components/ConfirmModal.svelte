<script>
  /** @type {string} */
  let { title, message, confirmText = 'Confirm', cancelText = 'Cancel', variant = 'default' } = $props();
  /** @type {() => void} */
  let { onConfirm, onCancel } = $props();

  function handleCancel() {
    onCancel();
  }

  function handleConfirm() {
    onConfirm();
  }
</script>

<div class="modal-overlay" onclick={handleCancel}>
  <div class="confirm-modal" onclick={(e) => e.stopPropagation()}>
    <div class="modal__header">
      <h3 class="modal__title">{title}</h3>
      <button class="modal__close" onclick={handleCancel}>&times;</button>
    </div>

    <div class="modal__body">
      <p>{message}</p>
    </div>

    <div class="modal__footer">
      <button class="btn btn--secondary" onclick={handleCancel}>{cancelText}</button>
      <button class="btn {variant === 'danger' ? 'btn--danger' : 'btn--primary'}" onclick={handleConfirm}>
        {confirmText}
      </button>
    </div>
  </div>
</div>

<style>
  .confirm-modal {
    width: 420px;
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
