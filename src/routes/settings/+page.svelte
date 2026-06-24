<script lang="ts">
  import { themeManager } from '$lib/theme.svelte';

  const isLinux = () => {
    try {
      const ua = navigator.userAgent || '';
      return /linux|x11/i.test(ua) && !/android/i.test(ua);
    } catch {
      return false;
    }
  };

  const dragRegionEnabled = !isLinux();

  let activeTab = $state('general');
  let autoCheckUpdates = $state(true);
  let minimizeToTray = $state(false);
</script>

<main class="settings-container">
  {#if dragRegionEnabled}
    <div class="drag-region" aria-hidden="true" data-tauri-drag-region></div>
  {/if}

  <div class="settings-body">
    <!-- Sidebar -->
    <aside class="settings-sidebar">
      <a href="/" class="back-link" title="Back to Timeline">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="19" y1="12" x2="5" y2="12" />
          <polyline points="12 19 5 12 12 5" />
        </svg>
        <span>Back</span>
      </a>

      <div class="sidebar-divider"></div>

      <button
        class="sidebar-item"
        class:is-active={activeTab === 'general'}
        onclick={() => activeTab = 'general'}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
        <span>General</span>
      </button>

      <button
        class="sidebar-item"
        class:is-active={activeTab === 'appearance'}
        onclick={() => activeTab = 'appearance'}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"/>
          <path d="M12 2a7 7 0 1 0 10 10"/>
          <path d="M12 6a6 6 0 1 0 6 6"/>
        </svg>
        <span>Appearance</span>
      </button>
    </aside>

    <!-- Content Pane -->
    <section class="settings-content-pane">
      {#if activeTab === 'general'}
        <div class="settings-section">
          <h3>General Settings</h3>

          <div class="setting-row">
            <div class="setting-info">
              <label for="auto-check-updates">Check for Updates Automatically</label>
              <span class="setting-desc">Check for new versions of the application upon startup</span>
            </div>
            <div class="setting-control">
              <input type="checkbox" id="auto-check-updates" bind:checked={autoCheckUpdates} class="native-switch" />
            </div>
          </div>

          <div class="setting-row">
            <div class="setting-info">
              <label for="minimize-to-tray">Minimize to System Tray on Close</label>
              <span class="setting-desc">Keep the application running in the background when closing the main window</span>
            </div>
            <div class="setting-control">
              <input type="checkbox" id="minimize-to-tray" bind:checked={minimizeToTray} class="native-switch" />
            </div>
          </div>
        </div>
      {:else if activeTab === 'appearance'}
        <div class="settings-section">
          <h3>App Theme</h3>

          <div class="setting-row">
            <div class="setting-info">
              <label for="theme-select">Theme Selection</label>
              <span class="setting-desc">Choose between System default, Light mode, or Dark mode</span>
            </div>
            <div class="setting-control">
              <select
                id="theme-select"
                class="native-select"
                value={themeManager.theme}
                onchange={(e) => themeManager.setTheme((e.target as HTMLSelectElement).value as any)}
              >
                <option value="system">System Default</option>
                <option value="light">Light Mode</option>
                <option value="dark">Dark Mode</option>
              </select>
            </div>
          </div>
        </div>
      {/if}
    </section>
  </div>
</main>

<style>
  :global(:root) {
    --panel-bg: #ffffff;
    --border: #e2e8f0;
    --drop-bg: #f8fafc;
    --drop-hover-bg: #f1f5f9;
  }

  :global(html.dark) {
    --panel-bg: #1e293b;
    --border: #334155;
    --drop-bg: #0f172a;
    --drop-hover-bg: #1e293b;
  }

  .settings-container {
    display: flex;
    min-height: 100vh;
    background: var(--bg-color);
    box-sizing: border-box;
    padding: 32px;
    justify-content: center;
    align-items: center;
  }

  .drag-region {
    position: fixed;
    top: 0;
    right: 0;
    left: 0;
    z-index: 10;
    height: 28px;
    user-select: none;
    -webkit-app-region: drag;
  }

  .settings-body {
    display: flex;
    width: 720px;
    max-width: 100%;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel-bg);
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.08);
    overflow: hidden;
    height: 460px;
  }

  /* Sidebar styling */
  .settings-sidebar {
    width: 180px;
    border-right: 1px solid var(--border);
    background: var(--row-bg);
    padding: 16px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    box-sizing: border-box;
  }

  .back-link {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 6px;
    color: var(--text-muted);
    background: transparent;
    text-decoration: none;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .back-link:hover {
    color: var(--text-color);
    background: var(--close-btn-hover-bg);
  }

  .back-link svg {
    width: 13px;
    height: 13px;
  }

  .sidebar-divider {
    height: 1px;
    background: var(--border);
    margin: 8px 4px;
  }

  .sidebar-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-color);
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: all 0.15s ease;
    width: 100%;
  }

  .sidebar-item:hover {
    background: var(--close-btn-hover-bg);
  }

  .sidebar-item.is-active {
    background: #3b82f6;
    color: white;
  }

  .sidebar-item svg {
    width: 15px;
    height: 15px;
    opacity: 0.8;
  }

  .sidebar-item.is-active svg {
    opacity: 1;
  }

  /* Content pane styling */
  .settings-content-pane {
    flex: 1;
    padding: 24px 32px;
    box-sizing: border-box;
    overflow-y: auto;
  }

  .settings-section {
    margin-bottom: 28px;
  }

  .settings-section h3 {
    margin: 0 0 12px;
    font-size: 11px;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
    gap: 20px;
  }

  .setting-row:last-child {
    border-bottom: none;
  }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .setting-info label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-color);
  }

  .setting-desc {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.4;
  }

  /* Control elements */
  .native-select {
    font-family: inherit;
    font-size: 12px;
    padding: 5px 24px 5px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--row-bg);
    color: var(--text-color);
    cursor: pointer;
    outline: none;
    transition: all 0.15s ease;
  }

  .native-select:hover {
    border-color: var(--close-btn-hover-border);
  }

  /* Native-looking Switch (Checkbox styled) */
  .native-switch {
    appearance: none;
    position: relative;
    width: 36px;
    height: 20px;
    border-radius: 10px;
    background: #cbd5e1;
    cursor: pointer;
    transition: background 0.2s;
    outline: none;
  }

  :global(html.dark) .native-switch {
    background: #475569;
  }

  .native-switch:checked {
    background: #3b82f6;
  }

  .native-switch::before {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: white;
    transition: transform 0.2s;
    box-shadow: 0 1px 3px rgba(0,0,0,0.15);
  }

  .native-switch:checked::before {
    transform: translateX(16px);
  }
</style>
