<script>
  import { onMount } from 'svelte';
  import { configApi } from '$lib/api';

  let currentConfig = $state(null);
  let saving = $state(false);
  let saveStatus = $state({ message: '', type: '' });

  let scan_directory = $state('');
  let movies_directory = $state('');
  let tv_directory = $state('');
  let tmdb_api_key = $state('');
  let omdb_api_key = $state('');
  let host = $state('');
  let port = $state(3000);
  let ripping_command = $state('');
  let rip_output_directory = $state('');
  let database_path = $state('');
  let verbose = $state(false);
  let healthchecks_uuid = $state('');
  let healthchecks_ping_interval_minutes = $state(5);

  let show_tmdb = $state(false);
  let show_omdb = $state(false);
  let show_healthchecks = $state(false);

  onMount(async () => {
    try {
      currentConfig = await configApi.get();
      scan_directory = currentConfig.scan_directory || '';
      movies_directory = currentConfig.movies_directory || '';
      tv_directory = currentConfig.tv_directory || '';
      tmdb_api_key = currentConfig.tmdb_api_key || '';
      omdb_api_key = currentConfig.omdb_api_key || '';
      host = currentConfig.host || '0.0.0.0';
      port = currentConfig.port || 3000;
      ripping_command = currentConfig.ripping?.command || '';
      rip_output_directory = currentConfig.rip_output_directory || '';
      database_path = currentConfig.database_path || 'riply.db';
      verbose = currentConfig.verbose || false;
      healthchecks_uuid = currentConfig.healthchecks?.uuid || '';
      healthchecks_ping_interval_minutes = currentConfig.healthchecks?.ping_interval_minutes || 5;
    } catch (e) {
      console.error('Failed to load config:', e);
    }
  });

  async function handleSubmit(e) {
    e.preventDefault();
    saveStatus = { message: '', type: '' };
    saving = true;

    try {
      const payload = {
        scan_directory: scan_directory || '',
        movies_directory: movies_directory || '',
        tv_directory: tv_directory || '',
        tmdb_api_key: tmdb_api_key || '',
        omdb_api_key: omdb_api_key || '',
        host: host || '0.0.0.0',
        port: parseInt(port, 10),
        ripping_command: ripping_command || '',
        rip_output_directory: rip_output_directory || '',
        database_path: database_path || 'riply.db',
        verbose,
        healthchecks_uuid: healthchecks_uuid || '',
        healthchecks_ping_interval_minutes: parseInt(healthchecks_ping_interval_minutes, 10) || 5,
      };

      const updated = await configApi.update(payload);
      currentConfig = updated;
      saveStatus = { message: 'Settings saved', type: 'success' };
    } catch (e) {
      console.error('Failed to save config:', e);
      saveStatus = { message: 'Failed to save: ' + e.message, type: 'error' };
    } finally {
      saving = false;
    }
  }
</script>

