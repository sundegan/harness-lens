<script lang="ts">
  import { onMount } from 'svelte';
  import type { Window as TauriWindow } from '@tauri-apps/api/window';
  import ControlBar from '$lib/menu/ControlBar.svelte';
  import { logWarn } from '$lib/logger';

  type TitlebarPlatform = 'macos' | 'windows' | 'linux';

  let appWindow: TauriWindow | null = null;
  let platform = $state<TitlebarPlatform>('macos');
  let isWindowExpanded = $state(false);

  const normalizePlatform = (value: string): TitlebarPlatform => {
    if (value === 'macos') return 'macos';
    if (value === 'windows') return 'windows';
    return 'linux';
  };

  const closeWindow = () => {
    void appWindow?.close();
  };

  const minimizeWindow = () => {
    void appWindow?.minimize();
  };

  const updateWindowState = async () => {
    if (!appWindow || platform === 'macos') return;
    isWindowExpanded = (await appWindow.isFullscreen()) || (await appWindow.isMaximized());
  };

  const toggleMaximizeWindow = async () => {
    if (!appWindow) return;
    await appWindow.toggleMaximize();
    await updateWindowState();
  };

  onMount(() => {
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const { invoke, isTauri } = await import('@tauri-apps/api/core');

      try {
        const value = await invoke<string>('desktop_platform');
        platform = normalizePlatform(value);
      } catch (err) {
        logWarn('Failed to detect desktop platform', err);
      }

      if (!isTauri()) return;

      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      appWindow = getCurrentWindow();

      if (platform === 'macos') return;

      await updateWindowState();

      const unlisten = await appWindow.onResized(() => {
        void updateWindowState();
      });
      unlisteners.push(unlisten);
    })();

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  });
</script>

<header class={`titlebar titlebar-${platform}`} data-tauri-drag-region>
  <div class="titlebar-left" data-tauri-drag-region></div>

  <div class="titlebar-center" data-tauri-drag-region></div>

  <div class="titlebar-right" data-tauri-drag-region>
    <ControlBar />

    {#if platform !== 'macos'}
      <div class={`window-controls desktop ${platform}`} aria-label="Window controls">
        <button class="caption minimize" type="button" aria-label="Minimize" onclick={minimizeWindow}></button>
        <button
          class={`caption maximize ${isWindowExpanded ? 'is-expanded' : ''}`}
          type="button"
          aria-label={isWindowExpanded ? 'Restore' : 'Maximize'}
          onclick={toggleMaximizeWindow}
        ></button>
        <button class="caption close" type="button" aria-label="Close" onclick={closeWindow}></button>
      </div>
    {/if}
  </div>
</header>

<style>
  .titlebar {
    --titlebar-height: 40px;
    display: grid;
    grid-template-columns: minmax(120px, 1fr) minmax(0, 1fr) minmax(120px, 1fr);
    height: var(--titlebar-height);
    flex: 0 0 var(--titlebar-height);
    box-sizing: border-box;
    user-select: none;
    color: var(--text-color);
    background: var(--titlebar-bg);
    border-bottom: 1px solid var(--titlebar-border);
    -webkit-app-region: drag;
  }

  .titlebar-macos {
    --titlebar-height: 38px;
    padding-left: 76px;
  }

  .titlebar-left,
  .titlebar-center,
  .titlebar-right {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .titlebar-left {
    justify-content: flex-start;
  }

  .titlebar-center {
    justify-content: center;
  }

  .titlebar-right {
    justify-content: flex-end;
  }

  .window-controls,
  .window-controls button {
    -webkit-app-region: no-drag;
  }

  .window-controls.desktop {
    display: inline-flex;
    align-items: stretch;
    height: 100%;
  }

  .caption {
    position: relative;
    flex: 0 0 auto;
    height: 100%;
    padding: 0;
    border: 0;
    appearance: none;
    color: var(--text-color);
    background: transparent;
    cursor: default;
  }

  .caption::before,
  .caption::after {
    position: absolute;
    top: 50%;
    left: 50%;
    display: block;
    content: "";
    background: currentColor;
    pointer-events: none;
    transform: translate(-50%, -50%);
  }

  .caption.minimize::before {
    width: 11px;
    height: 1px;
  }

  .caption.maximize::before {
    width: 10px;
    height: 10px;
    box-sizing: border-box;
    background: transparent;
    border: 1px solid currentColor;
  }

  .caption.maximize.is-expanded::before {
    width: 11px;
    height: 9px;
    border-radius: 1px;
    box-shadow: -3px 3px 0 -1px var(--titlebar-bg), -3px 3px 0 0 currentColor;
  }

  .caption.close::before,
  .caption.close::after {
    width: 12px;
    height: 1px;
  }

  .caption.close::before {
    transform: translate(-50%, -50%) rotate(45deg);
  }

  .caption.close::after {
    transform: translate(-50%, -50%) rotate(-45deg);
  }

  .window-controls.windows .caption {
    width: 46px;
    border-radius: 0;
  }

  .window-controls.windows .caption:hover {
    background: rgb(0 0 0 / 8%);
  }

  :global(html.dark) .window-controls.windows .caption:hover {
    background: rgb(255 255 255 / 10%);
  }

  .window-controls.windows .caption:active {
    background: rgb(0 0 0 / 14%);
  }

  .window-controls.windows .caption.close:hover {
    color: #fff;
    background: #e81123;
  }

  .window-controls.windows .caption.close:active {
    color: #fff;
    background: #f1707a;
  }

  .window-controls.linux {
    gap: 2px;
    padding-inline: 6px;
  }

  .window-controls.linux .caption {
    width: 36px;
    border-radius: 6px;
  }

  .window-controls.linux .caption:hover {
    background: rgb(127 127 127 / 16%);
  }

  .window-controls.linux .caption.close:hover {
    color: #fff;
    background: #c42b1c;
  }
</style>
