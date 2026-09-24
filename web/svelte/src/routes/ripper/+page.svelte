<script>
  import { onMount, onDestroy } from 'svelte';
  import { ripperApi } from '$lib/api';
  import { connectSSE } from '$lib/api/sse';
  import Tooltip from '$lib/components/Tooltip.svelte';
  import MediaNamingModal from '$lib/components/MediaNamingModal.svelte';

  /** @type {import('$lib/types').DriveInfo[]} */
  let drives = $state([]);
  let drivesLoading = $state(true);
  let installStatus = $state(null);

  // ── Unfinished Titles ──────────────────────────────────────
  // Derived from ripNaming state (per-drive naming failures/completions)
  // Spec: rips that fail to auto-name stay in the Rip view, not the Organizer
  let unfinishedTitles = $derived.by(() =>
    Object.entries(ripNaming)
      .filter(([, v]) => v.status === 'failed' || v.status === 'complete')
      .map(([idx, v]) => ({
        driveIndex: Number(idx),
        discName: v.discName,
        foundName: v.foundName,
        status: v.status,
      }))
  );

  // ── Rip progress ───────────────────────────────────────────
  /** @type {Record<number, { pct: number, eta: number | null, stage: string }>} */
  let ripProgress = $state({});

  // ── Rip naming status (ADR-024) ────────────────────────────
  /** @type {Record<number, { discName: string, foundName: string | null, status: 'naming' | 'complete' | 'failed' }>} */
  let ripNaming = $state({});

  // ── Rip poster ─────────────────────────────────────────────
  /** @type {Record<number, { title: string, year: number, posterUrl: string | null }>} */
  let ripPosters = $state({});

  // ── Autorip ────────────────────────────────────────────────
  let autoripDrives = $state([]);

  // ── Naming Modal ───────────────────────────────────────────
  /** @type {{ driveIndex: number, discName: string } | null} */
  let namingTarget = $state(null);

  // ── SSE ────────────────────────────────────────────────────
  let sseDisconnect = null;

  // ── Lifecycle ──────────────────────────────────────────────

  onMount(async () => {
    await Promise.all([fetchDrives(), fetchAutoripStatus(), fetchInstallStatus()]);
    sseDisconnect = connectSSE(handleSSEEvent);
  });

  onDestroy(() => {
    sseDisconnect?.();
  });

  // ── Data fetching ──────────────────────────────────────────

  async function fetchDrives() {
    try {
      drives = await ripperApi.listDrives();
    } catch (e) {
      console.error('Failed to load drives:', e);
    } finally {
      drivesLoading = false;
    }
  }

  async function fetchAutoripStatus() {
    try {
      autoripDrives = await ripperApi.getAutoripStatus();
    } catch (e) {
      console.error('Failed to load autorip status:', e);
    }
  }

  async function fetchInstallStatus() {
    try {
      const status = await ripperApi.getStatus();
      installStatus = { installed: status.installed, method: formatMethod(status.method) };
    } catch (e) {
      console.error('Failed to load install status:', e);
    }
  }

  function formatMethod(method) {
    if (typeof method === 'object' && method !== null) {
      const keys = Object.keys(method);
      return keys.length > 0 ? keys[0] : 'Unknown';
    }
    return String(method);
  }

  // ── Naming Modal Handlers ──────────────────────────────────

  function openNamingModal(title) {
    namingTarget = { driveIndex: title.driveIndex, discName: title.discName };
  }

  function closeNamingModal() {
    namingTarget = null;
  }

  async function handleNamingConfirm(choice) {
    if (!namingTarget) return;
    try {
      await ripperApi.rename(namingTarget.driveIndex, choice);
      // Update local state to reflect the applied name
      const title = namingTarget.discName;
      const foundName = choice.year ? `${choice.title} (${choice.year})` : choice.title;
      ripNaming = {
        ...ripNaming,
        [namingTarget.driveIndex]: { discName: title, foundName, status: 'complete' },
      };
    } catch (e) {
      console.error('Failed to apply name:', e);
    } finally {
      namingTarget = null;
    }
  }

  // ── SSE ────────────────────────────────────────────────────

  /** @param {import('$lib/api/sse').SseEvent} event */
  function handleSSEEvent(event) {
    const { kind, action, index: idx, data } = event;

    // Drive events
    if (kind === 'drive' && (action === 'inserted' || action === 'ejected' || action === 'removed' || action === 'updated')) {
      fetchDrives();
      return;
    }

    if (kind !== 'rip') return;

    if (action === 'starting') {
      drives = drives.map(d => d.index === idx ? { ...d, state: 'Ripping' } : d);
      ripProgress = { ...ripProgress, [idx]: { pct: 0, eta: null, stage: '' } };
      return;
    }

    if (action === 'progress') {
      const { pct, eta, stage } = data;
      ripProgress = { ...ripProgress, [idx]: { pct: pct || 0, eta: eta ?? null, stage: stage || '' } };
      return;
    }

    if (action === 'complete') {
      drives = drives.map(d => d.index === idx ? { ...d, state: 'Complete' } : d);
      const dest = data.dest || '';
      if (dest) {
        const folderName = dest.split('/').pop() || '';
        if (folderName) {
          // Auto-naming already happened, just show complete
        }
      }
      setTimeout(() => {
        ripProgress = Object.fromEntries(Object.entries(ripProgress).filter(([k]) => k !== String(idx)));
      }, 5000);
      fetchDrives();
      return;
    }

    if (action === 'error') {
      const message = data.message || 'Unknown error';
      drives = drives.map(d => d.index === idx ? { ...d, state: 'Error' } : d);
      ripProgress = { ...ripProgress, [idx]: { pct: 0, eta: null, stage: message } };
      setTimeout(() => {
        ripProgress = Object.fromEntries(Object.entries(ripProgress).filter(([k]) => k !== String(idx)));
      }, 5000);
      fetchDrives();
      return;
    }

    if (action === 'cancelled') {
      drives = drives.map(d => d.index === idx ? { ...d, state: 'Ready' } : d);
      ripProgress = Object.fromEntries(Object.entries(ripProgress).filter(([k]) => k !== String(idx)));
      return;
    }

    if (action === 'message') {
      const text = data.text || '';
      ripProgress = { ...ripProgress, [idx]: { ...ripProgress[idx], stage: text } };
      return;
    }

    if (action === 'naming') {
      const discName = drives.find(d => d.index === idx)?.disc_name || '';
      ripNaming = { ...ripNaming, [idx]: { discName, foundName: null, status: 'naming' } };
      return;
    }

    if (action === 'named') {
      const title = data.title || '';
      const year = data.year || null;
      const discName = drives.find(d => d.index === idx)?.disc_name || '';
      const foundName = year ? `${title} (${year})` : title;
      ripNaming = { ...ripNaming, [idx]: { discName, foundName, status: 'complete' } };

      fetchPoster(idx, title, year);
      return;
    }

    if (action === 'naming_failed') {
      const discName = drives.find(d => d.index === idx)?.disc_name || '';
      ripNaming = { ...ripNaming, [idx]: { discName, foundName: null, status: 'failed' } };
      return;
    }
  }

  // ── TMDb Poster ────────────────────────────────────────────
  // Cache key is (title, year) not (idx, title, year) to deduplicate
  // same title fetched for different drives.

  /** @param {number} idx - Drive index */
  /** @param {string} title - Movie/TV title */
  /** @param {number | null} year - Year */
  async function fetchPoster(idx, title, year) {
    if (!title) return;

    // Deduplicate by (title, year) — not per-drive
    const cacheKey = `${title}::${year ?? ''}`;
    if (Object.values(ripPosters).some(p => `${p.title}::${p.year ?? ''}` === cacheKey)) return;

    // Spec: search movies first, then TV if no movie match
    const searchParams = `${encodeURIComponent(title)}${year ? `&year=${year}` : ''}`;
    const searchUrl = `/v1/search/tmdb?query=${searchParams}&media_type=`;

    try {
      // Try movies first
      let res = await fetch(`${searchUrl}movie`);
      if (res.ok) {
        const result = await res.json();
        const matches = result.data?.results || [];
        if (matches.length) {
          const posterPath = matches[0].poster_path;
          if (posterPath) {
            ripPosters = { ...ripPosters, [idx]: { title, year, posterUrl: `https://image.tmdb.org/t/p/w300${posterPath}` } };
          }
          return;
        }
      }

      // Fallback to TV
      res = await fetch(`${searchUrl}tv`);
      if (!res.ok) return;

      const result = await res.json();
      const matches = result.data?.results || [];
      if (!matches.length) return;

      const posterPath = matches[0].poster_path;
      if (!posterPath) return;

      ripPosters = { ...ripPosters, [idx]: { title, year, posterUrl: `https://image.tmdb.org/t/p/w300${posterPath}` } };
    } catch (e) {
      console.error('Failed to fetch poster:', e);
    }
  }

  // ── Actions ────────────────────────────────────────────────

  async function startRip(index) {
    try {
      await ripperApi.start(index, '');
    } catch (e) {
      console.error(`Failed to start rip on drive ${index}:`, e);
    }
  }

  async function cancelRip(index) {
    try {
      await ripperApi.cancel(index);
    } catch (e) {
      console.error(`Failed to cancel rip on drive ${index}:`, e);
    }
  }

  async function ejectDisc(index) {
    try {
      await ripperApi.eject(index);
    } catch (e) {
      console.error(`Failed to eject disc from drive ${index}:`, e);
    }
  }

  async function toggleAutorip(index) {
    try {
      autoripDrives = await ripperApi.toggleAutorip(index);
    } catch (e) {
      console.error(`Failed to toggle autorip on drive ${index}:`, e);
    }
  }

  // ── Helpers ────────────────────────────────────────────────

  function driveStateIcon(state) {
    switch (state) {
      case 'Empty': return '⏹';
      case 'Loading': return '⟳';
      case 'Ready': return '💿';
      case 'Ripping': return '⚡';
      case 'Complete': return '✅';
      case 'Error': return '❌';
      default: return '❓';
    }
  }

  function driveStateColor(state) {
    switch (state) {
      case 'Empty': return 'var(--color-text-muted)';
      case 'Loading': return 'var(--color-warning)';
      case 'Ready': return 'var(--color-success)';
      case 'Ripping': return 'var(--color-primary)';
      case 'Complete': return 'var(--color-success)';
      case 'Error': return 'var(--color-danger)';
      default: return 'var(--color-text-muted)';
    }
  }

  function getRipDisabledTooltip(state) {
    switch (state) {
      case 'Empty': return 'No disc loaded';
      case 'Loading': return 'Drive loading...';
      case 'Ripping': return 'Rip in progress';
      default: return 'Rip only available when drive is ready';
    }
  }

  function getCancelDisabledTooltip(state) {
    switch (state) {
      case 'Ripping': return 'Cancel in progress';
      default: return 'No rip in progress';
    }
  }

  function getEjectDisabledTooltip(state) {
    switch (state) {
      case 'Empty': return 'No disc to eject';
      case 'Loading': return 'Drive loading...';
      case 'Ready': return 'Disc not yet ripped';
      case 'Ripping': return 'Cannot eject during rip';
      default: return 'Eject only available after rip completes';
    }
  }

  // ── ETA formatting ─────────────────────────────────────────
  /** @param {number} seconds */
  function formatEta(seconds) {
    if (seconds <= 0) return '0s';
    // Below 45s: show seconds (no flapping)
    if (seconds < 45) return `${seconds}s`;
    // 45s+: show minutes
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    if (mins === 0) return `${secs}s`;
    if (secs === 0) return `${mins}m`;
    return `${mins}m ${secs}s`;
  }
