<script lang="ts">
  import AboutDialog from '$lib/menu/AboutDialog.svelte';
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
</script>

<main>
  {#if dragRegionEnabled}
    <div class="drag-region" aria-hidden="true" data-tauri-drag-region></div>
  {/if}

  <h1>Codex Timeline</h1>
  <p>Local-first desktop tool for inspecting Codex agent runs.</p>
  <AboutDialog />

  <button
    class="theme-toggle"
    type="button"
    aria-label="Toggle theme"
    onclick={() => themeManager.toggleTheme()}
    title={`Theme: ${themeManager.theme}`}
  >
    {#if themeManager.theme === 'system'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="2" y="3" width="20" height="14" rx="2" ry="2" />
        <line x1="8" y1="21" x2="16" y2="21" />
        <line x1="12" y1="17" x2="12" y2="21" />
      </svg>
    {:else if themeManager.theme === 'light'}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
      </svg>
    {:else}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />
      </svg>
    {/if}
  </button>
</main>

<style>
  main {
    display: flex;
    box-sizing: border-box;
    min-height: 100vh;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px 32px 96px;
    text-align: center;
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

  .theme-toggle {
    position: fixed;
    top: 14px;
    right: 14px;
    z-index: 100;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: 1px solid var(--row-border);
    border-radius: 10px;
    color: var(--close-btn-color);
    background: var(--row-bg);
    cursor: pointer;
    transition: all 0.2s ease;
    -webkit-app-region: no-drag;
  }

  .theme-toggle:hover {
    border-color: var(--close-btn-hover-border);
    color: var(--text-color);
    background: var(--close-btn-hover-bg);
  }

  .theme-toggle svg {
    width: 18px;
    height: 18px;
  }

  h1 {
    margin: 0 0 4px;
    font-size: 28px;
    line-height: 1.2;
  }

  p {
    margin: 0;
    color: var(--text-muted);
  }
</style>
