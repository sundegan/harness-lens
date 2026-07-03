<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from '$lib/menu/TitleBar.svelte';
  import { appUpdateManager } from '$lib/update.svelte';
  import { themeManager } from '$lib/theme.svelte';

  // Reactively track theme manager theme state to ensure native integration updates
  $effect(() => {
    // Explicitly read the theme state to establish reactive tracking in Svelte 5
    const _ = themeManager.theme;
    void themeManager.updateTheme();
  });

  onMount(() => {
    const appWindow = getCurrentWindow();
    const unlisteners: Array<() => void> = [];

    void appUpdateManager.init();
    void appWindow.isFocused().then((focused) => {
      isWindowInactive = !focused;
    });
    void appWindow.onFocusChanged(({ payload: focused }) => {
      isWindowInactive = !focused;
    }).then((unlisten) => {
      unlisteners.push(unlisten);
    });

    const unlistenUpdate = listen('check-for-updates', () => {
      void goto('/settings');
      void appUpdateManager.checkForUpdates();
    });

    // Prevent default browser context menu globally to eliminate web feeling
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };

    // Prevent wheel zoom (Ctrl + Mouse Wheel / Pinch gesture on trackpad)
    const handleWheel = (e: WheelEvent) => {
      if (e.ctrlKey) {
        e.preventDefault();
      }
    };

    // Prevent keyboard zoom shortcuts: Cmd/Ctrl + = / - / 0
    const handleKeydown = (e: KeyboardEvent) => {
      const isZoomKey = e.key === '=' || e.key === '-' || e.key === '0' || e.key === '+' ||
                        e.code === 'Minus' || e.code === 'Equal' || e.code === 'Digit0' ||
                        e.code === 'NumpadAdd' || e.code === 'NumpadSubtract';
      if ((e.ctrlKey || e.metaKey) && isZoomKey) {
        e.preventDefault();
      }
    };

    // Prevent Safari/WebKit gesture zoom (pinch gesture on macOS/iOS)
    const handleGesture = (e: Event) => {
      e.preventDefault();
    };

    document.addEventListener('contextmenu', handleContextMenu);
    document.addEventListener('wheel', handleWheel, { passive: false });
    document.addEventListener('keydown', handleKeydown);
    document.addEventListener('gesturestart', handleGesture);
    document.addEventListener('gesturechange', handleGesture);

    return () => {
      document.removeEventListener('contextmenu', handleContextMenu);
      document.removeEventListener('wheel', handleWheel);
      document.removeEventListener('keydown', handleKeydown);
      document.removeEventListener('gesturestart', handleGesture);
      document.removeEventListener('gesturechange', handleGesture);
      void unlistenUpdate.then((dispose) => dispose());
      for (const unlisten of unlisteners) unlisten();
    };
  });

  let { children } = $props();
  let isWindowInactive = $state(false);
</script>

<div class={`macos-window-frame ${isWindowInactive ? 'is-inactive' : ''}`}>
  <div class="app-window">
    <TitleBar />
    <div class="content-region">
      {@render children()}
    </div>
  </div>
</div>

<style>
  :global(:root) {
    --bg-color: #f7f8f8;
    --text-color: #1f2933;
    --text-muted: #667085;
    --dialog-bg: #ffffff;
    --dialog-border: rgb(15 23 42 / 12%);
    --dialog-shadow: 0 24px 80px rgb(15 23 42 / 28%);
    --close-btn-color: #98a2b3;
    --close-btn-hover-bg: #f3f4f6;
    --close-btn-hover-border: #d0d5dd;
    --row-bg: #f9fafb;
    --row-border: #e4e7ec;
    --github-hover-bg: #eff6ff;
    --github-hover-border: #2f80ed;
    --github-hover-text: #2f80ed;
    --footer-color: #98a2b3;
    --backdrop-bg: rgb(8 11 16 / 42%);
    --titlebar-bg: #ffffff;
    --titlebar-border: rgb(15 23 42 / 10%);
    --window-edge: rgb(15 23 42 / 18%);
    --window-inner-highlight: rgb(255 255 255 / 70%);
    --window-focus-ring: rgb(15 23 42 / 8%);
  }

  :global(html.dark) {
    --bg-color: #0f172a;
    --text-color: #f8fafc;
    --text-muted: #94a3b8;
    --dialog-bg: #1e293b;
    --dialog-border: rgb(255 255 255 / 12%);
    --dialog-shadow: 0 24px 80px rgb(0 0 0 / 60%);
    --close-btn-color: #64748b;
    --close-btn-hover-bg: #334155;
    --close-btn-hover-border: #475569;
    --row-bg: #1e293b;
    --row-border: #334155;
    --github-hover-bg: #1e293b;
    --github-hover-border: #3b82f6;
    --github-hover-text: #60a5fa;
    --footer-color: #64748b;
    --backdrop-bg: rgb(0 0 0 / 60%);
    --titlebar-bg: #111827;
    --titlebar-border: rgb(255 255 255 / 10%);
    --window-edge: rgb(255 255 255 / 18%);
    --window-inner-highlight: rgb(255 255 255 / 8%);
    --window-focus-ring: rgb(0 0 0 / 40%);
  }

  :global(html),
  :global(body),
  :global(#app-root) {
    width: 100%;
    height: 100%;
    margin: 0;
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: var(--text-color);
    background: transparent;
    transition: background-color 0.2s ease, color 0.2s ease;
    overscroll-behavior: none; /* Disables elastic overscroll bounce globally */
    user-select: none; /* Prevents text selection on UI elements globally */
    -webkit-user-select: none;
    overflow: hidden;
  }

  .macos-window-frame {
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    background: transparent;
  }

  .app-window {
    display: flex;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--window-edge);
    border-radius: 10px;
    background: var(--bg-color);
    background-clip: padding-box;
    box-shadow: 0 0 0 1px var(--window-focus-ring), inset 0 0 0 1px var(--window-inner-highlight);
  }

  .macos-window-frame.is-inactive {
    --window-edge: rgb(15 23 42 / 9%);
    --window-inner-highlight: rgb(255 255 255 / 28%);
    --window-focus-ring: transparent;
    --titlebar-bg: #f3f4f6;
    --titlebar-border: rgb(15 23 42 / 7%);
  }

  :global(html.dark) .macos-window-frame.is-inactive {
    --window-edge: rgb(255 255 255 / 9%);
    --window-inner-highlight: rgb(255 255 255 / 5%);
    --titlebar-bg: #0f172a;
    --titlebar-border: rgb(255 255 255 / 7%);
  }

  .macos-window-frame.is-inactive :global(.traffic) {
    opacity: 0.55;
  }

  .content-region {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    background: var(--bg-color);
  }

  /* Re-enable text selection for input fields, textareas, editable areas, and code/log containers */
  :global(input),
  :global(textarea),
  :global([contenteditable="true"]),
  :global(pre),
  :global(code),
  :global(.selectable-text) {
    -webkit-user-select: text;
    user-select: text;
  }

  /* Prevent image and drag actions that show browser selection outlines */
  :global(img),
  :global(a) {
    -webkit-user-drag: none;
  }
</style>