{#if !currentConfig}
  <div class="empty">
    <div class="spinner" style="margin: 0 auto var(--space-4);"></div>
    <div class="empty__title">Loading settings...</div>
  </div>
{:else}
  <div class="page">
    <div class="page__header">
      <h2 class="page__title">Settings</h2>
    </div>
    <div class="page__body">
      <form autocomplete="off" onsubmit={handleSubmit}>
        <!-- Directories -->
        <div class="settings__section">
          <h3 class="settings__section-title">Directories</h3>

          <div class="settings__field">
            <label class="settings__label" for="settings-scan_directory">Scan Directory</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-scan_directory" bind:value={scan_directory} autocomplete="off" />
            </div>
            <div class="settings__hint">Path to scan for media files</div>
          </div>

          <div class="settings__field">
            <label class="settings__label" for="settings-movies_directory">Movies Directory</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-movies_directory" bind:value={movies_directory} autocomplete="off" />
            </div>
            <div class="settings__hint">Destination for movie files</div>
          </div>

          <div class="settings__field">
            <label class="settings__label" for="settings-tv_directory">TV Directory</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-tv_directory" bind:value={tv_directory} autocomplete="off" />
            </div>
            <div class="settings__hint">Destination for TV show files</div>
          </div>
        </div>

        <!-- Metadata APIs -->
        <div class="settings__section">
          <h3 class="settings__section-title">Metadata APIs</h3>

          <div class="settings__field">
            <label class="settings__label" for="settings-tmdb_api_key">TMDb API Key</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type={show_tmdb ? 'text' : 'password'} id="settings-tmdb_api_key" bind:value={tmdb_api_key} autocomplete="off" />
              <button class="settings__toggle-vis" type="button" onclick={() => show_tmdb = !show_tmdb}>{show_tmdb ? 'Hide' : 'Show'}</button>
            </div>
            <div class="settings__hint">Get one at themoviedb.org/settings/api</div>
          </div>

          <div class="settings__field">
            <label class="settings__label" for="settings-omdb_api_key">OMDb API Key</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type={show_omdb ? 'text' : 'password'} id="settings-omdb_api_key" bind:value={omdb_api_key} autocomplete="off" />
              <button class="settings__toggle-vis" type="button" onclick={() => show_omdb = !show_omdb}>{show_omdb ? 'Hide' : 'Show'}</button>
            </div>
            <div class="settings__hint">Get one at omdbapi.com/apikey.aspx</div>
          </div>
        </div>

        <!-- Server -->
        <div class="settings__section">
          <h3 class="settings__section-title">Server</h3>

          <div class="settings__field">
            <label class="settings__label" for="settings-host">Bind Address</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-host" bind:value={host} autocomplete="off" />
            </div>
          </div>

          <div class="settings__field">
            <label class="settings__label" for="settings-port">Port</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="number" id="settings-port" bind:value={port} min="0" max="65535" autocomplete="off" />
            </div>
          </div>
        </div>

        <!-- Ripping -->
        <div class="settings__section">
          <h3 class="settings__section-title">Ripping</h3>

          <div class="settings__field">
            <label class="settings__label" for="settings-ripping_command">Ripping Command</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-ripping_command" bind:value={ripping_command} autocomplete="off" />
            </div>
            <div class="settings__hint">Auto-detected if empty (makemkvcon)</div>
          </div>

          <div class="settings__field">
            <label class="settings__label" for="settings-rip_output_directory">Rip Output Directory</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-rip_output_directory" bind:value={rip_output_directory} autocomplete="off" />
            </div>
            <div class="settings__hint">Where ripped disc content is saved. Falls back to scan directory if empty.</div>
          </div>
        </div>

        <!-- Monitoring -->
        <div class="settings__section">
          <h3 class="settings__section-title">Monitoring</h3>

          <div class="settings__field">
            <label class="settings__label" for="settings-healthchecks_uuid">healthchecks.io UUID</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type={show_healthchecks ? 'text' : 'password'} id="settings-healthchecks_uuid" bind:value={healthchecks_uuid} autocomplete="off" />
              <button class="settings__toggle-vis" type="button" onclick={() => show_healthchecks = !show_healthchecks}>{show_healthchecks ? 'Hide' : 'Show'}</button>
            </div>
            <div class="settings__hint">Enables server heartbeat pings. Get one at healthchecks.io</div>
          </div>

          <div class="settings__field">
            <label class="settings__label" for="settings-healthchecks_interval">Ping Interval (minutes)</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="number" id="settings-healthchecks_interval" bind:value={healthchecks_ping_interval_minutes} min="1" autocomplete="off" />
            </div>
          </div>
        </div>

        <!-- Advanced -->
        <div class="settings__section">
          <h3 class="settings__section-title">Advanced</h3>

          <div class="settings__field">
            <label class="settings__label" for="settings-database_path">Database Path</label>
            <div class="settings__input-wrap">
              <input class="settings__input" type="text" id="settings-database_path" bind:value={database_path} autocomplete="off" />
            </div>
          </div>

          <div class="settings__field settings__field--toggle">
            <label class="settings__label" for="settings-verbose">Verbose Logging</label>
            <label class="toggle">
              <input type="checkbox" id="settings-verbose" bind:checked={verbose} />
              <span class="toggle__track"></span>
            </label>
          </div>
        </div>

        <div class="settings__actions">
          <button type="submit" class="btn btn--primary" disabled={saving}>Save Settings</button>
          <span class="settings__status settings__status--{saveStatus.type}">{saveStatus.message}</span>
        </div>
      </form>
    </div>
  </div>
{/if}
