/**
 * Riply Web Application
 * 
 * Main entry point. Initializes the UI, connects to SSE,
 * and sets up global event handlers.
 */

import { healthCheck, review, connectSSE } from './api.js';
import { store } from './store.js';
import { renderDashboard } from './components/dashboard.js';

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

    // Render initial view
    this.renderView();

    // Connect to SSE for live updates
    this.connectSSE();

    // Expose global functions for inline handlers
    this.setupGlobals();

    console.log('Riply ready');
  }

  renderView() {
    const main = document.getElementById('main-content');
    if (!main) return;

    // Clear loading state
    main.innerHTML = '';

    switch (this.currentView) {
      case 'dashboard':
        renderDashboard(main);
        break;
      default:
        renderDashboard(main);
    }
  }

  connectSSE() {
    this.unsubscribeSSE = connectSSE((message) => {
      console.log('SSE:', message);
      store.set({ lastEvent: message });

      // Handle specific events
      if (message.startsWith('review:')) {
        // Refresh review queue
        this.renderView();
      } else if (message.startsWith('rip:')) {
        // Refresh drives
        this.renderView();
      }
    });
  }

  setupGlobals() {
    // Expose review actions for inline onclick handlers
    window.riply = {
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

    // Trigger animation
    requestAnimationFrame(() => {
      toast.classList.add('toast--visible');
    });

    // Auto-remove after 3 seconds
    setTimeout(() => {
      toast.classList.remove('toast--visible');
      setTimeout(() => toast.remove(), 200);
    }, 3000);
  }

  destroy() {
    if (this.unsubscribeSSE) {
      this.unsubscribeSSE();
    }
  }
}

// ── Initialize ───────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
  const app = new RiplyApp();
  app.init();
});

// ── Helpers ──────────────────────────────────────────────────

function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
