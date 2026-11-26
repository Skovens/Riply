// Riply Frontend - Vanilla JS

(function() {
  'use strict';

  // State
  let drives = [];
  let rips = [];
  let sse = null;
  let searchRipId = null;
  let searchMode = null;      // 'drive' | 'rip'
  let searchDriveIndex = null;
  let searchSeq = 0;          // discards stale search responses
  let searchResultsList = [];
  let dirModalTarget = null;  // settings field id the browser fills
  let dirModalDir = null;     // directory currently listed
  let driveProgressMap = {}; // drivePath → {percent, speed_mbps, phase, title_index, title_total, rip_phase}
  let libraryData = { entries: [], series: [], movies: 0, episodes: 0, scanned_at: '' };

  // DOM refs
  const driveList = document.getElementById('drive-list');
  const ripList = document.getElementById('rip-list');
  const modal = document.getElementById('modal');
  const searchInput = document.getElementById('search-input');
  const searchResults = document.getElementById('search-results');
  const settingsForm = document.getElementById('settings-form');
  const dirModal = document.getElementById('dir-modal');
  const dirBreadcrumb = document.getElementById('dir-breadcrumb');
  const dirList = document.getElementById('dir-list');

  function esc(s) {
    return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  }

  // Navigation
  document.querySelectorAll('nav a').forEach(link => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      const target = link.getAttribute('href').replace('#', '');

      document.querySelectorAll('nav a').forEach(l => l.classList.remove('active'));
      document.querySelectorAll('.section').forEach(s => s.classList.remove('active'));

      link.classList.add('active');
      document.getElementById(target).classList.add('active');
    });
  });

  // Modal
  document.querySelector('.modal-close').addEventListener('click', closeModal);
  modal.addEventListener('click', (e) => {
    if (e.target === modal) closeModal();
  });

  function openModal(title) {
    document.getElementById('modal-title').textContent = title || 'Search TMDb';
    modal.classList.add('active');
    searchInput.value = '';
    searchResults.innerHTML = '';
    searchInput.focus();
  }

  function closeModal() {
    modal.classList.remove('active');
    searchRipId = null;
    searchDriveIndex = null;
    searchMode = null;
    searchSeq++;
  }

  // SSE Connection
  let sseReconnectDelay = 1000;
  const SSE_RECONNECT_MIN = 1000;
  const SSE_RECONNECT_MAX = 30000;
  let serverStoppedSeen = false; // set on the SSE shutdown event; blocks reconnects

  function connectSSE() {
    if (serverStoppedSeen) return; // server went down; wait for the recovery poll
    if (sse) sse.close();

    sse = new EventSource('/events');

    sse.addEventListener('drive_ready', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'info');
      delete driveProgressMap[data.drive];
      loadDrives();
    });

    sse.addEventListener('drive_loading', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'info');
      loadDrives();
    });

    sse.addEventListener('rip_duplicate', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'warning');
    });

    sse.addEventListener('rip_progress', (e) => {
      const data = JSON.parse(e.data);
      updateProgress(data.drive, data.progress);
    });

    sse.addEventListener('rip_complete', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'success');
      delete driveProgressMap[data.drive];
      loadDrives();
      loadRips();
    });

    sse.addEventListener('naming_started', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'info');
    });

    sse.addEventListener('naming_match', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'success');
      loadDrives();
      loadRips();
    });

    sse.addEventListener('naming_failed', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'warning');
    });

    sse.addEventListener('rip_failed', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'error');
      delete driveProgressMap[data.drive];
      loadDrives();
      loadRips();
    });

    sse.addEventListener('rip_accepted', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'success');
      loadRips();
    });

    sse.addEventListener('rip_cancelled', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'info');
      delete driveProgressMap[data.drive];
      loadDrives();
      loadRips();
    });

    sse.addEventListener('drive_accepted', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'success');
      loadDrives();
    });

    sse.addEventListener('drive_ejected', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'info');
      delete driveProgressMap[data.drive];
      loadDrives();
    });

    sse.addEventListener('library_scanned', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'success');
      enableScanButton();
      loadLibrary();
    });

    sse.addEventListener('library_scan_failed', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'error');
      enableScanButton();
      loadLibrary();
    });

    sse.addEventListener('library_reset', (e) => {
      const data = JSON.parse(e.data);
      toast(data.message, 'info');
      enableScanButton();
      hideResetConfirm();
      loadLibrary();
    });

    sse.addEventListener('shutdown', (e) => {
      // The server is going away for a restart; EventSource would
      // otherwise reconnect forever against a dead port (L10).
      let data = {};
      try { data = JSON.parse(e.data) || {}; } catch (_) {}
      serverStopped(data.message || 'Server stopped — waiting for restart.');
    });

    sse.addEventListener('open', () => {
      sseReconnectDelay = SSE_RECONNECT_MIN;
    });

    sse.onerror = () => {
      if (serverStoppedSeen) return; // server shutdown; recovery poll owns reconnect
      console.log('SSE connection lost, reconnecting in', sseReconnectDelay / 1000, 's...');
      sse.close();
      setTimeout(connectSSE, sseReconnectDelay);
      sseReconnectDelay = Math.min(sseReconnectDelay * 2, SSE_RECONNECT_MAX);
    };
  }

  // Server shutdown banner (L10): EventSource would keep reconnecting against
  // a dead port after a restart; stop it, show a persistent banner, and poll
  // for recovery instead.
  function serverStopped(message) {
    if (serverStoppedSeen) return;
    serverStoppedSeen = true;
    if (sse) { sse.close(); sse = null; }
    let banner = document.querySelector('.shutdown-banner');
    if (!banner) {
      banner = document.createElement('div');
      banner.className = 'shutdown-banner';
      document.body.prepend(banner);
    }
    banner.textContent = `${message} — reloading when the server is back…`;
    const poll = setInterval(async () => {
      try {
        const resp = await fetch('/api/config', { cache: 'no-store' });
        if (resp.ok) {
          clearInterval(poll);
          serverStoppedSeen = false;
          banner.remove();
          toast('Server back online', 'success');
          connectSSE();
          loadDrives();
          loadRips();
        }
      } catch (_) { /* not back yet */ }
    }, 3000);
  }

  // API helpers
  async function api(path, options = {}) {
    const resp = await fetch(path, {
      headers: { 'Content-Type': 'application/json' },
      ...options,
    });
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(`${resp.status}: ${text}`);
    }
    return resp.json();
  }

  // Load drives
  async function loadDrives() {
    try {
      drives = await api('/api/drives');
      renderDrives();
      loadRips();
    } catch (err) {
      console.error('Failed to load drives:', err);
    }
  }

  function renderDrives() {
    if (drives.length === 0) {
      driveList.innerHTML = '<div class="empty-state"><div class="icon">📀</div><p>No drives found</p></div>';
      return;
    }

    driveList.innerHTML = drives.map(d => {
      const stateClass = `state-${esc(d.state)}`;
      const isLoading = d.state === 'loading';
      const stateLabel = esc(isLoading ? 'loading disc' : d.state.replace('_', ' '));
      const isRipping = d.state === 'ripping';
      const isReady = d.state === 'ready';
      const isComplete = d.state === 'complete';
      const index = drives.indexOf(d);
      const activeRip = rips.find(r => r.drive_path === d.path && (r.state === 'ripping' || r.state === 'queued' || r.state === 'complete'));
      const displayTitle = esc(d.canonical_name || activeRip?.tmdb?.title || '');
      const displayYear = esc(d.canonical_year || (activeRip?.tmdb ? activeRip.tmdb.year : ''));
      const poster = d.poster || activeRip?.tmdb?.poster || '';
      const autoNamingChecked = d.auto_naming_enabled ? 'checked' : '';
      const autoRipChecked = d.autorip_enabled ? 'checked' : '';
      const hasName = !!(d.disc_name && displayTitle);
      const showNamingInput = d.disc_name && !d.canonical_name && isReady;

      let discInfoHtml = '';
      if (hasName) {
        const nameAction =
          activeRip && activeRip.state === 'queued'
            ? `<button class="btn btn-warning btn-small" onclick="undoAccept(${activeRip.id})">Undo</button>`
            : d.accepted_name
            ? `<button class="btn btn-warning btn-small" onclick="undoDriveAccept(${index})">Undo</button>`
            : `<button class="btn btn-success btn-small" onclick="acceptName(${index})">Accept</button>`;
        discInfoHtml = `<div class="disc-info"><span class="raw-name">${esc(d.disc_name)}</span> <span class="arrow">→</span> <span class="canonical-name">${displayTitle}</span><span class="year"> (${displayYear})</span>${nameAction}</div>`;
      } else if (showNamingInput) {
        discInfoHtml = `
          <div class="disc-info">
            <span class="raw-name">${esc(d.disc_name)}</span>
            <span class="arrow">→</span>
            <input type="text" class="name-input" id="name-input-${index}" placeholder="Enter name..." value="">
            <button class="btn btn-small" onclick="saveName(${index})">Save</button>
          </div>`;
      } else {
        discInfoHtml = `<div class="disc-info">${!d.has_disc ? 'No disc' : (esc(d.disc_name) || (isLoading ? 'Reading disc...' : 'Disc present'))}</div>`;
      }

      let nextUpHtml = '';
      if (d.next_up) {
        nextUpHtml = `<div class="next-up">Up next: S${String(d.next_up.season).padStart(2, '0')}E${String(d.next_up.episode).padStart(2, '0')}</div>`;
      }

      let lastRipHtml = '';
      if (d.last_rip) {
        const lrLabel = d.last_rip.media_type === 'tv' && d.last_rip.season > 0
          ? `S${String(d.last_rip.season).padStart(2, '0')}E${String(d.last_rip.episode || 1).padStart(2, '0')}`
          : '';
        lastRipHtml = `<div class="next-up">Last rip: ${esc(d.last_rip.title)}${d.last_rip.year ? ` (${esc(String(d.last_rip.year))})` : ''}${lrLabel ? ` ${lrLabel}` : ''}</div>`;
      }

      let suggestHtml = '';
      if (d.suggest) {
        suggestHtml = `<div class="next-up">Suggested for this disc: S${String(d.suggest.season).padStart(2, '0')}E${String(d.suggest.episode).padStart(2, '0')}</div>`;
      }

      return `
        <div class="drive-card" data-drive="${esc(d.path)}">
          <div class="drive-thumbnail clickable" role="button" aria-label="Search TMDb" onclick="openDriveSearch(${index})">
            ${poster ? `<img src="${esc(poster)}" class="poster" alt="">` : '<div class="poster placeholder"></div>'}
          </div>
          <div class="header">
            <span class="name">${esc(d.name)}</span>
            ${d.media_type === 'tv' ? '<span class="media-badge media-tv">TV</span>' : d.media_type === 'movie' ? '<span class="media-badge media-movie">Movie</span>' : ''}
            <span class="state-badge ${stateClass}">${stateLabel}</span>
          </div>
          ${discInfoHtml}
          ${nextUpHtml}
          ${lastRipHtml}
          ${suggestHtml}
          <label class="auto-naming-toggle">
            <input type="checkbox" ${autoNamingChecked} onchange="toggleAutoNaming(${index}, this.checked)">
            <span>Auto-Naming</span>
          </label>
          <label class="auto-naming-toggle">
            <input type="checkbox" ${d.auto_sequence_enabled ? 'checked' : ''} onchange="toggleAutoSequence(${index}, this.checked)">
            <span>Auto-Sequence</span>
          </label>
          <label class="auto-naming-toggle">
            <input type="checkbox" ${autoRipChecked} onchange="toggleAutoRip(${index}, this.checked)">
            <span>Autorip</span>
          </label>
          ${isRipping ? (() => {
            const p = driveProgressMap[d.path];
            const isScanning = p && p.rip_phase === 'scanning';
            if (isScanning) {
              return `
              <div class="progress-bar indeterminate">
                <div class="fill" id="progress-${esc(d.path)}"></div>
              </div>
              <div class="speed-info" id="speed-${esc(d.path)}">${esc(formatProgress(p, d.path))}</div>
            `;
            }
            return `
            <div class="progress-bar">
              <div class="fill" style="width: ${p ? p.percent : 0}%" id="progress-${esc(d.path)}"></div>
            </div>
            <div class="speed-info" id="speed-${esc(d.path)}">${esc(formatProgress(p, d.path))}</div>
          `;
          })() : ''}
          <div class="card-actions">
            ${isReady ? `<button class="btn btn-primary" onclick="startRip(${index})">Rip</button>` : ''}
            ${isLoading ? `<button class="btn btn-secondary" disabled>Loading...</button>` : ''}
            ${activeRip && (activeRip.state === 'ripping' || activeRip.state === 'queued') ? `<button class="btn btn-warning" onclick="abortRip(${activeRip.id})">Abort</button>` : ''}
            ${isComplete ? `<button class="btn btn-secondary" disabled>Complete</button>` : ''}
            <button class="btn btn-danger" onclick="ejectDrive(${index})">Eject</button>
          </div>
        </div>
      `;
    }).join('');
  }


  function formatETA(sec) {
    if (sec < 60) return `${Math.round(sec)}s`;
    const min = Math.floor(sec / 60);
    const s = Math.round(sec % 60);
    return s > 0 ? `${min}m ${s}s` : `${min}m`;
  }

  function formatProgress(p, drivePath) {
    if (!p) return '';
    if (p.rip_phase === 'scanning') {
      return p.phase || 'Scanning disc...';
    }
    const parts = [];
    if (p.title_total > 0) parts.push(`Saving title ${p.title_index} of ${p.title_total}`);
    parts.push(`${Math.round(p.percent)}%`);
    if (p.speed_mbps > 0) {
      parts.push(`${p.speed_mbps.toFixed(1)} MB/s`);
      // Remaining time from bytes written so far and the smoothed speed:
      // use the server's estimated total when available, else derive from pct.
      const pct = p.percent / 100;
      if (pct > 0 && p.speed_mbps > 0) {
        let totalBytes = p.estimated_total || (p.total_bytes / pct);
        const remainingSec = Math.round(((totalBytes - p.total_bytes) / (p.speed_mbps * 1024 * 1024)));
        if (remainingSec > 5) parts.push(`~${formatETA(remainingSec)} left`);
      }
    }
    return parts.join(' · ');
  }

  function updateProgress(drivePath, progress) {
    driveProgressMap[drivePath] = progress;
    const fill = document.getElementById(`progress-${drivePath}`);
    const speed = document.getElementById(`speed-${drivePath}`);
    if (fill) {
      if (progress.rip_phase === 'scanning') {
        // indeterminate: no width, CSS animation handles it
      } else {
        fill.style.width = `${progress.percent}%`;
      }
    }
    if (speed) speed.textContent = formatProgress(progress, drivePath);
  }

  // Load rips
  async function loadRips() {
    try {
      rips = await api('/api/rips');
      renderRips();
      renderDrives();
    } catch (err) {
      console.error('Failed to load rips:', err);
    }
  }

  function renderRips() {
    const finished = rips.filter(r => r.state === 'complete' || r.state === 'error');
    if (finished.length === 0) {
      ripList.innerHTML = '<div class="empty-state"><div class="icon">🎬</div><p>No finished rips</p></div>';
      return;
    }

    ripList.innerHTML = finished.map(r => {
      const poster = r.tmdb?.poster
        ? `<img src="${esc(r.tmdb.poster)}" class="poster clickable" role="button" aria-label="Search TMDb" onclick="openSearch(${r.id})" alt="">`
        : `<div class="poster placeholder clickable" role="button" aria-label="Search TMDb" onclick="openSearch(${r.id})"></div>`;
      const stateLabel = esc(r.state.replace('_', ' ').toUpperCase());
      const stateClass = `state-${esc(r.state)}`;
      const hasTitles = r.titles && r.titles.length > 0;

      // Build per-title rows
      let titlesHtml = '';
      if (hasTitles) {
        const allAccepted = r.titles.every(t => t.state === 'accepted');
        const anyPending = r.titles.some(t => t.state === 'pending');
        const anyAccepting = r.titles.some(t => t.state === 'accepting');
        const anyError = r.titles.some(t => t.state === 'error');

        titlesHtml = r.titles.map((t, ti) => {
          const isAccepted = t.state === 'accepted';
          const isPending = t.state === 'pending';
          const isAccepting = t.state === 'accepting';
          const isError = t.state === 'error';

          const statusIcon = isAccepted ? '✓' : isAccepting ? '⏳' : isError ? '✗' : '○';
          const statusColor = isAccepted ? 'var(--success)' : isAccepting ? 'var(--warning)' : isError ? 'var(--error)' : 'var(--muted)';
          const statusText = isAccepted ? 'Moved' : isAccepting ? 'Moving...' : isError ? t.error || 'Error' : 'Pending';

          const acceptedName = t.tmdb ? `${esc(t.tmdb.title)} (${esc(t.tmdb.year)})` : esc(t.accepted_name || t.source_file);
          const pathDisplay = t.proposed_destination ? `<div class="title-path">→ ${esc(t.proposed_destination)}</div>` : '';
          const pathAfter = t.destination ? `<div class="title-path">✓ ${esc(t.destination)}</div>` : '';

          let acceptBtn = '';
          if (isPending && anyPending && !allAccepted) {
            acceptBtn = `<button class="btn btn-success btn-small" onclick="acceptTitle(${r.id}, ${ti})" ${anyAccepting ? 'disabled' : ''}>Accept</button>`;
          }

          return `
            <div class="title-item ${isError ? 'title-error' : ''}">
              <span class="title-status" style="color: ${statusColor};">${statusIcon}</span>
              <span class="title-name">${acceptedName}</span>
              ${pathDisplay}
              ${pathAfter}
              ${isError ? `<span class="title-error-msg">${esc(t.error)}</span>` : ''}
              ${acceptBtn}
            </div>
          `;
        }).join('');
      }

      let actions = '';
      if (r.state === 'complete' && !hasTitles && r.tmdb) {
        actions = `<button class="btn btn-success" onclick="acceptRip(${r.id})">Accept & Move</button>`;
      } else if (r.state === 'complete' && hasTitles && allHaveTMDB(r.titles)) {
        const anyPending = r.titles.some(t => t.state === 'pending');
        if (anyPending) {
          actions = `<button class="btn btn-success" onclick="acceptAllTitles(${r.id})">Accept All & Move</button>`;
        }
      } else if (r.state === 'complete' && !hasTitles && !r.tmdb) {
        actions = `<button class="btn btn-primary" onclick="openSearch(${r.id})">Search</button>`;
      } else if (r.state === 'error') {
        actions = `<button class="btn btn-primary" onclick="openSearch(${r.id})">Search</button>`;
      }

      const autofillHtml = (r.media_type === 'tv' && r.season_number > 0)
        ? `<div class="next-up">Autofilled from library: starting S${String(r.season_number).padStart(2, '0')}E${String(r.episode_number || 1).padStart(2, '0')}</div>`
        : '';

      return `
        <div class="rip-item ${r.state === 'error' ? 'state-error' : ''}" data-rip-id="${r.id}">
          ${poster}
          <div class="info">
            <div class="title">${esc(r.disc_name)}${r.tmdb ? ` → ${esc(r.tmdb.title)} (${esc(r.tmdb.year)})` : ''}</div>
            <div class="meta">
              <span class="state-badge ${stateClass}">${stateLabel}</span>
              ${r.media_type === 'tv' ? '<span class="media-badge media-tv">TV</span>' : r.media_type === 'movie' ? '<span class="media-badge media-movie">Movie</span>' : ''}
              ${hasTitles ? `<span class="title-count">${r.titles.length} title(s)</span>` : ''}
            </div>
          </div>
          <div class="actions">${actions}</div>
          ${autofillHtml}
          ${hasTitles ? `<div class="titles-panel">${titlesHtml}</div>` : ''}
        </div>
      `;
    }).join('');
  }

  // Actions
  // driveQS returns the ?path= echo that guards against a stale index after
  // hotplug; empty string when the drive is unknown.
  function driveQS(index) {
    const d = drives[index];
    return d?.path ? `?path=${encodeURIComponent(d.path)}` : '';
  }

  window.startRip = async (index) => {
    try {
      await api(`/api/drives/${index}/start${driveQS(index)}`, { method: 'POST' });
      await loadDrives();
      await loadRips();
      toast('Rip started', 'info');
    } catch (err) {
      toast(`Failed to start rip: ${err.message}`, 'error');
    }
  };


  window.ejectDrive = async (index) => {
    try {
      await api(`/api/drives/${index}/eject${driveQS(index)}`, { method: 'POST' });
      toast('Disc ejected', 'info');
    } catch (err) {
      toast(`Failed to eject: ${err.message}`, 'error');
    }
  };

  window.toggleAutoNaming = async (index, enabled) => {
    try {
      await api(`/api/drives/${index}/auto-naming${driveQS(index)}`, {
        method: 'PUT',
        body: JSON.stringify({ enabled }),
      });
      toast(`Auto-naming ${enabled ? 'enabled' : 'disabled'}`, 'info');
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
      // Revert checkbox on failure
      loadDrives();
    }
  };

  window.toggleAutoSequence = async (index, enabled) => {
    try {
      await api(`/api/drives/${index}/auto-sequence${driveQS(index)}`, {
        method: 'PUT',
        body: JSON.stringify({ enabled }),
      });
      toast(`Auto-Sequence ${enabled ? 'enabled' : 'disabled'}`, 'info');
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
      loadDrives();
    }
  };

  window.toggleAutoRip = async (index, enabled) => {
    try {
      await api(`/api/drives/${index}/autorip${driveQS(index)}`, {
        method: 'PUT',
        body: JSON.stringify({ enabled }),
      });
      toast(`Autorip ${enabled ? 'enabled' : 'disabled'}`, 'info');
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
      loadDrives();
    }
  };

  window.saveName = async (index) => {
    const input = document.getElementById(`name-input-${index}`);
    if (!input || !input.value.trim()) {
      toast('Please enter a name', 'error');
      return;
    }
    try {
      await api(`/api/drives/${index}/name${driveQS(index)}`, {
        method: 'PUT',
        body: JSON.stringify({ canonical_name: input.value.trim() }),
      });
      toast('Name saved', 'success');
      loadDrives();
    } catch (err) {
      toast(`Failed to save name: ${err.message}`, 'error');
    }
  };

  window.acceptRip = async (id) => {
    const rip = rips.find(r => r.id === id);
    if (rip && !rip.tmdb) {
      openSearch(id);
      return;
    }
    try {
      const result = await api(`/api/rips/${id}/accept`, { method: 'POST' });
      if (result.status === 'queued') {
        toast('Accept queued - will execute when rip completes', 'info');
        loadRips();
        return;
      }
      toast('Accepted - moving to library', 'success');
      loadRips();
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  };

  window.acceptName = async (index) => {
    const d = drives[index];
    const activeRip = rips.find(r => r.drive_path === d.path && (r.state === 'ripping' || r.state === 'queued' || r.state === 'complete'));
    if (activeRip) {
      await acceptRip(activeRip.id);
      loadDrives();
      return;
    }
    try {
      await api(`/api/drives/${index}/accept${driveQS(index)}`, { method: 'POST' });
      toast('Name accepted - files will move after the rip', 'success');
      loadDrives();
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  };

  window.undoDriveAccept = async (index) => {
    try {
      await api(`/api/drives/${index}/undo-accept${driveQS(index)}`, { method: 'POST' });
      toast('Accept cancelled', 'info');
      loadDrives();
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  };

  window.abortRip = async (id) => {
    try {
      await api(`/api/rips/${id}/cancel`, { method: 'POST' });
      toast('Rip cancelled', 'info');
      loadDrives();
      loadRips();
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  };

    window.acceptTitle = async (ripId, titleIdx) => {
    try {
      const rip = rips.find(r => r.id === ripId);
      const title = rip?.titles?.[titleIdx];
      if (!title) return;

      const body = {
        title_name: title.source_file,
        index: titleIdx,
        year: title.year,
        is_main: title.is_main_file,
        tv_season: title.tv_season || 0,
        tv_episode: title.tv_episode || 0,
      };
      if (title.tmdb) {
        body.tmdb = title.tmdb;
      }

      await api(`/api/rips/${ripId}/accept-title`, {
        method: 'POST',
        body: JSON.stringify(body),
      });
      toast('Title moved', 'success');
      loadRips();
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  };

  function allHaveTMDB(titles) {
    return titles.every(t => t.tmdb);
  }

  window.acceptAllTitles = async (ripId) => {
    const rip = rips.find(r => r.id === ripId);
    if (!rip || !rip.titles) return;
    const pendingIdx = [];
    rip.titles.forEach((t, i) => {
      if (t.state === 'pending') pendingIdx.push(i);
    });
    for (const idx of pendingIdx) {
      try {
        await acceptTitle(ripId, idx);
      } catch (err) {
        toast(`Failed to move title: ${err.message}`, 'error');
        break;
      }
    }
  };

  window.undoAccept = async (id) => {
    try {
      await api(`/api/rips/${id}/undo-accept`, { method: 'POST' });
      toast('Accept cancelled', 'info');
      loadRips();
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  };

  function lastRipFor(d) { return d && d.last_rip ? d.last_rip : null; }

  window.openDriveSearch = (index) => {
    const activeRip = rips.find(r => r.drive_path === drives[index]?.path && (r.state === 'ripping' || r.state === 'queued'));
    if (activeRip) {
      searchMode = 'rip';
      searchRipId = activeRip.id;
      const title = activeRip.tmdb?.title || drives[index]?.canonical_name || drives[index]?.disc_name || 'Movie';
      openModal(`Search for ${title}`);
      searchInput.value = title;
      if (activeRip.tmdb) {
        renderSearchResults([activeRip.tmdb]);
      } else {
        runSearch(title);
      }
      return;
    }
    searchMode = 'drive';
    searchDriveIndex = index;
    const d = drives[index];
    const title = d?.canonical_name || d?.disc_name || (lastRipFor(d)?.title) || 'Movie';
    openModal(`Search for ${title}`);
    searchInput.value = title;
    if (d?.canonical_name) {
      renderSearchResults([{ id: d.canonical_id || 0, title: d.canonical_name, year: d.canonical_year || 0, poster: d.poster || '' }]);
    } else {
      runSearch(title);
    }
  };

  window.openSearch = (id) => {
    searchMode = 'rip';
    searchRipId = id;
    const rip = rips.find(r => r.id === id);
    const title = rip?.tmdb?.title || rip?.parsed_title || rip?.disc_name || lastRipFor(drives.find(x => x.path === rip?.drive_path))?.title || 'Movie';
    openModal(`Search for ${title}`);
    searchInput.value = title;
    if (rip?.tmdb) {
      renderSearchResults([rip.tmdb]);
    } else {
      runSearch(title);
    }
  };

  let searchTimeout = null;
  searchInput.addEventListener('input', () => {
    clearTimeout(searchTimeout);
    const query = searchInput.value;
    searchTimeout = setTimeout(() => runSearch(query), 500);
  });

  async function runSearch(query) {
    const seq = ++searchSeq;
    const q = query.trim();
    if (q.length < 2) {
      searchResultsList = [];
      searchResults.innerHTML = '';
      return;
    }
    try {
      const results = await api(`/api/search?q=${encodeURIComponent(q)}`);
      if (seq !== searchSeq) return;
      renderSearchResults(results);
    } catch (err) {
      if (seq !== searchSeq) return;
      searchResultsList = [];
      searchResults.innerHTML = `<p style="color: var(--error);">Search failed: ${esc(err.message)}</p>`;
    }
  }

  function renderSearchResults(results) {
    searchResultsList = results;
    if (results.length === 0) {
      searchResults.innerHTML = '<p style="color: var(--text-secondary); text-align: center;">No results found</p>';
      return;
    }
    searchResults.innerHTML = results.map((r, i) => `
      <div class="search-result" data-idx="${i}" role="button">
        ${r.poster ? `<img src="${esc(r.poster)}" class="poster" alt="">` : '<div class="poster"></div>'}
        <div class="info">
          <div class="title">${esc(r.title)} ${r.is_tv ? '<span class="media-badge media-tv">TV</span>' : '<span class="media-badge media-movie">Movie</span>'}</div>
          <div class="year">${esc(r.year || '')}</div>
        </div>
      </div>
    `).join('');
  }

  searchResults.addEventListener('click', (e) => {
    const el = e.target.closest('.search-result');
    if (!el) return;
    const r = searchResultsList[Number(el.dataset.idx)];
    if (r) selectSearchResult(r);
  });

  async function selectSearchResult(r) {
    const body = JSON.stringify({ tmdb_id: r.id, title: r.title, year: r.year, poster: r.poster || '', is_tv: !!r.is_tv });
    try {
      if (searchMode === 'drive') {
        await api(`/api/drives/${searchDriveIndex}/name${driveQS(searchDriveIndex)}`, { method: 'PUT', body });
        toast(`Selected: ${r.title}`, 'success');
        closeModal();
        loadDrives();
      } else if (searchMode === 'rip') {
        await api(`/api/rips/${searchRipId}/accept`, { method: 'POST', body });
        toast(`Selected: ${r.title}`, 'success');
        closeModal();
        loadRips();
      }
    } catch (err) {
      toast(`Failed: ${err.message}`, 'error');
    }
  }


  // Settings
  async function loadPathOptions() {
    try {
      const { paths } = await api('/api/paths');
      const dl = document.getElementById('path-options');
      dl.innerHTML = '';
      for (const p of paths || []) {
        const opt = document.createElement('option');
        opt.value = p;
        dl.appendChild(opt);
      }
    } catch (err) {
      console.error('Failed to load path suggestions:', err);
    }
  }

  // Directory browser modal
  function openDirModal(targetId) {
    dirModalTarget = targetId;
    const field = document.getElementById(targetId);
    browseDir(field.value.trim() || '');
    dirModal.classList.add('active');
  }

  function closeDirModal() {
    dirModal.classList.remove('active');
    dirModalTarget = null;
    dirModalDir = null;
  }

  async function browseDir(dir) {
    try {
      const data = await api(`/api/paths/browse?dir=${encodeURIComponent(dir)}`);
      renderDirListing(data);
    } catch (err) {
      if (dir && dir !== '/' && dir !== '') {
        toast('Directory not reachable, starting from /', 'error');
        browseDir('/');
      } else {
        dirList.innerHTML = `<p style="color: var(--error);">Failed to list: ${esc(err.message)}</p>`;
      }
    }
  }

  function renderDirListing(data) {
    dirModalDir = data.dir;
    dirBreadcrumb.textContent = data.dir;
    let html = '';
    if (data.parent !== '') {
      html += `<div class="dir-result dir-up" data-path="${esc(data.parent)}" role="button">.. ${esc(data.parent)}</div>`;
    }
    for (const d of data.dirs || []) {
      html += `<div class="dir-result" data-path="${esc(d.path)}" role="button">${esc(d.name)}</div>`;
    }
    if (data.parent === '' && !(data.dirs || []).length) {
      html = '<p style="color: var(--text-secondary);">No subdirectories</p>';
    }
    dirList.innerHTML = html;
  }

  dirList.addEventListener('click', (e) => {
    const el = e.target.closest('.dir-result');
    if (el) browseDir(el.dataset.path);
  });

  document.querySelectorAll('.dir-browse').forEach(btn =>
    btn.addEventListener('click', () => openDirModal(btn.dataset.target)));
  document.getElementById('dir-modal-close').addEventListener('click', closeDirModal);
  document.getElementById('dir-cancel').addEventListener('click', closeDirModal);
  dirModal.addEventListener('click', (e) => {
    if (e.target === dirModal) closeDirModal();
  });
  document.getElementById('dir-use').addEventListener('click', () => {
    if (dirModalTarget && dirModalDir) {
      document.getElementById(dirModalTarget).value = dirModalDir;
    }
    closeDirModal();
  });

  async function loadSettings() {
    try {
      const cfg = await api('/api/config');
      document.getElementById('movies_dir').value = cfg.movies_dir || '';
      document.getElementById('tv_shows_dir').value = cfg.tv_shows_dir || '';
      document.getElementById('temp_dir').value = cfg.temp_dir || '';
      document.getElementById('http_port').value = cfg.http_port || 3000;
      document.getElementById('auto_sequence').checked = cfg.auto_sequence !== false;
      document.getElementById('temp_fill_limit_percent').value = cfg.temp_fill_limit_percent ?? 80;
      document.getElementById('move_fill_limit_percent').value = cfg.move_fill_limit_percent ?? 95;
    } catch (err) {
      console.error('Failed to load settings:', err);
    }
  }

  settingsForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const updates = {
      movies_dir: document.getElementById('movies_dir').value,
      tv_shows_dir: document.getElementById('tv_shows_dir').value,
      temp_dir: document.getElementById('temp_dir').value,
      http_port: parseInt(document.getElementById('http_port').value) || 3000,
      auto_sequence: document.getElementById('auto_sequence').checked,
      temp_fill_limit_percent: parseInt(document.getElementById('temp_fill_limit_percent').value) || 0,
      move_fill_limit_percent: parseInt(document.getElementById('move_fill_limit_percent').value) || 0,
    };

    const tmdbKey = document.getElementById('tmdb_api_key').value;
    if (tmdbKey) {
      updates.tmdb_api_key = tmdbKey;
    }

    try {
      await api('/api/config', {
        method: 'PUT',
        body: JSON.stringify(updates),
      });
      toast('Settings saved', 'success');
      document.getElementById('tmdb_api_key').value = '';
    } catch (err) {
      toast(`Failed to save: ${err.message}`, 'error');
    }
  });

  // Library
  async function loadLibrary() {
    try {
      libraryData = await api('/api/library');
      renderLibrary();
      renderDrives();
    } catch (err) {
      console.error('Failed to load library:', err);
    }
  }

  function formatEpisodes(eps) {
    // Join episode numbers; use an en-dash for runs of 3 or more.
    const sorted = [...eps].sort((a, b) => a - b);
    const parts = [];
    let i = 0;
    while (i < sorted.length) {
      let j = i;
      while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
      if (j - i >= 2) {
        parts.push(`${sorted[i]}–${sorted[j]}`);
      } else {
        for (let k = i; k <= j; k++) parts.push(String(sorted[k]));
      }
      i = j + 1;
    }
    return parts.join(', ');
  }

  function renderLibrary() {
    const summary = document.getElementById('library-summary');
    const seriesEl = document.getElementById('library-series');
    const moviesEl = document.getElementById('library-movies');

    const scanned = libraryData.scanned_at || 'never';
    summary.innerHTML = `<p>${esc(String(libraryData.movies || 0))} movies · ${esc(String(libraryData.episodes || 0))} episodes across ${esc(String((libraryData.series || []).length))} series · last scan ${esc(scanned)}</p>`;

    const series = libraryData.series || [];
    if (series.length === 0) {
      seriesEl.innerHTML = '<div class="empty-state"><p>No TV series tracked</p></div>';
    } else {
      seriesEl.innerHTML = series.map(s => {
        const titleHtml = s.year ? `${esc(s.title)} (${esc(String(s.year))})` : esc(s.title);
        const seasonsHtml = (s.seasons || []).map(ss =>
          `<div class="season-line">Season ${esc(String(ss.season))}: ${esc(formatEpisodes(ss.episodes || []))}</div>`
        ).join('');
        const nextHtml = s.next_season > 0
          ? `<div class="next-up">Up next: S${String(s.next_season).padStart(2, '0')}E${String(s.next_episode).padStart(2, '0')}</div>`
          : '';
        return `<div class="series-item"><div class="series-title">${titleHtml}</div>${seasonsHtml}${nextHtml}</div>`;
      }).join('');
    }

    const movies = (libraryData.entries || []).filter(e => e.media_type === 'movie');
    if (movies.length === 0) {
      moviesEl.innerHTML = '<div class="empty-state"><p>No movies tracked</p></div>';
    } else {
      moviesEl.innerHTML = movies.map(m => {
        const label = m.year ? `${esc(m.title)} (${esc(String(m.year))})` : esc(m.title);
        const missingTag = m.missing ? '<span class="missing-tag">missing</span>' : '';
        return `<div class="library-item">${label}${missingTag}</div>`;
      }).join('');
    }
  }

  function enableScanButton() {
    const btn = document.getElementById('scan-library');
    if (btn) btn.disabled = false;
  }

  function hideResetConfirm() {
    const c = document.getElementById('reset-confirm');
    if (c) c.classList.add('hidden');
    const yes = document.getElementById('reset-confirm-yes');
    const reset = document.getElementById('reset-library');
    if (yes) yes.disabled = false;
    if (reset) reset.disabled = false;
  }

  // Scan button: the SSE event re-enables it, never this handler.
  document.getElementById('scan-library').addEventListener('click', async () => {
    const btn = document.getElementById('scan-library');
    btn.disabled = true;
    try {
      await api('/api/library/scan', { method: 'POST' });
      toast('Scanning library…', 'info');
    } catch (err) {
      btn.disabled = false;
      toast(`Scan failed: ${err.message}`, 'error');
    }
  });

  document.getElementById('reset-library').addEventListener('click', () => {
    document.getElementById('reset-confirm').classList.remove('hidden');
  });

  document.getElementById('reset-cancel').addEventListener('click', hideResetConfirm);

  document.getElementById('reset-confirm-yes').addEventListener('click', async () => {
    const resetBtn = document.getElementById('reset-library');
    const yesBtn = document.getElementById('reset-confirm-yes');
    resetBtn.disabled = true;
    yesBtn.disabled = true;
    document.getElementById('reset-confirm').classList.add('hidden');
    try {
      await api('/api/library/reset', { method: 'POST' });
      toast('Library database reset', 'success');
      loadLibrary();
    } catch (err) {
      resetBtn.disabled = false;
      yesBtn.disabled = false;
      document.getElementById('reset-confirm').classList.remove('hidden');
      toast(`Reset failed: ${err.message}`, 'error');
    }
  });

  // Toast notifications
  function toast(message, type = 'info') {
    const container = document.querySelector('.toast-container') || createToastContainer();
    const t = document.createElement('div');
    t.className = `toast ${type}`;
    t.textContent = message;
    container.appendChild(t);
    setTimeout(() => t.remove(), 4000);
  }

  function createToastContainer() {
    const c = document.createElement('div');
    c.className = 'toast-container';
    document.body.appendChild(c);
    return c;
  }

  // MakeMKV availability banner (GET /api/status). The install needs sudo,
  // so it happens in a terminal; the banner only points there. The server
  // re-probes MakeMKV every few seconds, so once the installer finishes the
  // banner clears without a reload.
  let makemkvBanner = null;
  async function checkMakemkv() {
    let status;
    try {
      status = await api('/api/status');
    } catch (_) {
      return; // server unreachable; the shutdown banner owns that case
    }
    if (status.makemkv_available) {
      if (makemkvBanner) {
        makemkvBanner.remove();
        makemkvBanner = null;
        toast('MakeMKV detected — ripping enabled', 'success');
      }
      return;
    }
    if (!makemkvBanner) {
      makemkvBanner = document.createElement('div');
      makemkvBanner.className = 'makemkv-banner';
      makemkvBanner.innerHTML =
        'MakeMKV is not installed — Riply cannot rip. Install it in the ' +
        'terminal: <code>bash scripts/install-makemkv.sh</code> ' +
        '(or run <code>./riply</code> and answer yes to the install prompt). ' +
        'It asks for sudo once; this page updates automatically when it is done.';
      document.body.prepend(makemkvBanner);
    }
  }

  // Initialize
  loadDrives();
  loadRips();
  loadSettings();
  loadPathOptions();
  loadLibrary();
  connectSSE();
  checkMakemkv();

  // Refresh drives periodically
  setInterval(loadDrives, 5000);
  setInterval(loadRips, 10000);
  setInterval(checkMakemkv, 5000);
})();
