<script lang="ts">
  import { onMount } from 'svelte';
  import { appUpdateManager } from '$lib/update.svelte';
  import { themeManager } from '$lib/theme.svelte';
  import { i18nManager } from '$lib/i18n.svelte';
  import { logWarn } from '$lib/logger';
  import Select from '$lib/components/Select.svelte';

  type SettingsTab = 'general' | 'appearance';

  let activeTab = $state<SettingsTab>('general');

  onMount(() => {
    const params = new URLSearchParams(window.location.search);
    const tab = params.get('tab');

    if (tab === 'appearance') {
      activeTab = tab;
    }
  });

  const setActiveTab = (tab: SettingsTab) => {
    activeTab = tab;
  };

  let showRestartModal = $state(false);

  $effect(() => {
    if (appUpdateManager.status === 'ready') {
      showRestartModal = true;
    }
  });

  const handleRestart = async () => {
    if (import.meta.env.DEV) {
      window.location.reload();
    } else {
      const { invoke } = await import('@tauri-apps/api/core');
      try {
        await invoke('restart_app');
      } catch (e) {
        logWarn('Failed to restart app', e);
        window.location.reload();
      }
    }
  };
</script>

<main class="settings-container">
  <div class="settings-body">
    <!-- Sidebar -->
    <aside class="settings-sidebar">
      <a href="/" class="back-link" title={i18nManager.t('nav.back')} aria-label={i18nManager.t('nav.back')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <line x1="19" y1="12" x2="5" y2="12" />
          <polyline points="12 19 5 12 12 5" />
        </svg>
      </a>

      <div class="sidebar-divider"></div>

      <button
        class="sidebar-item"
        class:is-active={activeTab === 'general'}
        onclick={() => setActiveTab('general')}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
        <span>{i18nManager.t('nav.general')}</span>
      </button>

      <button
        class="sidebar-item"
        class:is-active={activeTab === 'appearance'}
        onclick={() => setActiveTab('appearance')}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"/>
          <path d="M12 2a7 7 0 1 0 10 10"/>
          <path d="M12 6a6 6 0 1 0 6 6"/>
        </svg>
        <span>{i18nManager.t('nav.appearance')}</span>
      </button>

    </aside>

    <!-- Content Pane -->
    <section class="settings-content-pane">
      {#if activeTab === 'general'}
        <div class="settings-section">
          <h3>{i18nManager.t('settings.general.title')}</h3>

          <!-- Language Selection -->
          <div class="setting-row">
            <div class="setting-info">
              <label for="language-select-btn">{i18nManager.t('settings.language.title')}</label>
              <span class="setting-desc">{i18nManager.t('settings.language.desc')}</span>
            </div>
            <div class="setting-control">
              <Select
                bind:value={i18nManager.language}
                options={[
                  { value: 'system', label: i18nManager.t('settings.language.lang_system') },
                  { value: 'en', label: i18nManager.t('settings.language.lang_en') },
                  { value: 'zh', label: i18nManager.t('settings.language.lang_zh') },
                  { value: 'zh_tw', label: i18nManager.t('settings.language.lang_zh_tw') },
                  { value: 'ja', label: i18nManager.t('settings.language.lang_ja') },
                  { value: 'ko', label: i18nManager.t('settings.language.lang_ko') },
                  { value: 'es', label: i18nManager.t('settings.language.lang_es') },
                  { value: 'fr', label: i18nManager.t('settings.language.lang_fr') },
                  { value: 'de', label: i18nManager.t('settings.language.lang_de') }
                ]}
                id="language-select-btn"
                ariaLabel="Language options"
              />
            </div>
          </div>

          <div class="setting-row">
            <div class="setting-info">
              <label for="auto-check-updates">{i18nManager.t('settings.general.auto_check')}</label>
              <span class="setting-desc">{i18nManager.t('settings.general.auto_check_desc')}</span>
            </div>
            <div class="setting-control">
              <input
                type="checkbox"
                id="auto-check-updates"
                checked={appUpdateManager.autoCheckUpdates}
                onchange={(event) => {
                  appUpdateManager.setAutoCheckUpdates(
                    (event.currentTarget as HTMLInputElement).checked,
                  );
                }}
                class="native-switch"
              />
            </div>
          </div>

          <div class="setting-row" class:has-update={appUpdateManager.hasUpdate}>
            <div class="setting-info">
              <label for="check-update-btn">{i18nManager.t('settings.general.app_update')}</label>
              <span class="setting-desc">
                {#if appUpdateManager.status === 'checking'}
                  {i18nManager.t('update.status.checking')}
                {:else if appUpdateManager.status === 'installing'}
                  {i18nManager.t('update.status.installing')}
                {:else if appUpdateManager.status === 'ready'}
                  {i18nManager.t('update.status.ready')}
                {:else if appUpdateManager.status === 'error'}
                  {i18nManager.t('update.status.error', { error: appUpdateManager.error })}
                {:else if appUpdateManager.hasUpdate}
                  {i18nManager.t('update.status.available_prefix')}<span class="new-version-number">v{appUpdateManager.latestVersion}</span>{i18nManager.t('update.status.available_suffix', { currentVersion: appUpdateManager.currentVersion || '0.1.0' })}
                {:else}
                  {i18nManager.t('update.status.latest', { currentVersion: appUpdateManager.currentVersion || '0.1.0' })}
                {/if}
              </span>
            </div>
            <div class="setting-control">
              {#if appUpdateManager.status === 'ready'}
                <button
                  id="check-update-btn"
                  class="primary-action install-btn"
                  type="button"
                  onclick={handleRestart}
                >
                  {i18nManager.t('update.action.restart')}
                </button>
              {:else if appUpdateManager.hasUpdate}
                <button id="check-update-btn" class="primary-action install-btn" type="button" onclick={() => appUpdateManager.installUpdate()}>
                  {i18nManager.t('update.action.install')}
                </button>
              {:else}
                <button
                  id="check-update-btn"
                  class="secondary-action"
                  type="button"
                  disabled={appUpdateManager.isChecking}
                  onclick={() => appUpdateManager.checkForUpdates()}
                >
                  {appUpdateManager.status === 'checking' ? i18nManager.t('update.action.checking') : i18nManager.t('update.action.check_now')}
                </button>
              {/if}
            </div>
          </div>
        </div>
      {:else if activeTab === 'appearance'}
        <div class="settings-section">
          <h3>{i18nManager.t('settings.appearance.title')}</h3>

          <div class="setting-row">
            <div class="setting-info">
              <label for="theme-select">{i18nManager.t('settings.appearance.select')}</label>
              <span class="setting-desc">{i18nManager.t('settings.appearance.desc')}</span>
            </div>
            <div class="setting-control">
              <Select
                bind:value={themeManager.theme}
                options={[
                  { value: 'system', label: i18nManager.t('settings.appearance.theme_system') },
                  { value: 'light', label: i18nManager.t('settings.appearance.theme_light') },
                  { value: 'dark', label: i18nManager.t('settings.appearance.theme_dark') }
                ]}
                id="theme-select-btn"
                ariaLabel="Theme options"
              />
            </div>
          </div>
        </div>
      {/if}
    </section>
  </div>
</main>

{#if showRestartModal}
  <div class="modal-backdrop" role="presentation" onclick={() => showRestartModal = false}>
    <dialog
      class="modal-dialog"
      open
      aria-modal="true"
      onclick={(event) => event.stopPropagation()}
    >
      <div class="modal-header">
        <h4>{i18nManager.t('modal.restart.title')}</h4>
      </div>
      <div class="modal-body">
        <p>{i18nManager.t('modal.restart.desc')}</p>
      </div>
      <div class="modal-footer">
        <button class="secondary-action" type="button" onclick={() => showRestartModal = false}>
          {i18nManager.t('modal.restart.later')}
        </button>
        <button class="primary-action restart-confirm-btn" type="button" onclick={handleRestart}>
          {i18nManager.t('modal.restart.now')}
        </button>
      </div>
    </dialog>
  </div>
{/if}

<style>
  :global(:root) {
    --panel-bg: #ffffff;
    --border: #e2e8f0;
    --drop-hover-bg: #f1f5f9;
  }

  :global(html.dark) {
    --panel-bg: #1e293b;
    --border: #334155;
    --drop-hover-bg: #1e293b;
  }

  .settings-container {
    display: flex;
    min-height: 100%;
    background: var(--bg-color);
    box-sizing: border-box;
    padding: 32px;
    justify-content: center;
    align-items: center;
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
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
    background: transparent;
    text-decoration: none;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .back-link:hover {
    color: var(--text-color);
    background: var(--close-btn-hover-bg);
  }

  .back-link svg {
    width: 15px;
    height: 15px;
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


  /* Native-looking Switch (Checkbox styled) */
  .native-switch {
    appearance: none;
    position: relative;
    width: 40px;
    height: 22px;
    border-radius: 11px;
    background: #e2e8f0;
    border: 1px solid #cbd5e1;
    cursor: pointer;
    transition: background-color 0.2s, border-color 0.2s;
    outline: none;
    box-sizing: border-box;
  }

  :global(html.dark) .native-switch {
    background: #0f172a;
    border-color: #334155;
  }

  .native-switch:checked {
    background: #3b82f6;
    border-color: #3b82f6;
  }

  :global(html.dark) .native-switch:checked {
    background: #60a5fa;
    border-color: #60a5fa;
  }

  .native-switch::before {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #ffffff;
    transition: transform 0.2s, background-color 0.2s;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }

  :global(html.dark) .native-switch::before {
    background: #94a3b8;
  }

  :global(html.dark) .native-switch:checked::before {
    background: #ffffff;
  }

  .native-switch:checked::before {
    transform: translateX(18px);
  }



  .primary-action,
  .secondary-action {
    height: 30px;
    padding: 0 12px;
    border-radius: 7px;
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }

  .primary-action {
    border: 1px solid #2563eb;
    color: white;
    background: #2563eb;
  }

  .secondary-action {
    border: 1px solid var(--border);
    color: var(--text-color);
    background: var(--panel-bg);
  }

  .secondary-action:disabled {
    cursor: default;
    opacity: 0.58;
  }

  /* Beautiful glowing update row highlight and install button pulse */
  :global(:root) {
    --update-glow: rgba(22, 163, 74, 0.35);
    --update-glow-transparent: rgba(22, 163, 74, 0);
  }

  :global(html.dark) {
    --update-glow: rgba(74, 222, 128, 0.28);
    --update-glow-transparent: rgba(74, 222, 128, 0);
  }

  .setting-row.has-update {
    background: rgba(22, 163, 74, 0.05);
    border: 1px solid rgba(22, 163, 74, 0.18);
    border-radius: 8px;
    padding: 12px 14px;
    margin: 8px -14px;
    transition: all 0.25s ease;
  }

  :global(html.dark) .setting-row.has-update {
    background: rgba(74, 222, 128, 0.06);
    border-color: rgba(74, 222, 128, 0.22);
  }

  .new-version-number {
    color: #16a34a;
    font-weight: 600;
  }

  :global(html.dark) .new-version-number {
    color: #4ade80;
  }

  .primary-action.install-btn {
    color: #16a34a;
    border-color: rgba(22, 163, 74, 0.3);
    background: rgba(22, 163, 74, 0.08);
    box-shadow: 0 0 0 0 var(--update-glow);
    animation: button-pulse 2.4s infinite ease-in-out;
    transition: background-color 0.12s ease, border-color 0.12s ease;
  }

  .primary-action.install-btn:hover {
    background: rgba(22, 163, 74, 0.16);
    border-color: rgba(22, 163, 74, 0.45);
  }

  :global(html.dark) .primary-action.install-btn {
    color: #4ade80;
    border-color: rgba(74, 222, 128, 0.25);
    background: rgba(74, 222, 128, 0.08);
  }

  :global(html.dark) .primary-action.install-btn:hover {
    background: rgba(74, 222, 128, 0.16);
    border-color: rgba(74, 222, 128, 0.38);
  }

  @keyframes button-pulse {
    0% {
      transform: scale(1);
      box-shadow: 0 0 0 0 var(--update-glow);
    }
    50% {
      transform: scale(1.02);
      box-shadow: 0 0 0 5px var(--update-glow-transparent);
    }
    100% {
      transform: scale(1);
      box-shadow: 0 0 0 0 var(--update-glow-transparent);
    }
  }

  /* Custom Svelte Modal styling */
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    background: var(--backdrop-bg);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal-dialog {
    width: 320px;
    background: var(--dialog-bg);
    border: 1px solid var(--dialog-border);
    border-radius: 12px;
    box-shadow: var(--dialog-shadow);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    animation: scale-up 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  @keyframes scale-up {
    0% {
      transform: scale(0.92);
      opacity: 0;
    }
    100% {
      transform: scale(1);
      opacity: 1;
    }
  }

  .modal-header h4 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--text-color);
  }

  .modal-body p {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }

  .restart-confirm-btn {
    color: #16a34a;
    border-color: rgba(22, 163, 74, 0.3);
    background: rgba(22, 163, 74, 0.08);
    box-shadow: 0 0 0 0 var(--update-glow);
    animation: button-pulse 2.4s infinite ease-in-out;
    transition: background-color 0.12s ease, border-color 0.12s ease;
  }

  .restart-confirm-btn:hover {
    background: rgba(22, 163, 74, 0.16);
    border-color: rgba(22, 163, 74, 0.45);
  }

  :global(html.dark) .restart-confirm-btn {
    color: #4ade80;
    border-color: rgba(74, 222, 128, 0.25);
    background: rgba(74, 222, 128, 0.08);
  }

  :global(html.dark) .restart-confirm-btn:hover {
    background: rgba(74, 222, 128, 0.16);
    border-color: rgba(74, 222, 128, 0.38);
  }
</style>
