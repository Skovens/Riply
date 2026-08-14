<script>
  import { onMount, onDestroy } from 'svelte';
  import { filesApi, reviewApi, ripperApi, jobsApi, moveApi, scanApi, configApi, openMediaNamingModal, titleGroupsApi } from '$lib/api';
  import { connectSSE } from '$lib/api/sse';
  import { parseEvent, ripEventIndex, ripProgressFields, ripMessageText } from '$lib/api/parse-event';
  import DetailDrawer from '$lib/components/DetailDrawer.svelte';

  // ── Sidebar ────────────────────────────────────────────────
  let sidebarCollapsed = $state(false);
  let sidebarPanel = $state({ drives: true, review: true, jobs: true });

  // ── Stats ──────────────────────────────────────────────────
  let stats = $state({ scanned: 0, matched: 0, review: 0, confirmed: 0, moved: 0 });
  let statsLoading = $state(true);

  // ── Pipeline counts ────────────────────────────────────────
  let pipeline = $state({ scanned: 0, matched: 0, review: 0, confirmed: 0, moved: 0, total: 0 });

  // ── Drives ─────────────────────────────────────────────────
  let drives = $state([]);
  let drivesLoading = $state(true);
  let ripOutputDir = $state('');
  let autoripDrives = $state([]);

  // ── Drive reordering (drag-and-drop) ──────────────────────
  let driveOrder = $state(() => {
    try {
      const saved = localStorage.getItem('riply:driveOrder');
      return saved ? JSON.parse(saved) : [];
    } catch {
      return [];
    }
  });
  let dragIndex = $state(null);
  let dropIndex = $state(null);

  // ── Rip progress ───────────────────────────────────────────
  /** @type {Record<number, { pct: number, eta: number | null, stage: string, doneAt?: number }>} */
  let ripProgress = $state({});

  // ── Files ──────────────────────────────────────────────────
  let files = $state([]);
  let filesLoading = $state(true);

  // ── Review ─────────────────────────────────────────────────
  let reviewQueue = $state([]);
  let reviewLoading = $state(true);

  // ── Jobs ───────────────────────────────────────────────────
  let jobs = $state([]);
  let jobsLoading = $state(true);

  // ── DetailDrawer ───────────────────────────────────────────
  let selectedFile = $state(null);
  let drawerOpen = $state(false);

  // ── Toasts ─────────────────────────────────────────────────
  let toasts = $state([]);

  // ── SSE connection ─────────────────────────────────────────
  let sseDisconnect = null;

  // ── Derived ────────────────────────────────────────────────
  let activeJobs = $derived(jobs.filter(j => j.status === 'Queued' || j.status === 'Running'));

  // ── Lifecycle ──────────────────────────────────────────────

  onMount(async () => {
    // Smart polling: if drives are cached in localStorage and no rips are active,
    // reuse them instead of making a redundant API call.
    let drivesCached = false;
    try {
      const cached = localStorage.getItem('riply:drives');
      if (cached) {
        const parsed = JSON.parse(cached);
        const hasActiveRip = parsed.some(d => d.state === 'Ripping');
        if (!hasActiveRip) {
          drives = parsed;
          drivesCached = true;
        }
      }
    } catch { /* ignore */ }

    await Promise.all([
      fetchStats(),
      drivesCached ? Promise.resolve() : fetchDrives(),
      fetchAutoripStatus(),
      fetchFiles(),
      fetchReviewQueue(),
      fetchJobs(),
      fetchConfig(),
    ]);
    applyDriveOrder();
    sseDisconnect = connectSSE(handleSSEEvent);
  });

  onDestroy(() => {
    sseDisconnect?.();
  });

  // ── Data fetching ──────────────────────────────────────────

  async function fetchStats() {
    try {
      const [scanned, matched, reviewRes, moveRes] = await Promise.all([
        filesApi.list({ status: 'Scanned', per_page: 1 }),
        filesApi.list({ status: 'Matched', per_page: 1 }),
        reviewApi.list(),
        moveApi.status(),
      ]);

      stats.scanned = scanned.total || 0;
      stats.matched = matched.total || 0;
      stats.review = reviewRes.length || 0;
      stats.confirmed = moveRes.confirmed_count || 0;
      stats.moved = moveRes.moved_count || 0;

      // Pipeline counts
      pipeline.scanned = stats.scanned;
      pipeline.matched = stats.matched;
      pipeline.review = stats.review;
      pipeline.confirmed = stats.confirmed;
      pipeline.moved = stats.moved;
      pipeline.total = stats.scanned + stats.matched + stats.review + stats.confirmed + stats.moved;

      statsLoading = false;
    } catch (e) {
      console.error('Failed to load stats:', e);
      statsLoading = false;
    }
  }

  async function fetchDrives() {
    try {
      const result = await ripperApi.listDrives();
      const newDrives = result || [];
      // Drop stale rip progress for drives that are no longer ripping
      // and whose doneAt is older than 5 seconds (grace period).
      const now = Date.now();
      const staleKeys = Object.keys(ripProgress).filter(key => {
        const drive = newDrives.find(d => d.index === Number(key));
        if (!drive) return true;
        if (drive.state === 'Ripping') return false;
        const prog = ripProgress[Number(key)];
        return prog && prog.doneAt && now - prog.doneAt > 5000;
      });
      if (staleKeys.length > 0) {
        const cleaned = { ...ripProgress };
        for (const key of staleKeys) {
          delete cleaned[key];
        }
        ripProgress = cleaned;
      }
      drives = newDrives;
      // Cache drives in localStorage for smart polling on refresh
      try {
        localStorage.setItem('riply:drives', JSON.stringify(newDrives));
      } catch { /* ignore */ }
    } catch (e) {
      console.error('Failed to load drives:', e);
    } finally {
      drivesLoading = false;
    }
  }

  /** Apply saved drive order to the drives array. */
  function applyDriveOrder() {
    if (driveOrder.length === 0) return;
    const sorted = [...drives].sort((a, b) => {
      const aIdx = driveOrder.indexOf(a.index);
      const bIdx = driveOrder.indexOf(b.index);
      // Drives not in the order list go to the end
      if (aIdx === -1 && bIdx === -1) return 0;
      if (aIdx === -1) return 1;
      if (bIdx === -1) return -1;
      return aIdx - bIdx;
    });
    drives = sorted;
  }

  /** Save current drive order to localStorage. */
  function saveDriveOrder() {
    driveOrder = drives.map(d => d.index);
    try {
      localStorage.setItem('riply:driveOrder', JSON.stringify(driveOrder));
    } catch { /* ignore */ }
  }

  /** Reset drive order to backend default. */
  function resetDriveOrder() {
    driveOrder = [];
    try {
      localStorage.removeItem('riply:driveOrder');
    } catch { /* ignore */ }
  }

  async function fetchAutoripStatus() {
    try {
      autoripDrives = await ripperApi.getAutoripStatus();
    } catch (e) {
      console.error('Failed to load autorip status:', e);
    }
  }

  async function fetchFiles() {
    try {
      const result = await filesApi.list({ per_page: 50 });
      files = result.items || [];
    } catch (e) {
      console.error('Failed to load files:', e);
    } finally {
      filesLoading = false;
    }
  }

  async function fetchReviewQueue() {
    try {
      const queue = await reviewApi.list();
      reviewQueue = queue || [];
    } catch (e) {
      console.error('Failed to load review:', e);
    } finally {
      reviewLoading = false;
    }
  }

  async function fetchJobs() {
    try {
      const jobList = await jobsApi.list();
      jobs = jobList || [];
    } catch (e) {
      console.error('Failed to load jobs:', e);
    } finally {
      jobsLoading = false;
    }
  }

  async function fetchConfig() {
    try {
      const config = await configApi.get();
      ripOutputDir = config.rip_output_directory || '';
    } catch (e) {
      console.error('Failed to load config:', e);
    }
  }

  // ── SSE handler ───────────────────────────────────────────

  function handleSSEEvent(data) {
    const event = parseEvent(data);
    if (!event) return;

    const { kind, action, detail } = event;

    // Rip events: progress bar, stage messages, toasts, and auto-scan
    if (kind === 'rip') {
      const index = ripEventIndex(detail);
      if (action === 'starting') {
        if (!Number.isNaN(index)) {
          ripProgress = { ...ripProgress, [index]: { pct: 0, eta: null, stage: 'Getting disc info…' } };
          markDriveRipping(index);
        }
        showToast('Rip started', 'success');
      } else if (action === 'message') {
        const msg = ripMessageText(detail);
        if (!Number.isNaN(index)) {
          const cur = ripProgress[index];
          ripProgress = { ...ripProgress, [index]: { pct: cur?.pct ?? 0, eta: cur?.eta ?? null, stage: msg } };
        }
      } else if (action === 'progress') {
        const fields = ripProgressFields(detail);
        if (!Number.isNaN(fields.index)) {
          ripProgress = { ...ripProgress, [fields.index]: { pct: fields.pct, eta: fields.eta, stage: 'Ripping…' } };
          markDriveRipping(fields.index);
        }
      } else if (action === 'complete') {
        if (!Number.isNaN(index)) {
          ripProgress = { ...ripProgress, [index]: { pct: 100, eta: null, stage: 'Rip complete!', doneAt: Date.now() } };
        }
        showToast('Rip complete', 'success');
        // Auto-scan after rip completes (not when it starts)
        if (ripOutputDir) {
          setTimeout(() => {
            scanApi.start(ripOutputDir).catch(e => console.error('Auto-scan failed:', e));
          }, 2000);
        }
      } else if (action === 'error') {
        if (!Number.isNaN(index)) {
          ripProgress = { ...ripProgress, [index]: { pct: 0, eta: null, stage: 'Rip failed', doneAt: Date.now() } };
        }
        showToast('Rip failed', 'error');
      }
      return;
    }

    // Drive events: handle insertion, ejection, removal, or full state change
    if (kind === 'drive') {
      const index = parseInt(detail, 10);
      if (Number.isNaN(index)) return;
      if (action === 'inserted' || action === 'ejected' || action === 'removed') {
        fetchDrives();
      } else if (action === 'changed') {
        fetchDrives();
      }
      return;
    }

    // Job progress events
    if (kind === 'scan' || kind === 'analyze' || kind === 'move') {
      if (action === 'progress' || action === 'starting') {
        fetchJobs();
      } else if (action === 'complete' || action === 'cancelled') {
        fetchJobs();
        if (kind === 'scan') fetchFiles(); // Refresh file list after scan completes
      }
      return;
    }

    // Review events
    if (kind === 'review' && (action === 'accepted' || action === 'rejected')) {
      fetchReviewQueue();
      fetchStats();
      return;
    }
  }

  // ── Toasts ─────────────────────────────────────────────────

  function showToast(message, type = 'success') {
    const id = Date.now();
    toasts = [...toasts, { id, message, type }];
    setTimeout(() => {
      toasts = toasts.filter(t => t.id !== id);
    }, 4000);
  }

  // ── Actions ────────────────────────────────────────────────

  async function handleAccept(id) {
    try {
      await reviewApi.accept(id);
      reviewQueue = reviewQueue.filter(r => r.id !== id);
      stats.review = Math.max(0, stats.review - 1);
      stats.confirmed = stats.confirmed + 1;
      pipeline.review = Math.max(0, pipeline.review - 1);
      pipeline.confirmed = pipeline.confirmed + 1;
      pipeline.total = pipeline.total;
      fetchFiles(); // Refresh file list
      showToast('File accepted', 'success');
    } catch (e) {
      console.error('Failed to accept:', e);
      showToast('Failed to accept', 'error');
    }
  }

  async function handleReject(id) {
    try {
      await reviewApi.reject(id);
      reviewQueue = reviewQueue.filter(r => r.id !== id);
      stats.review = Math.max(0, stats.review - 1);
      fetchFiles(); // Refresh file list
      showToast('File rejected', 'success');
    } catch (e) {
      console.error('Failed to reject:', e);
      showToast('Failed to reject', 'error');
    }
  }

  /** @param {number} index */
  function markDriveRipping(index) {
    const drive = drives.find(d => d.index === index);
    if (drive) drive.state = 'Ripping';
  }

  /** Parse "Title (2020)" → { title: "Title", year: 2020 }. */
  function parseDiscName(discName) {
    const m = discName.match(/^(.*?)\s*\((\d{4})\)\s*$/);
    if (m) return { title: m[1].trim(), year: parseInt(m[2], 10) };
    return { title: discName.trim(), year: null };
  }

  /** @param {number} seconds */
  function formatEta(seconds) {
    if (!seconds || seconds <= 0) return '';
    const m = Math.floor(seconds / 60);
    const s = Math.round(seconds % 60);
    if (m >= 60) {
      const h = Math.floor(m / 60);
      return `${h}h ${m % 60}m`;
    }
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  }

  /** @param {number} index */
  function ripStage(index) {
    return ripProgress[index]?.stage || '';
  }
  /** @param {number} index */
  function ripPct(index) {
    return ripProgress[index]?.pct ?? 0;
  }
  /** @param {number} index */
  function ripLabel(index) {
    const p = ripProgress[index];
    if (!p) return '';
    let label = `${p.pct}%`;
    if (p.eta && p.eta > 0 && p.pct < 100) label += ` · ~${formatEta(p.eta)} left`;
    return label;
  }

  /** @param {number} index */
  async function handleRip(index) {
    const drive = drives.find(d => d.index === index);
    const discName = (drive && drive.disc_name) || '';
    const parsed = parseDiscName(discName);
    if (parsed.title) {
      // Ask for a name before ripping; prefill from the disc name (ADR-022: prefill, not commit).
      const choice = await openMediaNamingModal({
        mediaType: 'movie',
        suggest: async () => ({ title: parsed.title, year: parsed.year }),
      });
      if (!choice) return;
      const folderName = choice.year ? `${choice.title} (${choice.year})` : choice.title;
      try {
        await ripperApi.start(index, `${ripOutputDir}/${folderName}`);
        return;
      } catch (e) {
        console.error('Failed to start rip:', e);
        showToast('Failed to start rip', 'error');
        return;
      }
    }
    try {
      await ripperApi.start(index, ripOutputDir);
    } catch (e) {
      console.error('Failed to start rip:', e);
      showToast('Failed to start rip', 'error');
    }
  }

  /** @param {number} index */
  async function handleNameDisc(index) {
    const drive = drives.find(d => d.index === index);
    if (!drive || !drive.disc_name) return;

    const parsed = parseDiscName(drive.disc_name);
    const choice = await openMediaNamingModal({
      mediaType: 'movie',
      initialTitle: parsed.title || drive.disc_name,
      suggest: parsed.title
        ? async () => ({ title: parsed.title, year: parsed.year })
        : undefined,
    });

    if (!choice) return;

    try {
      await ripperApi.rename(index, choice);
      // Refresh drives to pick up the new derived_name
      await fetchDrives();
    } catch (e) {
      console.error('Failed to rename disc:', e);
      showToast('Failed to set disc name', 'error');
    }
  }

  async function handleCancelRip(index) {
    try {
      await ripperApi.cancel(index);
    } catch (e) {
      console.error('Failed to cancel rip:', e);
    }
  }

  /** @param {number} index */
  async function handleEject(index) {
    try {
      await ripperApi.eject(index);
      showToast('Disc ejected', 'success');
      await fetchDrives();
    } catch (e) {
      console.error('Failed to eject:', e);
      showToast('Failed to eject', 'error');
    }
  }

  async function handleToggleAutorip(index) {
    try {
      autoripDrives = await ripperApi.toggleAutorip(index);
    } catch (e) {
      console.error('Failed to toggle auto-rip:', e);
    }
  }

  async function handleCancelJob(id) {
    try {
      await jobsApi.cancel(id);
      jobs = jobs.filter(j => j.id !== id);
    } catch (e) {
      console.error('Failed to cancel job:', e);
    }
  }

  function handleSelectFile(file) {
    selectedFile = file;
    drawerOpen = true;
  }

  function handleDrawerClose() {
    drawerOpen = false;
    selectedFile = null;
  }

  async function handleDrawerConfirm(id, data) {
    try {
      await filesApi.update(id, data);
      await fetchReviewQueue();
      await fetchStats();
      handleDrawerClose();
    } catch (e) {
      console.error('Failed to confirm file:', e);
    }
  }

  async function handleDrawerReject(id) {
    try {
      await reviewApi.reject(id);
      await fetchReviewQueue();
      await fetchStats();
      handleDrawerClose();
    } catch (e) {
      console.error('Failed to reject file:', e);
    }
  }

  // ── Media naming (ADR-022) ─────────────────────────────────

  /** Open the shared naming modal and apply the choice to a review item. */
  async function handleNameReviewItem(item) {
    const choice = await openMediaNamingModal({
      initialTitle: item.canonical_title || item.suggested_title || item.filename,
      mediaType: item.media_type === 'Tv' ? 'tv' : 'movie',
    });
    if (!choice) return;
    try {
      await filesApi.update(item.id, {
        canonical_title: choice.title,
        canonical_year: choice.year,
        media_type: choice.media_type,
        series_name: choice.series_name,
        season_number: choice.season_number,
        episode_number: choice.episode_number,
      });
      await reviewApi.accept(item.id);
      reviewQueue = reviewQueue.filter(r => r.id !== item.id);
      stats.review = Math.max(0, stats.review - 1);
      stats.confirmed = stats.confirmed + 1;
      pipeline.review = Math.max(0, pipeline.review - 1);
      pipeline.confirmed = pipeline.confirmed + 1;
      fetchFiles(); // Refresh file list to show updated status
      showToast('Named and accepted', 'success');
    } catch (e) {
      console.error('Failed to name review item:', e);
      showToast('Failed to name file', 'error');
    }
  }

  // ── Helpers ────────────────────────────────────────────────

  function statusColor(status) {
    const colors = {
      Scanned: 'default',
      Matched: 'accent',
      NeedsReview: 'warning',
      Confirmed: 'success',
      Moved: 'success',
      Failed: 'error',
    };
    return colors[status] || 'default';
  }

  function formatSize(size) {
    if (!size) return '-';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (size >= 1024 && i < units.length - 1) {
      size /= 1024;
      i++;
    }
    return `${size.toFixed(1)} ${units[i]}`;
  }

  function formatDuration(seconds) {
    if (!seconds || seconds <= 0) return '-';
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function jobProgress(job) {
    if (!job.progress_total || job.progress_total === 0) return 0;
    return Math.round((job.progress_processed / job.progress_total) * 100);
  }

  function jobIcon(kind) {
    const icons = { scan: '\uD83D\uDD0D', analyze: '\uD83D\uDD0D', move: '\uD83D\uDCE4', rip: '\uD83D\uDCBF' };
    return icons[kind] || '\u2699\uFE0F';
  }

  function jobLabel(kind) {
    const labels = { scan: 'Scan', analyze: 'Analyze', move: 'Move', rip: 'Rip' };
    return labels[kind] || kind;
  }
</script>

<!-- Toasts -->
{#each toasts as toast}
  <div class="toast toast--visible toast--{toast.type}">
    <div class="banner__text">{toast.message}</div>
  </div>
{/each}

<!-- DetailDrawer -->
{#if drawerOpen && selectedFile}
  <DetailDrawer
    file={selectedFile}
    open={drawerOpen}
    onClose={handleDrawerClose}
    onConfirm={handleDrawerConfirm}
    onReject={handleDrawerReject}
  />
{/if}

<!-- Main Layout -->
<div class="dashboard-grid">
  <!-- Stats Bar -->
  <div class="stats-bar">
    <div class="card stat">
      <div class="stat__value">{stats.scanned}</div>
      <div class="stat__label">Scanned</div>
    </div>
    <div class="card stat stat--accent">
      <div class="stat__value">{stats.matched}</div>
      <div class="stat__label">Matched</div>
    </div>
    <div class="card stat stat--warning">
      <div class="stat__value">{stats.review}</div>
      <div class="stat__label">Needs Review</div>
    </div>
    <div class="card stat stat--success">
      <div class="stat__value">{stats.confirmed}</div>
      <div class="stat__label">Confirmed</div>
    </div>
    <div class="card stat stat--accent">
      <div class="stat__value">{stats.moved}</div>
      <div class="stat__label">Moved</div>
    </div>
  </div>

  <!-- Pipeline Progress -->
  {#if pipeline.total > 0}
    <div class="pipeline-bar">
      {#if pipeline.scanned > 0}
        <a href="/files?status=Scanned" class="pipeline-stage pipeline-stage--scanned">
          <span class="pipeline-stage__count">{pipeline.scanned}</span>
          <span class="pipeline-stage__label">Scanned</span>
        </a>
      {/if}
      <span class="pipeline-arrow">&#8594;</span>
      {#if pipeline.matched > 0}
        <a href="/files?status=Matched" class="pipeline-stage pipeline-stage--matched">
          <span class="pipeline-stage__count">{pipeline.matched}</span>
          <span class="pipeline-stage__label">Matched</span>
        </a>
      {/if}
      <span class="pipeline-arrow">&#8594;</span>
      {#if pipeline.review > 0}
        <a href="/files?status=NeedsReview" class="pipeline-stage pipeline-stage--review">
          <span class="pipeline-stage__count">{pipeline.review}</span>
          <span class="pipeline-stage__label">Review</span>
        </a>
      {/if}
      <span class="pipeline-arrow">&#8594;</span>
      {#if pipeline.confirmed > 0}
        <a href="/files?status=Confirmed" class="pipeline-stage pipeline-stage--confirmed">
          <span class="pipeline-stage__count">{pipeline.confirmed}</span>
          <span class="pipeline-stage__label">Confirmed</span>
        </a>
      {/if}
      <span class="pipeline-arrow">&#8594;</span>
      {#if pipeline.moved > 0}
        <a href="/files?status=Moved" class="pipeline-stage pipeline-stage--moved">
          <span class="pipeline-stage__count">{pipeline.moved}</span>
          <span class="pipeline-stage__label">Moved</span>
        </a>
      {/if}
    </div>
  {/if}

  <!-- Content Area -->
  <div class="content-area">
    <!-- Sidebar (left) -->
    <aside class="sidebar" class:sidebar--collapsed={sidebarCollapsed}>
      <!-- Sidebar Toggle -->
      <button class="sidebar-toggle" onclick={() => sidebarCollapsed = !sidebarCollapsed}>
        {#if sidebarCollapsed}
          &#9776;
        {:else}
          &#9664;
        {/if}
      </button>

      <!-- Drives Panel -->
      <div class="sidebar-panel">
        <div class="sidebar-panel-header-row">
          <button class="sidebar-panel-header" onclick={() => sidebarPanel.drives = !sidebarPanel.drives}>
            <h4 class="sidebar-panel-title">Drives</h4>
            <span class="sidebar-panel-chevron">{#if sidebarPanel.drives}&#9660;{:else}&#9654;{/if}</span>
          </button>
          <button class="btn btn--ghost btn--xs" onclick={() => fetchDrives()} title="Refresh drives">↻</button>
        </div>
        {#if sidebarPanel.drives}
          <div class="sidebar-panel-body">
            {#if drivesLoading}
              <div class="sidebar-loading">
                <div class="spinner" style="width: 14px; height: 14px;"></div>
                <span>Querying drives...</span>
              </div>
            {:else if drives.length === 0}
              <div class="sidebar-empty">No drives found</div>
            {:else}
              <div class="drives-list">
                {#each drives as drive, i}
                  <div
                    class="drive-card"
                    class:drive-card--drag-over={dropIndex === i}
                    draggable="true"
                    ondragstart={() => { dragIndex = i; }}
                    ondragover={(e) => { e.preventDefault(); dropIndex = i; }}
                    ondragleave={() => { dropIndex = null; }}
                    ondragend={() => {
                      if (dragIndex !== null && dropIndex !== null && dragIndex !== dropIndex) {
                        const reordered = [...drives];
                        const [dragged] = reordered.splice(dragIndex, 1);
                        reordered.splice(dropIndex, 0, dragged);
                        drives = reordered;
                        saveDriveOrder();
                      }
                      dragIndex = null;
                      dropIndex = null;
                    }}
                  >
                    <div class="drive-card__info">
                      <div class="drive-card__drag-handle" title="Drag to reorder">⠿</div>
                      <div class="drive-card__name">{drive.name}</div>
                      {#if drive.disc_name}
                        <div class="drive-card__disc" onclick={() => handleNameDisc(drive.index)} title="Click to set media name">
                          {drive.disc_name}
                          {#if drive.derived_name && drive.derived_name !== drive.disc_name}
                            <span class="drive-card__arrow"> → </span>
                            <span class="drive-card__derived-name">{drive.derived_name}</span>
                          {/if}
                        </div>
                      {:else}
                        <div class="drive-card__disc">No disc</div>
                      {/if}
                      <span class="badge badge--{drive.state === 'Ready' ? 'success' : 'default'}">{drive.state}</span>
                    </div>
                    {#if (drive.state === 'Ripping' || ripProgress[drive.index]?.doneAt) && ripProgress[drive.index]}
                      <div class="drive-card__progress">
                        <div class="text-sm font-semibold" style="margin-bottom: var(--space-1);">
                          {ripStage(drive.index)}
                        </div>
                        <div class="progress">
                          <div
                            class="progress__bar"
                            class:progress__bar--done={ripProgress[drive.index].pct >= 100}
                            style="width: {ripProgress[drive.index].pct}%;"
                          ></div>
                        </div>
                        <div class="text-xs text-muted" style="margin-top: var(--space-1);">
                          {ripLabel(drive.index)}
                        </div>
                      </div>
                    {/if}
                    <div class="drive-card__actions">
                      <div class="drive-card__action-row">
                        {#if drive.state === 'Ready'}
                          <button
                            class="btn btn--primary btn--sm"
                            onclick={() => handleRip(drive.index)}
                            disabled={!ripOutputDir}
                          >
                            Rip
                          </button>
                          {#if drive.disc_name}
                            <button
                              class="btn btn--secondary btn--sm"
                              onclick={() => handleEject(drive.index)}
                              title="Eject disc"
                            >
                              Eject
                            </button>
                          {/if}
                        {/if}
                        {#if drive.state === 'Ripping'}
                          <button
                            class="btn btn--danger btn--sm"
                            onclick={() => handleCancelRip(drive.index)}
                          >
                            Cancel
                          </button>
                        {/if}
                      </div>
                      <label class="toggle autorip-toggle" title="Auto-rip discs inserted in this drive">
                        <input
                          type="checkbox"
                          checked={autoripDrives.includes(drive.index)}
                          onclick={() => handleToggleAutorip(drive.index)}
                        />
                        <span class="toggle__track"></span>
                      </label>
                    </div>
                  </div>
                {/each}
                {#if driveOrder.length > 0}
                  <button class="btn btn--ghost btn--xs" onclick={resetDriveOrder} style="margin: var(--space-2) auto; display: block;">Reset order</button>
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Review Panel -->
      <div class="sidebar-panel">
        <button class="sidebar-panel-header" onclick={() => sidebarPanel.review = !sidebarPanel.review}>
          <h4 class="sidebar-panel-title">Review</h4>
          <span class="sidebar-panel-chevron">{#if sidebarPanel.review}&#9660;{:else}&#9654;{/if}</span>
          {#if reviewQueue.length > 0}
            <span class="badge badge--warning">{reviewQueue.length}</span>
          {/if}
        </button>
        {#if sidebarPanel.review}
          <div class="sidebar-panel-body">
            {#if reviewLoading}
              <div class="sidebar-loading">
                <div class="spinner" style="width: 14px; height: 14px;"></div>
                <span>Loading review queue...</span>
              </div>
            {:else if reviewQueue.length === 0}
              <div class="sidebar-empty">None</div>
            {:else}
              <div class="review-list">
                {#each reviewQueue as item}
                  <div class="review-item">
                    <div class="review-item__info">
                      <div class="truncate">{item.canonical_title || item.suggested_title || item.filename}</div>
                      {#if item.confidence}
                        <div class="text-xs text-muted">{Math.round(item.confidence * 100)}% confidence</div>
                      {/if}
                    </div>
                    <div class="review-item__actions">
                      <button class="btn btn--secondary btn--sm" onclick={() => handleNameReviewItem(item)}>Name</button>
                      <button class="btn btn--primary btn--sm" onclick={() => handleAccept(item.id)}>Accept</button>
                      <button class="btn btn--danger btn--sm" onclick={() => handleReject(item.id)}>Reject</button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Jobs Panel -->
      <div class="sidebar-panel">
        <button class="sidebar-panel-header" onclick={() => sidebarPanel.jobs = !sidebarPanel.jobs}>
          <h4 class="sidebar-panel-title">Jobs</h4>
          {#if activeJobs.length > 0}
            <span class="badge badge--accent">{activeJobs.length}</span>
          {/if}
        </button>
        {#if sidebarPanel.jobs}
          <div class="sidebar-panel-body">
            {#if jobsLoading}
              <div class="sidebar-loading">
                <div class="spinner" style="width: 14px; height: 14px;"></div>
                <span>Loading...</span>
              </div>
            {:else if activeJobs.length === 0}
              <div class="sidebar-empty">No active jobs</div>
            {:else}
              <div class="jobs-list">
                {#each activeJobs as job}
                  <div class="job-card">
                    <div class="job-card__header">
                      <span class="job-icon">{jobIcon(job.kind)}</span>
                      <div class="flex-1 min-w-0">
                        <div class="text-xs font-semibold">{jobLabel(job.kind)}</div>
                        <div class="text-xs text-muted">{job.status}</div>
                      </div>
                      {#if job.status === 'Running'}
                        <button
                          class="btn btn--danger btn--sm"
                          onclick={() => handleCancelJob(job.id)}
                        >
                          Cancel
                        </button>
                      {/if}
                    </div>
                    {#if job.progress_total > 0}
                      <div class="progress" style="margin-top: var(--space-2);">
                        <div
                          class="progress__bar"
                          style="width: {jobProgress(job)}%;"
                        ></div>
                      </div>
                      <div class="text-xs text-muted" style="margin-top: var(--space-1);">
                        {job.progress_processed}/{job.progress_total}
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </aside>

    <!-- File List (right) -->
    <div class="file-list-panel">
      <div class="panel-header">
        <h3 class="panel-title">Files</h3>
        <a href="/files" class="btn btn--secondary btn--sm">View All</a>
      </div>
      {#if filesLoading}
        <div class="empty" style="padding-top: 40px;">
          <div class="spinner" style="margin: 0 auto var(--space-3);"></div>
          <div class="empty__title">Loading files...</div>
        </div>
      {:else if files.length === 0}
        <div class="empty">
          <div class="empty__icon">&#128193;</div>
          <div class="empty__title">No files yet</div>
          <div class="empty__description">Run a scan to get started</div>
        </div>
      {:else}
        <div class="table-container">
          <table class="table">
            <thead>
              <tr>
                <th>Filename</th>
                <th>Type</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each files as file}
                <tr class="file-row" onclick={() => handleSelectFile(file)}>
                  <td class="truncate" style="max-width: 250px;">{file.filename}</td>
                  <td><span class="badge badge--default">{file.media_type}</span></td>
                  <td>
                    <span class="badge badge--{statusColor(file.status)}">{file.status}</span>
                  </td>
                  {#if file.status === 'NeedsReview' && file.confidence >= 0.8}
                    <td>
                      <button
                        class="btn btn--primary btn--sm"
                        onclick={(e) => {
                          e.stopPropagation();
                          handleAccept(file.id);
                        }}
                      >
                        Accept
                      </button>
                    </td>
                  {/if}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* ── Dashboard Grid ─────────────────────────────────────── */
  .dashboard-grid {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  /* ── Stats Bar ──────────────────────────────────────────── */
  .stats-bar {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: var(--space-4);
  }

  /* ── Pipeline Bar ───────────────────────────────────────── */
  .pipeline-bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    overflow-x: auto;
  }

  .pipeline-stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-lg);
    text-decoration: none;
    transition: background var(--transition-fast);
    min-width: 60px;
  }

  .pipeline-stage:hover {
    background: var(--color-bg-tertiary);
  }

  .pipeline-stage__count {
    font-size: var(--text-xl);
    font-weight: 700;
    line-height: 1;
  }

  .pipeline-stage__label {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .pipeline-stage--scanned .pipeline-stage__count { color: var(--color-text-primary); }
  .pipeline-stage--matched .pipeline-stage__count { color: var(--color-accent); }
  .pipeline-stage--review .pipeline-stage__count { color: var(--color-warning); }
  .pipeline-stage--confirmed .pipeline-stage__count { color: var(--color-success); }
  .pipeline-stage--moved .pipeline-stage__count { color: var(--color-accent); }

  .pipeline-arrow {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    flex-shrink: 0;
  }

  /* ── Content Area ───────────────────────────────────────── */
  .content-area {
    display: grid;
    grid-template-columns: 1fr 280px;
    gap: var(--space-4);
    flex: 1;
    min-height: 0;
  }

  /* ── File List Panel ────────────────────────────────────── */
  .file-list-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-4);
  }

  .panel-title {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .table-container {
    flex: 1;
    overflow-y: auto;
  }

  .file-row {
    cursor: pointer;
  }

  .file-row:hover {
    background: var(--color-bg-tertiary) !important;
  }

  /* ── Sidebar ────────────────────────────────────────────── */
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    overflow-y: auto;
    max-height: calc(100vh - 200px);
    position: sticky;
    top: 80px;
  }

  .sidebar--collapsed {
    width: 48px;
    min-width: 48px;
  }

  .sidebar--collapsed .sidebar-panel {
    display: none;
  }

  .sidebar-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    padding: var(--space-2);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--text-sm);
    transition: color var(--transition-fast), background var(--transition-fast);
  }

  .sidebar-toggle:hover {
    color: var(--color-text-primary);
    background: var(--color-bg-tertiary);
  }

  /* ── Sidebar Panels ─────────────────────────────────────── */
  .sidebar-panel {
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    overflow: hidden;
  }

  .sidebar-panel-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-3) var(--space-4);
    background: none;
    border: none;
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .sidebar-panel-header:hover {
    background: var(--color-bg-tertiary);
  }

  .sidebar-panel-title {
    font-size: var(--text-sm);
    font-weight: 600;
    flex: 1;
    text-align: left;
  }

  .sidebar-panel-chevron {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .sidebar-panel-header-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .sidebar-panel-header-row .sidebar-panel-header {
    flex: 1;
  }

  .sidebar-panel-body {
    padding: var(--space-3) var(--space-4) var(--space-4);
  }

  .sidebar-loading {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .sidebar-empty {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    text-align: center;
    padding: var(--space-4) 0;
  }

  /* ── Drive Card ─────────────────────────────────────────── */
  .drive-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--color-border);
    cursor: grab;
    transition: opacity var(--transition-fast), border-color var(--transition-fast);
  }

  .drive-card:active {
    cursor: grabbing;
  }

  .drive-card--drag-over {
    border: 2px dashed var(--color-primary);
  }

  .drive-card:last-child {
    border-bottom: none;
  }

  .drive-card__info {
    flex: 1;
    min-width: 0;
  }

  .drive-card__drag-handle {
    cursor: grab;
    color: var(--color-text-muted);
    user-select: none;
    font-size: var(--text-sm);
    opacity: 0.5;
    margin-right: var(--space-2);
    transition: opacity var(--transition-fast);
  }

  .drive-card:hover .drive-card__drag-handle {
    opacity: 1;
  }

  .drive-card__progress {
    width: 100%;
    margin-top: var(--space-2);
  }

  .drive-card__name {
    font-size: var(--text-sm);
    font-weight: 500;
    margin-bottom: var(--space-1);
  }

  .drive-card__disc {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--space-1);
    cursor: pointer;
    user-select: none;
  }

  .drive-card__disc:hover {
    text-decoration: underline;
  }

  .drive-card__arrow {
    color: var(--color-text-muted);
    margin: 0 var(--space-1);
    opacity: 0.6;
  }

  .drive-card__derived-name {
    color: var(--color-primary);
    font-weight: 500;
  }

  .drive-card__actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }

  .drive-card__action-row {
    display: flex;
    gap: var(--space-2);
  }

  .autorip-toggle {
    margin: 0;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    cursor: pointer;
  }

  .toggle input {
    display: none;
  }

  .toggle__track {
    display: block;
    width: 36px;
    height: 20px;
    background: var(--color-border);
    border-radius: 10px;
    position: relative;
    transition: background var(--transition-fast);
  }

  .toggle input:checked + .toggle__track {
    background: var(--color-success);
  }

  .toggle__track::after {
    content: '';
    display: block;
    width: 16px;
    height: 16px;
    background: white;
    border-radius: 50%;
    position: absolute;
    top: 2px;
    left: 2px;
    transition: transform var(--transition-fast);
  }

  .toggle input:checked + .toggle__track::after {
    transform: translateX(16px);
  }

  /* ── Review Item ────────────────────────────────────────── */
  .review-item {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--color-border);
  }

  .review-item:last-child {
    border-bottom: none;
  }

  .review-item__info {
    flex: 1;
    min-width: 0;
  }

  .review-item__actions {
    display: flex;
    gap: var(--space-1);
    flex-shrink: 0;
  }

  /* ── Job Card ───────────────────────────────────────────── */
  .job-card {
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--color-border);
  }

  .job-card:last-child {
    border-bottom: none;
  }

  .job-card__header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  /* ── Responsive ─────────────────────────────────────────── */
  @media (max-width: 1024px) {
    .content-area {
      grid-template-columns: 1fr;
    }

    .sidebar {
      position: static;
      max-height: none;
      flex-direction: row;
      flex-wrap: wrap;
    }

    .sidebar-panel {
      flex: 1;
      min-width: 200px;
    }

    .sidebar--collapsed .sidebar-panel {
      display: block;
    }

    .sidebar-toggle {
      display: none;
    }
  }

  @media (max-width: 768px) {
    .stats-bar {
      grid-template-columns: repeat(2, 1fr);
    }

    .pipeline-bar {
      flex-wrap: wrap;
    }
  }
</style>
