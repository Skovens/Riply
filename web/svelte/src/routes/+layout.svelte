<script>
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { connectSSE } from '$lib/api/sse';
  import { jobsApi } from '$lib/api';
  import '../app.css';

  let { children } = $props();

  const navItems = [
    { href: '/ripper', label: 'Ripper' },
    { href: '/organizer', label: 'Organizer' },
    { href: '/settings', label: 'Settings' }
  ];

  /** @param {string} href */
  function isActive(href) {
    if (href === '/') return $page.url.pathname === '/';
    return $page.url.pathname.startsWith(href);
  }

  let defaultDir = '';

  async function loadConfig() {
    try {
      const res = await fetch('/v1/config');
      if (res.ok) {
        const data = await res.json();
        defaultDir = data.data?.scan_directory || '';
      }
    } catch (e) {
      console.error('Config load error:', e);
    }
  }

  loadConfig();

  /** @type {string | null} */
  let activeJob = $state(null);
  let scanLoading = $state(false);
  let moveLoading = $state(false);

  /** @param {import('$lib/api/sse').SseEvent} event */
  function handleSSEEvent(event) {
    const { kind, action } = event;

    if (action === 'starting' || action === 'progress') {
      if (kind === 'scan') activeJob = 'scan';
      else if (kind === 'analyze') activeJob = 'analyze';
      else if (kind === 'move') activeJob = 'move';
    }

    if (action === 'complete' || action === 'cancelled') {
      if (kind === 'scan' || kind === 'analyze') activeJob = null;
      if (kind === 'move') activeJob = null;
    }
  }

  onMount(() => connectSSE(handleSSEEvent));

  async function handleScan() {
    if (activeJob) return;
    const dir = window.prompt('Directory to scan:', defaultDir);
    if (!dir) return;
    scanLoading = true;
    try {
      const res = await fetch('/v1/scan', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ directory: dir })
      });
      if (!res.ok) throw new Error(`Scan failed: ${res.status}`);
    } catch (e) {
      console.error('Scan error:', e);
    } finally {
      scanLoading = false;
    }
  }

  async function handleMove() {
    if (activeJob) return;
    moveLoading = true;
    try {
      const res = await fetch('/v1/move', { method: 'POST' });
      if (!res.ok) throw new Error(`Move failed: ${res.status}`);
    } catch (e) {
      console.error('Move error:', e);
    } finally {
      moveLoading = false;
    }
  }

  async function handleCancel() {
    if (!activeJob) return;
    try {
      const jobs = await jobsApi.list();
      const running = jobs.find(j => j.status === 'Running');
      if (running) {
        await jobsApi.cancel(running.id);
      }
    } catch (e) {
      console.error('Cancel error:', e);
    }
    activeJob = null;
    scanLoading = false;
    moveLoading = false;
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
        <button class="btn btn--secondary btn--sm" onclick={handleScan} disabled={scanLoading || moveLoading}>
          {#if scanLoading}
            Scan...
          {:else}
            Scan
          {/if}
        </button>
        <button class="btn btn--primary btn--sm" onclick={handleMove} disabled={scanLoading || moveLoading}>
          {#if moveLoading}
            Move...
          {:else}
            Move
          {/if}
        </button>
        {#if activeJob}
          <button class="btn btn--danger btn--sm" onclick={handleCancel}>
            Cancel
          </button>
        {/if}
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
