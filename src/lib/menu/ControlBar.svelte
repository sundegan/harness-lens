<script lang="ts">
  import { onMount } from 'svelte';
  import { appUpdateManager } from '$lib/update.svelte';
  import { themeManager } from '$lib/theme.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  let isAlwaysOnTop = $state(false);

  onMount(async () => {
    try {
      isAlwaysOnTop = await getCurrentWindow().isAlwaysOnTop();
    } catch (err) {
      console.error('Failed to get always on top status:', err);
    }
  });

  const toggleAlwaysOnTop = async () => {
    try {
      const win = getCurrentWindow();
      const newValue = !isAlwaysOnTop;
      await win.setAlwaysOnTop(newValue);
      isAlwaysOnTop = newValue;
    } catch (err) {
      console.error('Failed to toggle always on top status:', err);
    }
  };

  const toggleThemeMain = () => {
    if (themeManager.isDarkMode) {
      themeManager.setTheme('light');
    } else {
      themeManager.setTheme('dark');
    }
  };
</script>

<div class="control-bar">
  {#if appUpdateManager.hasUpdate}
    <a
      class="control-btn update-btn"
      href="/settings"
      aria-label={`Update available: ${appUpdateManager.latestVersion}`}
      title={`Update available: ${appUpdateManager.latestVersion}`}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 3v12" />
        <path d="m7 10 5 5 5-5" />
        <path d="M5 21h14" />
      </svg>
    </a>
  {/if}

  <!-- Theme Toggle (Only Light & Dark in main interface) -->
  <button
    class="control-btn"
    type="button"
    aria-label={themeManager.isDarkMode ? 'Switch to Light Theme' : 'Switch to Dark Theme'}
    onclick={toggleThemeMain}
    title={themeManager.isDarkMode ? 'Switch to Light Theme' : 'Switch to Dark Theme'}
  >
    {#if themeManager.isDarkMode}
      <!-- Sun icon (for switching to light) -->
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
      </svg>
    {:else}
      <!-- Moon icon (for switching to dark) -->
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />
      </svg>
    {/if}
  </button>

  <!-- Settings -->
  <a
    class="control-btn"
    href="/settings"
    aria-label="Settings"
    title="Settings"
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  </a>

  <!-- Pin Window -->
  <button
    class="control-btn"
    class:is-active={isAlwaysOnTop}
    type="button"
    onclick={toggleAlwaysOnTop}
    aria-label={isAlwaysOnTop ? 'Unpin Window' : 'Pin Window'}
    title={isAlwaysOnTop ? 'Unpin Window' : 'Pin Window'}
  >
    <svg viewBox="0 0 24 24" fill={isAlwaysOnTop ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
      <line x1="12" y1="17" x2="12" y2="22" />
      <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V17z" />
    </svg>
  </button>
</div>

<style>
  .control-bar {
    position: fixed;
    top: 14px;
    right: 14px;
    z-index: 100;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: none;
    box-shadow: none;
    -webkit-app-region: no-drag;
  }

  .control-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 6px;
    color: var(--text-color);
    opacity: 0.75;
    background: transparent;
    cursor: pointer;
    transition: background-color 0.12s ease, opacity 0.12s ease, color 0.12s ease;
  }

  .control-btn:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.05);
  }

  :global(html.dark) .control-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .control-btn:active {
    background: rgba(0, 0, 0, 0.1);
  }

  :global(html.dark) .control-btn:active {
    background: rgba(255, 255, 255, 0.15);
  }

  .control-btn.is-active {
    opacity: 1;
    color: #3b82f6;
    background: rgba(59, 130, 246, 0.12);
  }

  .control-btn.update-btn {
    opacity: 1;
    color: #16a34a;
    background: rgba(22, 163, 74, 0.12);
  }

  :global(html.dark) .control-btn.update-btn {
    color: #4ade80;
    background: rgba(74, 222, 128, 0.16);
  }

  :global(html.dark) .control-btn.is-active {
    color: #60a5fa;
    background: rgba(96, 165, 250, 0.18);
  }

  .control-btn svg {
    width: 15px;
    height: 15px;
  }
</style>
