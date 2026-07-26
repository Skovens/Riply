/**
 * Riply Web Application
 *
 * Main entry point. Initializes the UI, connects to SSE,
 * and sets up global event handlers.
 */

import { healthCheck, scan, analyze, move, review, connectSSE } from './api.js';
import { store } from './store.js';
import { escapeHtml } from './utils.js';
import { renderDashboard } from './components/dashboard.js';
import { renderJobs, stopJobsPolling } from './components/jobs.js';
import { renderFiles } from './components/files.js';
import { renderReview } from './components/review.js';
import { renderRipper, stopRipperPolling } from './components/ripper.js';
import { renderSettings } from './components/settings.js';

class RiplyApp {
  constructor() {
    this.currentView = 'dashboard';
    this.unsubscribeSSE = null;
  }

  async init() {
    console.log('Riply initializing...');

    // Check backend connection
    const connected = await healthCheck();
    store.set({ connected });

    if (!connected) {
      this.showError('Cannot connect to Riply backend');
      return;
    }

    // Load config for default scan directory
    try {
      const { config: cfg } = await import('./api.js');
      const result = await cfg.get();
      if (result.data) store.set({ config: result.data });
    } catch (_) {}

    // Handle hash routing
    this.handleRoute();
    window.addEventListener('hashchange', () => this.handleRoute());

    // Connect to SSE for live updates
    this.connectSSE();

    // Expose global functions for inline handlers
    this.setupGlobals();

    console.log('Riply ready');
  }

  handleRoute() {
    const hash = window.location.hash.slice(1) || '/';
    const view = hash.replace(/^\//, '') || 'dashboard';
    this.currentView = view;
    stopJobsPolling();
    stopRipperPolling();
    this.renderView();
    this.updateNav();
  }

  updateNav() {
    document.querySelectorAll('.header__nav-link').forEach(link => {
      const view = link.dataset.view;
      link.classList.toggle('header__nav-link--active', view === this.currentView);
    });
  }

  renderView() {
    const main = document.getElementById('main-content');
    if (!main) return;

    main.innerHTML = '';

    switch (this.currentView) {
      case 'files':
        renderFiles(main);
        break;
      case 'review':
        renderReview(main);
        break;
      case 'ripper':
        renderRipper(main);
        break;
      case 'jobs':
        renderJobs(main);
        break;
      case 'settings':
        renderSettings(main);
        break;
      case 'dashboard':
      default:
        renderDashboard(main);
        break;
    }
  }

  connectSSE() {
    this.unsubscribeSSE = connectSSE((message) => {
      console.log('SSE:', message);
      store.set({ lastEvent: message });

      // Refresh view on relevant events
      if (message.startsWith('review:') || message.startsWith('scan:') ||
          message.startsWith('analyze:') || message.startsWith('move:') ||
          message.startsWith('job:')) {
        if (this.currentView === 'jobs') {
          this.renderView();
        }
      }
    });
  }

  setupGlobals() {
    window.riply = {
      scan: async () => {
        const config = store.state.config;
        const defaultDir = config?.scan_directory || '.';
        const directory = prompt('Directory to scan:', defaultDir);
        if (!directory) return;

        try {
          const result = await scan.start(directory);
          this.showToast(`Scan started (job ${result.data?.job_id})`, 'success');
          window.location.hash = '#/jobs';
        } catch (error) {
          this.showToast(`Failed to start scan: ${error.message}`, 'error');
        }
      },

      analyze: async () => {
        try {
          const result = await analyze.start();
          this.showToast(`Analysis started (job ${result.data?.job_id})`, 'success');
          window.location.hash = '#/jobs';
        } catch (error) {
          this.showToast(`Failed to start analysis: ${error.message}`, 'error');
        }
      },

      moveFiles: async () => {
        if (!confirm('Move all confirmed files to the library?')) return;

        try {
          const result = await move.start();
          this.showToast(`Move started (job ${result.data?.job_id})`, 'success');
          window.location.hash = '#/jobs';
        } catch (error) {
          this.showToast(`Failed to start move: ${error.message}`, 'error');
        }
      },

      review: {
        accept: async (id) => {
          try {
            await review.accept(id);
            this.showToast('File accepted', 'success');
            this.renderView();
          } catch (error) {
            this.showToast(`Failed to accept: ${error.message}`, 'error');
          }
        },
        reject: async (id) => {
          try {
            await review.reject(id);
            this.showToast('File rejected', 'success');
            this.renderView();
          } catch (error) {
            this.showToast(`Failed to reject: ${error.message}`, 'error');
          }
        },
      },

      navigate: (view) => {
        window.location.hash = `#/${view}`;
      },
    };
  }

  showError(message) {
    const main = document.getElementById('main-content');
    if (main) {
      main.innerHTML = `
        <div class="empty" style="padding-top: 100px;">
          <div class="empty__icon">&#9888;</div>
          <div class="empty__title">${escapeHtml(message)}</div>
          <div class="empty__description">Make sure the server is running</div>
        </div>
      `;
    }
  }

  showToast(message, type = 'default') {
    const existing = document.querySelector('.toast');
    if (existing) existing.remove();

    const toast = document.createElement('div');
    toast.className = `toast toast--${type}`;
    toast.textContent = message;
    document.body.appendChild(toast);

    requestAnimationFrame(() => {
      toast.classList.add('toast--visible');
    });

    setTimeout(() => {
      toast.classList.remove('toast--visible');
      setTimeout(() => toast.remove(), 200);
    }, 3000);
  }

  destroy() {
    if (this.unsubscribeSSE) {
      this.unsubscribeSSE();
    }
    stopJobsPolling();
    stopRipperPolling();
  }
}

// ── Initialize ───────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  const app = new RiplyApp();
  app.init();
});
