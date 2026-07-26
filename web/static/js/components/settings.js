import { config } from '../api.js';
import { store } from '../store.js';
import { escapeHtml } from '../utils.js';

let currentConfig = null;
let dirty = {};

export function renderSettings(container) {
  dirty = {};
  container.innerHTML = `
    <div class="page">
      <div class="page__header">
        <h2 class="page__title">Settings</h2>
      </div>
      <div class="page__body" id="settings-content">
        <div class="empty">
          <div class="spinner" style="margin: 0 auto var(--space-4);"></div>
          <div class="empty__title">Loading settings...</div>
        </div>
      </div>
    </div>
  `;
  loadConfig();
}

async function loadConfig() {
  const el = document.getElementById('settings-content');
  try {
    const res = await config.get();
    currentConfig = res.data;
    store.set('config', currentConfig);
    renderForm(currentConfig);
  } catch (error) {
    console.error('Failed to load config:', error);
    if (el) {
      el.innerHTML =
        '<div class="empty"><div class="empty__title">Failed to load settings</div></div>';
    }
  }
}

function renderForm(config) {
  const el = document.getElementById('settings-content');
  if (!el) return;

  el.innerHTML = `
    <form id="settings-form" autocomplete="off">
      ${renderSection('Directories', [
        inputField('scan_directory', 'Scan Directory', config.scan_directory || '', 'text', 'Path to scan for media files'),
        inputField('movies_directory', 'Movies Directory', config.movies_directory || '', 'text', 'Destination for movie files'),
        inputField('tv_directory', 'TV Directory', config.tv_directory || '', 'text', 'Destination for TV show files'),
      ])}
      ${renderSection('Metadata APIs', [
        inputField('tmdb_api_key', 'TMDb API Key', config.tmdb_api_key || '', 'password', 'Get one at themoviedb.org/settings/api'),
        inputField('omdb_api_key', 'OMDb API Key', config.omdb_api_key || '', 'password', 'Get one at omdbapi.com/apikey.aspx'),
      ])}
      ${renderSection('Server', [
        inputField('host', 'Bind Address', config.host || '0.0.0.0', 'text'),
        inputField('port', 'Port', config.port || 3000, 'number'),
      ])}
      ${renderSection('Ripping', [
        inputField('ripping_command', 'Ripping Command', config.ripping?.command || '', 'text', 'Auto-detected if empty (makemkvcon)'),
      ])}
      ${renderSection('Advanced', [
        inputField('database_path', 'Database Path', config.database_path || 'riply.db', 'text'),
        toggleField('verbose', 'Verbose Logging', config.verbose || false),
      ])}
      <div class="settings__actions">
        <button type="submit" class="btn btn--primary">Save Settings</button>
        <span class="settings__status" id="settings-status"></span>
      </div>
    </form>
  `;

  const form = document.getElementById('settings-form');
  form.addEventListener('submit', handleSubmit);

  form.querySelectorAll('input').forEach(input => {
    input.addEventListener('input', () => {
      const key = input.name;
      const original = getOriginalValue(key, config);
      const current = input.type === 'checkbox' ? input.checked : input.value;
      if (current !== original) {
        dirty[key] = current;
      } else {
        delete dirty[key];
      }
      updateStatus('');
    });
  });

  document.querySelectorAll('.settings__toggle-vis').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.preventDefault();
      const target = document.getElementById(btn.dataset.target);
      if (target) {
        target.type = target.type === 'password' ? 'text' : 'password';
      }
    });
  });
}

function getOriginalValue(key, config) {
  if (key === 'ripping_command') return config.ripping?.command || '';
  if (key === 'verbose') return config.verbose || false;
  return config[key] ?? '';
}

async function handleSubmit(e) {
  e.preventDefault();
  updateStatus('');

  if (Object.keys(dirty).length === 0) {
    updateStatus('No changes to save', 'muted');
    return;
  }

  const payload = {};
  for (const [key, value] of Object.entries(dirty)) {
    if (key === 'port') {
      payload[key] = parseInt(value, 10);
    } else if (key === 'verbose') {
      payload[key] = !!value;
    } else {
      payload[key] = value || '';
    }
  }

  try {
    const res = await config.update(payload);
    currentConfig = res.data;
    store.set('config', currentConfig);
    dirty = {};
    updateStatus('Settings saved', 'success');
    renderForm(currentConfig);
    if (window.riply?.toast) {
      window.riply.toast('Settings saved successfully', 'success');
    }
  } catch (error) {
    console.error('Failed to save config:', error);
    updateStatus('Failed to save: ' + error.message, 'error');
  }
}

function updateStatus(msg, type = '') {
  const el = document.getElementById('settings-status');
  if (el) {
    el.textContent = msg;
    el.className = 'settings__status' + (type ? ` settings__status--${type}` : '');
  }
}

function renderSection(title, fields) {
  return `
    <div class="settings__section">
      <h3 class="settings__section-title">${escapeHtml(title)}</h3>
      ${fields.join('')}
    </div>
  `;
}

function inputField(name, label, value, type = 'text', hint = '') {
  const isPassword = type === 'password';
  const displayValue = type === 'number' ? value : escapeHtml(String(value));

  return `
    <div class="settings__field">
      <label class="settings__label" for="settings-${name}">${escapeHtml(label)}</label>
      <div class="settings__input-wrap">
        <input
          class="settings__input"
          type="${type}"
          id="settings-${name}"
          name="${name}"
          value="${displayValue}"
          ${type === 'number' ? 'min="0" max="65535"' : ''}
          autocomplete="off"
        />
        ${isPassword ? `<button class="settings__toggle-vis" data-target="settings-${name}" type="button">Show</button>` : ''}
      </div>
      ${hint ? `<div class="settings__hint">${escapeHtml(hint)}</div>` : ''}
    </div>
  `;
}

function toggleField(name, label, checked) {
  return `
    <div class="settings__field settings__field--toggle">
      <label class="settings__label" for="settings-${name}">${escapeHtml(label)}</label>
      <label class="toggle">
        <input type="checkbox" id="settings-${name}" name="${name}" ${checked ? 'checked' : ''} />
        <span class="toggle__track"></span>
      </label>
    </div>
  `;
}