</script>

<div class="ripper">
  <!-- Install warning -->
  {#if installStatus && !installStatus.installed}
    <div class="ripper__warning" role="alert">
      <span class="ripper__warning-icon">⚠</span>
      <span class="ripper__warning-text">
        MakeMKV is not installed. Install it to enable ripping.
        {#if installStatus.method === 'Flatpak'}
          Detected Flatpak — check permissions for drive access.
        {/if}
      </span>
    </div>
  {/if}

  <div class="ripper__header">
    <h2 class="ripper__title">Ripper</h2>
    {#if drivesLoading}
      <span class="ripper__loading">Loading drives...</span>
    {:else}
      <span class="ripper__count">{drives.length} drive{drives.length !== 1 ? 's' : ''}</span>
    {/if}
  </div>

  <!-- Drive cards -->
  <div class="ripper__drives">
    {#each drives as drive (drive.index)}
      <div class="drive-card" class:drive-card--active={drive.state === 'Ripping'}>
        <!-- Poster (left side) -->
        <div class="drive-card__poster">
          {#if ripPosters[drive.index]?.posterUrl}
            <img src={ripPosters[drive.index].posterUrl} alt="Poster" class="drive-card__poster-img" />
          {:else}
            <div class="drive-card__poster-placeholder">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 180" width="120" height="180">
                <rect width="120" height="180" rx="8" fill="#374151"/>
                <circle cx="60" cy="70" r="25" fill="#4B5563"/>
                <path d="M20 160 Q60 120 100 160" stroke="#4B5563" stroke-width="3" fill="none"/>
              </svg>
            </div>
          {/if}
        </div>

        <div class="drive-card__info">
          <div class="drive-card__header">
            <span class="drive-card__state" style="color: {driveStateColor(drive.state)}">
              {driveStateIcon(drive.state)}
            </span>
            <span class="drive-card__name">Drive {drive.index}</span>
            {#if drive.name}
              <span class="drive-card__disc-name">{drive.name}</span>
            {/if}
            {#if drive.disc_name}
              <span class="drive-card__disc-label">Disc: {drive.disc_name}</span>
            {/if}
          </div>

          <!-- Status line (MakeMKV stage text) -->
          {#if ripProgress[drive.index]?.stage}
            <div class="drive-card__status">
              {ripProgress[drive.index].stage}
            </div>
          {/if}

          <!-- Progress bar -->
          {#if ripProgress[drive.index]}
            <div class="drive-card__progress">
              <div class="progress-bar">
                <div class="progress-bar__fill" style="width: {ripProgress[drive.index].pct}%"></div>
              </div>
              <div class="drive-card__progress-info">
                <span class="drive-card__progress-pct">{ripProgress[drive.index].pct}%</span>
                {#if ripProgress[drive.index].eta !== null && ripProgress[drive.index].eta > 0}
                  <span class="drive-card__progress-eta">ETA: {formatEta(ripProgress[drive.index].eta)}</span>
                {/if}
              </div>
            </div>
          {/if}

          <!-- Auto-naming status -->
          {#if ripNaming[drive.index]}
            <div class="drive-card__naming">
              {#if ripNaming[drive.index].status === 'naming'}
                <span class="drive-card__disc-name">{ripNaming[drive.index].discName}</span>
                <span class="drive-card__arrow">→</span>
                <span class="drive-card__naming-status">Searching...</span>
              {:else if ripNaming[drive.index].status === 'complete'}
                <span class="drive-card__disc-name">{ripNaming[drive.index].discName}</span>
                <span class="drive-card__arrow">→</span>
                <span class="drive-card__derived-name">{ripNaming[drive.index].foundName}</span>
              {:else}
                <span class="drive-card__disc-name">{ripNaming[drive.index].discName}</span>
                <span class="drive-card__arrow">→</span>
                <span class="drive-card__failed-name">Not found</span>
              {/if}
            </div>
          {/if}

          <!-- Unidentified message -->
          {#if (drive.state === 'Complete' || drive.state === 'Error') && !drive.derived_name}
            <div class="drive-card__naming">
              <span class="drive-card__failed-name">Unidentified — view in Organizer</span>
            </div>
          {/if}

          <!-- Actions -->
          <div class="drive-card__actions">
            <div class="drive-card__action-row">
              <!-- Rip button -->
              {#if drive.state === 'Ready' || drive.state === 'Complete'}
                <button
                  class="btn btn--primary btn--sm"
                  onclick={() => startRip(drive.index)}
                  title="Start rip"
                >
                  Rip
                </button>
              {:else}
                <Tooltip text={getRipDisabledTooltip(drive.state)}>
                  <button
                    class="btn btn--primary btn--sm btn--disabled"
                    disabled
                  >
                    Rip
                  </button>
                </Tooltip>
              {/if}

              <!-- Cancel button -->
              {#if drive.state === 'Ripping'}
                <button
                  class="btn btn--danger btn--sm"
                  onclick={() => cancelRip(drive.index)}
                  title="Cancel rip"
                >
                  Cancel
                </button>
              {:else}
                <Tooltip text={getCancelDisabledTooltip(drive.state)}>
                  <button
                    class="btn btn--danger btn--sm btn--disabled"
                    disabled
                  >
                    Cancel
                  </button>
                </Tooltip>
              {/if}

              <!-- Eject button -->
              {#if drive.state === 'Ready' || drive.state === 'Complete' || drive.state === 'Error'}
                <button
                  class="btn btn--secondary btn--sm"
                  onclick={() => ejectDisc(drive.index)}
                  title="Eject disc"
                >
                  Eject
                </button>
              {:else}
                <Tooltip text={getEjectDisabledTooltip(drive.state)}>
                  <button
                    class="btn btn--secondary btn--sm btn--disabled"
                    disabled
                  >
                    Eject
                  </button>
                </Tooltip>
              {/if}

              <!-- View in Organizer button (unidentified rips) -->
              {#if (drive.state === 'Complete' || drive.state === 'Error') && !drive.derived_name}
                <a
                  href="/organizer"
                  class="btn btn--primary btn--sm"
                  title="View in Organizer"
                >
                  View in Organizer
                </a>
              {/if}
            </div>

            <!-- Autorip toggle -->
            <label class="autorip-toggle">
              <input
                type="checkbox"
                checked={autoripDrives.includes(drive.index)}
                onchange={() => toggleAutorip(drive.index)}
              />
              <span class="toggle__track"></span>
              <span class="toggle__label">AutoRip</span>
            </label>
          </div>
        </div>
      </div>
    {/each}

    {#if !drivesLoading && drives.length === 0}
      <div class="ripper__empty">No drives detected</div>
    {/if}
  </div>

  <!-- Unfinished Titles — derived from ripNaming state (per-drive naming failures/completions) -->
  {#if unfinishedTitles.length > 0}
    <div class="ripper__unfinished">
      <h3 class="ripper__unfinished-title">
        <span class="ripper__unfinished-icon">⏳</span>
        Unfinished Titles ({unfinishedTitles.length})
      </h3>
      <div class="ripper__unfinished-list">
        {#each unfinishedTitles as title (title.driveIndex)}
          <button
            type="button"
            class="unfinished-item"
            onclick={() => openNamingModal(title)}
          >
            <span class="unfinished-item__name">{title.discName}</span>
            {#if title.foundName}
              <span class="unfinished-item__suggested">→ {title.foundName}</span>
            {:else}
              <span class="unfinished-item__suggested">→ Media naming failed</span>
            {/if}
            <span class="unfinished-item__status">{title.status}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Naming Modal for unfinished titles -->
  {#if namingTarget}
    <MediaNamingModal
      initialTitle={namingTarget.discName}
      mediaType="movie"
      onConfirm={handleNamingConfirm}
      onClose={closeNamingModal}
    />
  {/if}
</div>

<style>
  .ripper {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    height: 100%;
  }

  .ripper__warning {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    background: var(--color-warning-bg);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-lg);
    color: var(--color-warning);
  }

  .ripper__warning-icon {
    font-size: var(--text-lg);
  }

  .ripper__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .ripper__title {
    font-size: var(--text-xl);
    font-weight: 700;
    margin: 0;
  }

  .ripper__loading,
  .ripper__count {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }

  .ripper__drives {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    flex: 1;
    overflow-y: auto;
  }

  .ripper__empty {
    text-align: center;
    padding: var(--space-8) 0;
    color: var(--color-text-muted);
    font-size: var(--text-base);
  }

  /* ── Drive Card ─────────────────────────────────────────── */
  .drive-card {
    display: flex;
    gap: var(--space-4);
    padding: var(--space-4);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .drive-card--active {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 1px var(--color-primary);
  }

  /* ── Poster (left side) ────────────────────────────────── */
  .drive-card__poster {
    flex-shrink: 0;
    width: 120px;
    height: 180px;
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--color-bg-tertiary);
  }

  .drive-card__poster-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: var(--radius-lg);
  }

  .drive-card__poster-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .drive-card__info {
    flex: 1;
    min-width: 0;
  }

  .drive-card__header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .drive-card__state {
    font-size: var(--text-lg);
    flex-shrink: 0;
  }

  .drive-card__name {
    font-weight: 600;
    font-size: var(--text-base);
  }

  .drive-card__disc-name {
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
  }

  .drive-card__disc-label {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-style: italic;
  }

  /* ── Status line ────────────────────────────────────────── */
  .drive-card__status {
    margin-top: var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    font-style: italic;
  }

  /* ── Auto-naming ────────────────────────────────────────── */
  .drive-card__naming {
    display: flex;
    align-items: center;
    gap: 0;
    margin-top: var(--space-2);
    font-size: var(--text-xs);
  }

  .drive-card__disc-name {
    color: var(--color-text-muted);
    font-style: italic;
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

  .drive-card__failed-name {
    color: var(--color-warning);
    font-style: italic;
  }

  .drive-card__naming-status {
    color: var(--color-text-muted);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  /* ── Progress ───────────────────────────────────────────── */
  .drive-card__progress {
    margin-top: var(--space-3);
  }

  .progress-bar {
    height: 6px;
    background: var(--color-bg-tertiary);
    border-radius: var(--radius-full);
    overflow: hidden;
  }

  .progress-bar__fill {
    height: 100%;
    background: var(--color-primary);
    transition: width 0.3s ease;
  }

  .drive-card__progress-info {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  .drive-card__progress-pct {
    font-weight: 600;
    color: var(--color-text-primary);
  }

  /* ── Actions ────────────────────────────────────────────── */
  .drive-card__actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: var(--space-3);
    gap: var(--space-3);
  }

  .drive-card__action-row {
    display: flex;
    gap: var(--space-2);
  }

  /* ── Disabled Buttons ──────────────────────────────────── */
  .btn--disabled {
    opacity: 0.4;
    cursor: not-allowed;
    pointer-events: none;
  }

  /* ── Autorip Toggle ─────────────────────────────────────── */
  .autorip-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    cursor: pointer;
    margin: 0;
  }

  .autorip-toggle input {
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
    flex-shrink: 0;
  }

  .autorip-toggle input:checked + .toggle__track {
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

  .autorip-toggle input:checked + .toggle__track::after {
    transform: translateX(16px);
  }

  .toggle__label {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    user-select: none;
  }

  /* ── Unfinished Titles ──────────────────────────────────── */
  .ripper__unfinished {
    padding: var(--space-4);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-xl);
  }

  .ripper__unfinished-title {
    font-size: var(--text-sm);
    font-weight: 600;
    margin: 0 0 var(--space-3) 0;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .ripper__unfinished-icon {
    font-size: var(--text-base);
  }

  .ripper__unfinished-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .unfinished-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-tertiary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    text-decoration: none;
    color: var(--color-text-primary);
    transition: all var(--transition-fast);
  }

  .unfinished-item:hover {
    background: var(--color-bg-secondary);
    border-color: var(--color-primary);
  }

  .unfinished-item__name {
    font-size: var(--text-sm);
    font-weight: 500;
    flex: 1;
  }

  .unfinished-item__suggested {
    font-size: var(--text-xs);
    color: var(--color-primary);
  }

  .unfinished-item__status {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    padding: var(--space-0.5) var(--space-2);
    background: var(--color-bg-secondary);
    border-radius: var(--radius-full);
  }
</style>
