<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { listen } from '@tauri-apps/api/event';
  import { appUpdateManager } from '$lib/update.svelte';
  import { themeManager } from '$lib/theme.svelte';

  // Reactively track theme manager theme state to ensure native integration updates
  $effect(() => {
    // Explicitly read the theme state to establish reactive tracking in Svelte 5
    const _ = themeManager.theme;
    void themeManager.updateTheme();
  });

  onMount(() => {
    void appUpdateManager.init();

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
    };
  });

  let { children } = $props();
</script>

{@render children()}

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
  }

  :global(html),
  :global(body) {
    margin: 0;
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: var(--text-color);
    background: var(--bg-color);
    transition: background-color 0.2s ease, color 0.2s ease;
    overscroll-behavior: none; /* Disables elastic overscroll bounce globally */
    user-select: none; /* Prevents text selection on UI elements globally */
    -webkit-user-select: none;
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
