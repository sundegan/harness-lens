<script lang="ts">
import CopyIcon from '@lucide/svelte/icons/copy';
import MinusIcon from '@lucide/svelte/icons/minus';
import PanelLeftCloseIcon from '@lucide/svelte/icons/panel-left-close';
import PanelLeftOpenIcon from '@lucide/svelte/icons/panel-left-open';
import SquareIcon from '@lucide/svelte/icons/square';
import XIcon from '@lucide/svelte/icons/x';
import type { Window as TauriWindow } from '@tauri-apps/api/window';
import { onMount } from 'svelte';
import { Button } from '$lib/components/ui/button';
import * as Tooltip from '$lib/components/ui/tooltip';
import { i18nManager } from '$lib/i18n.svelte';
import { logWarn } from '$lib/logger';
import { getMainSidebar } from '$lib/main-sidebar.svelte';
import ControlBar from '$lib/menu/ControlBar.svelte';
import { cn } from '$lib/utils';

type TitlebarPlatform = 'macos' | 'windows' | 'linux';
const RESIZE_STATE_UPDATE_DELAY_MS = 120;

let appWindow: TauriWindow | null = null;
let platform = $state<TitlebarPlatform>('macos');
let isWindowExpanded = $state(false);
let isWindowFullscreen = $state(false);
let resizeStateUpdateTimer: ReturnType<typeof setTimeout> | undefined;
const sidebar = getMainSidebar();

const normalizePlatform = (value: string): TitlebarPlatform => {
  if (value === 'macos') return 'macos';
  if (value === 'windows') return 'windows';
  return 'linux';
};

const updateWindowState = async () => {
  if (!appWindow) return;
  isWindowFullscreen = await appWindow.isFullscreen();
  if (platform === 'macos') return;
  isWindowExpanded = isWindowFullscreen || (await appWindow.isMaximized());
};

const toggleMaximizeWindow = async () => {
  if (!appWindow) return;
  await appWindow.toggleMaximize();
  await updateWindowState();
};

const scheduleWindowStateUpdate = () => {
  if (!appWindow) return;
  if (resizeStateUpdateTimer !== undefined) clearTimeout(resizeStateUpdateTimer);
  resizeStateUpdateTimer = setTimeout(() => {
    resizeStateUpdateTimer = undefined;
    void updateWindowState();
  }, RESIZE_STATE_UPDATE_DELAY_MS);
};

onMount(() => {
  let disposed = false;
  const unlisteners: Array<() => void> = [];
  const registerUnlistener = (unlisten: () => void) => {
    if (disposed) unlisten();
    else unlisteners.push(unlisten);
  };

  void (async () => {
    const { invoke, isTauri } = await import('@tauri-apps/api/core');

    if (!isTauri()) return;

    try {
      const detectedPlatform = normalizePlatform(await invoke<string>('desktop_platform'));
      if (disposed) return;
      platform = detectedPlatform;
    } catch (error) {
      logWarn('Failed to detect desktop platform', error);
    }
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    if (disposed) return;
    appWindow = getCurrentWindow();
    await updateWindowState();
    registerUnlistener(await appWindow.onResized(scheduleWindowStateUpdate));
  })().catch((error) => logWarn('Failed to initialize titlebar controls', error));

  return () => {
    disposed = true;
    if (resizeStateUpdateTimer !== undefined) clearTimeout(resizeStateUpdateTimer);
    for (const unlisten of unlisteners) unlisten();
    appWindow = null;
  };
});
</script>

{#snippet sidebarToggle()}
  <Tooltip.Provider delayDuration={150}>
    <Tooltip.Root ignoreNonKeyboardFocus>
      <Tooltip.Trigger onclick={() => sidebar.toggle()}>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon-sm"
            class="rounded-lg text-muted-foreground hover:bg-muted hover:text-foreground [-webkit-app-region:no-drag]"
            aria-label={i18nManager.t('main.nav.toggle')}
            aria-expanded={sidebar.open}
            data-testid="titlebar-sidebar-toggle"
          >
            {#if sidebar.open}
              <PanelLeftCloseIcon strokeWidth={1.8} aria-hidden="true" />
            {:else}
              <PanelLeftOpenIcon strokeWidth={1.8} aria-hidden="true" />
            {/if}
          </Button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="bottom" sideOffset={6}>{i18nManager.t('main.nav.toggle')}</Tooltip.Content>
    </Tooltip.Root>
  </Tooltip.Provider>
{/snippet}

<header
  class={cn(
    'grid h-10 shrink-0 grid-cols-[minmax(7.5rem,1fr)_minmax(0,1fr)_minmax(7.5rem,1fr)] bg-sidebar text-foreground select-none [-webkit-app-region:drag]',
    platform === 'macos' && 'h-[2.375rem]',
    platform === 'macos' && (isWindowFullscreen ? 'pl-2' : 'pl-[4.75rem]')
  )}
  data-tauri-drag-region
>
  <div
    class={cn(
      'flex items-center gap-1 px-1 [-webkit-app-region:drag]',
      platform === 'macos' && !isWindowFullscreen && '-ml-1'
    )}
    data-tauri-drag-region
  >
    {#if platform === 'macos'}
      {@render sidebarToggle()}
    {/if}
  </div>
  <div data-tauri-drag-region></div>
  <div class="flex min-w-0 items-center justify-end" data-tauri-drag-region>
    <ControlBar />

    {#if platform !== 'macos'}
      {@render sidebarToggle()}
      <div
        class={cn(
          'flex h-full items-stretch [-webkit-app-region:no-drag]',
          platform === 'linux' && 'gap-0.5 px-1.5'
        )}
        aria-label={i18nManager.t('control.window_controls')}
      >
        <Button
          variant="ghost"
          class={cn(
            'h-full rounded-none px-0',
            platform === 'windows' ? 'w-11.5' : 'w-9 rounded-md'
          )}
          aria-label={i18nManager.t('control.minimize')}
          onclick={() => void appWindow?.minimize()}
        >
          <MinusIcon aria-hidden="true" />
        </Button>
        <Button
          variant="ghost"
          class={cn(
            'h-full rounded-none px-0',
            platform === 'windows' ? 'w-11.5' : 'w-9 rounded-md'
          )}
          aria-label={
            isWindowExpanded
              ? i18nManager.t('control.restore')
              : i18nManager.t('control.maximize')
          }
          onclick={toggleMaximizeWindow}
        >
          {#if isWindowExpanded}
            <CopyIcon aria-hidden="true" />
          {:else}
            <SquareIcon aria-hidden="true" />
          {/if}
        </Button>
        <Button
          variant="ghost"
          class={cn(
            'h-full rounded-none px-0 hover:bg-destructive hover:text-destructive-foreground',
            platform === 'windows' ? 'w-11.5' : 'w-9 rounded-md'
          )}
          aria-label={i18nManager.t('control.close')}
          onclick={() => void appWindow?.close()}
        >
          <XIcon aria-hidden="true" />
        </Button>
      </div>
    {/if}
  </div>
</header>
