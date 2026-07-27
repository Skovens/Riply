<script>
  import { page } from '$app/stores';
  import '../app.css';

  let { children } = $props();

  const navItems = [
    { href: '/', label: 'Dashboard', view: 'dashboard' },
    { href: '/files', label: 'Files', view: 'files' },
    { href: '/review', label: 'Review', view: 'review' },
    { href: '/ripper', label: 'Ripper', view: 'ripper' },
    { href: '/jobs', label: 'Jobs', view: 'jobs' },
    { href: '/settings', label: 'Settings', view: 'settings' }
  ];

  function isActive(href) {
    if (href === '/') return $page.url.pathname === '/';
    return $page.url.pathname.startsWith(href);
  }

  let defaultDir = '';

  async function loadConfig() {
    try {
      const res = await fetch('/api/config');
      if (res.ok) {
        const data = await res.json();
        defaultDir = data.data?.scan_directory || '';
      }
    } catch (e) {
      console.error('Config load error:', e);
    }
  }

  loadConfig();

  async function handleScan() {
    const dir = window.prompt('Directory to scan:', defaultDir);
    if (!dir) return;
    try {
      const res = await fetch('/api/scan', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ directory: dir })
      });
      if (!res.ok) throw new Error(`Scan failed: ${res.status}`);
    } catch (e) {
      console.error('Scan error:', e);
    }
  }

  async function handleAnalyze() {
    try {
      const res = await fetch('/api/analyze', { method: 'POST' });
      if (!res.ok) throw new Error(`Analyze failed: ${res.status}`);
    } catch (e) {
      console.error('Analyze error:', e);
    }
  }

  let previewOpen = $state(false);
  let previewItems = $state([]);
  let previewLoading = $state(false);

  async function handlePreview() {
    previewLoading = true;
    previewOpen = true;
    try {
      const res = await fetch('/api/move/preview');
      if (res.ok) {
        const data = await res.json();
        previewItems = data.data || [];
      }
    } catch (e) {
      console.error('Preview error:', e);
    } finally {
      previewLoading = false;
    }
  }

  async function handleMove() {
    try {
      const res = await fetch('/api/move', { method: 'POST' });
      if (!res.ok) throw new Error(`Move failed: ${res.status}`);
    } catch (e) {
      console.error('Move error:', e);
    }
  }
</script>

<div class="app">
  <header class="header">
    <div class="header__inner">
      <div class="header__brand">
        <div class="header__logo">R</div>
        <h1 class="header__title">Riply</h1>
      </div>
      <nav class="header__nav">
        {#each navItems as item}
          <a
            class="header__nav-link"
            class:header__nav-link--active={isActive(item.href)}
            href={item.href}
          >
            {item.label}
          </a>
        {/each}
        <span class="header__nav-divider"></span>
        <button class="btn btn--secondary btn--sm" onclick={handleScan}>
          Scan
        </button>
        <button class="btn btn--secondary btn--sm" onclick={handleAnalyze}>
          Analyze
        </button>
        <button class="btn btn--secondary btn--sm" onclick={handlePreview}>
          Preview
        </button>
        <button class="btn btn--primary btn--sm" onclick={handleMove}>
          Move
        </button>
      </nav>
    </div>
  </header>

  <main class="main" id="main-content">
    {@render children()}
  </main>

  <footer class="footer">
    Riply &mdash; Media Management
  </footer>
</div>

{#if previewOpen}
  <div class="modal-overlay" onclick={() => previewOpen = false}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal__header">
        <h3 class="modal__title">Move Preview</h3>
        <button class="modal__close" onclick={() => previewOpen = false}>&times;</button>
      </div>
      <div class="modal__body">
        {#if previewLoading}
          <div class="empty"><div class="spinner"></div></div>
        {:else if previewItems.length === 0}
          <div class="empty">
            <div class="empty__icon">&#128193;</div>
            <div class="empty__title">No files to move</div>
            <div class="empty__description">No confirmed files found</div>
          </div>
        {:else}
          <div class="table" style="overflow-x: auto;">
            <table>
              <thead>
                <tr>
                  <th>File</th>
                  <th>Source</th>
                  <th>Destination</th>
                </tr>
              </thead>
              <tbody>
                {#each previewItems as item}
                  <tr>
                    <td class="truncate" style="max-width: 150px;">{item.filename}</td>
                    <td class="truncate" style="max-width: 200px;" title={item.source_path}>{item.source_path}</td>
                    <td class="truncate" style="max-width: 250px;" title={item.dest_path}>{item.dest_path}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <div class="text-sm text-muted" style="margin-top: var(--space-2);">
            {previewItems.length} file(s) will be moved
          </div>
        {/if}
      </div>
      <div class="modal__footer">
        <button class="btn btn--secondary" onclick={() => previewOpen = false}>Close</button>
      </div>
    </div>
  </div>
{/if}
