<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import ControlBar from '$lib/menu/ControlBar.svelte';

  type TitlebarPlatform = 'macos' | 'windows' | 'linux';

  const appWindow = getCurrentWindow();

  let platform = $state<TitlebarPlatform>('macos');
  let isWindowExpanded = $state(false);

  const normalizePlatform = (value: string): TitlebarPlatform => {
    if (value === 'macos') return 'macos';
    if (value === 'windows') return 'windows';
    return 'linux';
  };

  const closeWindow = () => {
    void appWindow.close();
  };

  const minimizeWindow = () => {
    void appWindow.minimize();
  };

  const updateWindowState = async () => {
    isWindowExpanded = (await appWindow.isFullscreen()) || (await appWindow.isMaximized());
  };

  const toggleMaximizeWindow = async () => {
    await appWindow.toggleMaximize();
    await updateWindowState();
  };

  onMount(() => {
    const unlisteners: Array<() => void> = [];

    void invoke<string>('desktop_platform')
      .then((value) => {
        platform = normalizePlatform(value);
      })
      .catch((err) => {
        console.warn('Failed to detect desktop platform:', err);
      });

    void updateWindowState();

    void appWindow.onResized(() => {
      void updateWindowState();
    }).then((unlisten) => {
      unlisteners.push(unlisten);
    });

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  });
</script>

<header class={`titlebar titlebar-${platform}`} data-tauri-drag-region>
  <div class="titlebar-left" data-tauri-drag-region>
    {#if platform === 'macos'}
      <div class="window-controls macos" aria-label="Window controls">
        <button class="traffic close" type="button" aria-label="Close" onclick={closeWindow}></button>
        <button class="traffic minimize" type="button" aria-label="Minimize" onclick={minimizeWindow}></button>
        <button
          class={`traffic maximize ${isWindowExpanded ? 'is-expanded' : ''}`}
          type="button"
          aria-label={isWindowExpanded ? 'Restore' : 'Maximize'}
          onclick={toggleMaximizeWindow}
        ></button>
      </div>
    {/if}
  </div>

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
    user-select: none;
    color: var(--text-color);
    background: var(--titlebar-bg);
    border-bottom: 1px solid var(--titlebar-border);
    -webkit-app-region: drag;
  }

  .titlebar-macos {
    --titlebar-height: 38px;
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

  .window-controls.macos {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding-left: 12px;
    padding-right: 10px;
  }

  .traffic {
    position: relative;
    width: 12px;
    height: 12px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 14%);
  }

  .traffic.close {
    background: #ff5f57;
  }

  .traffic.minimize {
    background: #ffbd2e;
  }

  .traffic.maximize {
    background: #28c840;
  }

  .traffic::before,
  .traffic::after {
    position: absolute;
    display: block;
    content: "";
    background: rgb(0 0 0 / 58%);
    opacity: 0;
    pointer-events: none;
  }

  .window-controls.macos:hover .traffic::before,
  .window-controls.macos:hover .traffic::after {
    opacity: 1;
  }

  .traffic.close::before,
  .traffic.close::after {
    top: 50%;
    left: 50%;
    width: 6.5px;
    height: 1.3px;
    border-radius: 999px;
  }

  .traffic.close::before {
    transform: translate(-50%, -50%) rotate(45deg);
  }

  .traffic.close::after {
    transform: translate(-50%, -50%) rotate(-45deg);
  }

  .traffic.minimize::before {
    top: 50%;
    left: 50%;
    width: 7.5px;
    height: 1.5px;
    border-radius: 999px;
    transform: translate(-50%, -50%);
  }

  .traffic.maximize::before {
    inset: 2px;
    background: rgb(0 0 0 / 58%);
    -webkit-mask: url("data:image/svg+xml,%3Csvg viewBox='0 0 8 8' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M3.5 1.7H1.7v1.8M1.7 1.7l2 2M4.5 6.3h1.8V4.5M6.3 6.3l-2-2' fill='none' stroke='black' stroke-width='1.15' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E") center / contain no-repeat;
    mask: url("data:image/svg+xml,%3Csvg viewBox='0 0 8 8' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M3.5 1.7H1.7v1.8M1.7 1.7l2 2M4.5 6.3h1.8V4.5M6.3 6.3l-2-2' fill='none' stroke='black' stroke-width='1.15' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E") center / contain no-repeat;
  }

  .traffic.maximize::after {
    display: none;
  }

  .traffic.maximize.is-expanded::before {
    -webkit-mask-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 8 8' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M1.7 1.7l2 2M3.7 2.1v1.6H2.1M6.3 6.3l-2-2M4.3 5.9V4.3h1.6' fill='none' stroke='black' stroke-width='1.15' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
    mask-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 8 8' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M1.7 1.7l2 2M3.7 2.1v1.6H2.1M6.3 6.3l-2-2M4.3 5.9V4.3h1.6' fill='none' stroke='black' stroke-width='1.15' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
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
    background: transparent;
    border: 1px solid currentColor;
    box-sizing: border-box;
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
