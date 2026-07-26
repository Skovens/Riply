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

  async function handleScan() {
    const defaultDir = '/home/morten';
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
