<script lang="ts">
import { onMount } from 'svelte';
import AppUpdateDialog from '$lib/components/AppUpdateDialog.svelte';
import SettingsDialog from '$lib/components/settings-dialog.svelte';
import { i18nManager } from '$lib/i18n.svelte';
import { installFrontendErrorLogging, logWarn } from '$lib/logger';
import AboutDialog from '$lib/menu/AboutDialog.svelte';
import TitleBar from '$lib/menu/TitleBar.svelte';
import { settingsDialogManager } from '$lib/settings-dialog.svelte';
import { syncStatusManager } from '$lib/sync-status.svelte';
import { themeManager } from '$lib/theme.svelte';
import { appUpdateManager } from '$lib/update.svelte';
import { cn } from '$lib/utils';
import '../app.css';

const syncTrayMenuLabels = async (labels: {
  showMain: string;
  settings: string;
  checkUpdates: string;
  quit: string;
}) => {
  const { isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) return;

  const { invoke } = await import('@tauri-apps/api/core');
  try {
    await invoke('set_tray_menu_labels', { labels });
  } catch (error) {
    logWarn('Failed to update tray menu labels', error);
  }
};

$effect(() => {
  void syncTrayMenuLabels({
    showMain: i18nManager.t('tray.show_main'),
    settings: i18nManager.t('tray.settings'),
    checkUpdates: i18nManager.t('tray.check_updates'),
    quit: i18nManager.t('tray.quit'),
  });
});

onMount(() => {
  let disposed = false;
  const unlisteners: Array<() => void> = [];
  const registerUnlistener = (unlisten: () => void) => {
    if (disposed) unlisten();
    else unlisteners.push(unlisten);
  };

  void themeManager.init();
  void i18nManager.init();
  void appUpdateManager.init();
  void syncStatusManager.init();
  void installFrontendErrorLogging()
    .then(registerUnlistener)
    .catch((error) => logWarn('Failed to install frontend error logging', error));

  void (async () => {
    if (import.meta.env.VITE_WDIO_TAURI === '1') {
      await import('@wdio/tauri-plugin');
    }

    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;

    const { listen } = await import('@tauri-apps/api/event');
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const appWindow = getCurrentWindow();

    const windowInactive = !(await appWindow.isFocused());
    if (disposed) return;
    isWindowInactive = windowInactive;
    const unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
      if (!disposed) isWindowInactive = !focused;
    });
    registerUnlistener(unlistenFocus);

    const unlistenUpdate = await listen('check-for-updates', () => {
      settingsDialogManager.show('updates');
      void appUpdateManager.checkForUpdates();
    });
    registerUnlistener(unlistenUpdate);

    const unlistenSettings = await listen('open-settings', () => {
      if (!disposed) settingsDialogManager.show();
    });
    registerUnlistener(unlistenSettings);

    const unlistenAnalytics = await listen('analytics-updated', () => {
      if (!disposed) void syncStatusManager.refresh();
    });
    registerUnlistener(unlistenAnalytics);
  })().catch((error) => logWarn('Failed to initialize desktop window listeners', error));

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
    const isZoomKey =
      e.key === '=' ||
      e.key === '-' ||
      e.key === '0' ||
      e.key === '+' ||
      e.code === 'Minus' ||
      e.code === 'Equal' ||
      e.code === 'Digit0' ||
      e.code === 'NumpadAdd' ||
      e.code === 'NumpadSubtract';
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
    disposed = true;
    document.removeEventListener('contextmenu', handleContextMenu);
    document.removeEventListener('wheel', handleWheel);
    document.removeEventListener('keydown', handleKeydown);
    document.removeEventListener('gesturestart', handleGesture);
    document.removeEventListener('gesturechange', handleGesture);
    for (const unlisten of unlisteners) unlisten();
  };
});

let { children } = $props();
let isWindowInactive = $state(false);
</script>

<div
  class={cn(
    'size-full bg-transparent',
    isWindowInactive &&
      '[--window-edge:color-mix(in_oklch,var(--foreground)_8%,transparent)] [--window-focus-ring:transparent] [--window-inner-highlight:color-mix(in_oklch,var(--foreground)_4%,transparent)]'
  )}
>
  <div
    class="flex size-full flex-col overflow-hidden rounded-[calc(var(--radius)*1.4)] border border-[var(--window-edge)] bg-background bg-clip-padding shadow-[0_0_0_1px_var(--window-focus-ring),inset_0_0_0_1px_var(--window-inner-highlight)]"
  >
    <TitleBar />
    <AboutDialog />
    <SettingsDialog bind:open={settingsDialogManager.open} />
    <AppUpdateDialog />
    <div class="min-h-0 flex-1 overflow-auto bg-background">
      {@render children()}
    </div>
  </div>
</div>
