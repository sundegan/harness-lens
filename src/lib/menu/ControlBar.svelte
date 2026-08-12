<script lang="ts">
import MoonIcon from '@lucide/svelte/icons/moon';
import PinIcon from '@lucide/svelte/icons/pin';
import SunIcon from '@lucide/svelte/icons/sun';
import { onMount } from 'svelte';
import { Button } from '$lib/components/ui/button';
import * as Tooltip from '$lib/components/ui/tooltip';
import { i18nManager } from '$lib/i18n.svelte';
import { logError } from '$lib/logger';
import { themeManager } from '$lib/theme.svelte';
import { appUpdateManager } from '$lib/update.svelte';

let isAlwaysOnTop = $state(false);

const themeLabel = $derived(
  themeManager.isDarkMode
    ? i18nManager.t('control.theme_light')
    : i18nManager.t('control.theme_dark')
);
const pinLabel = $derived(
  isAlwaysOnTop ? i18nManager.t('control.unpin') : i18nManager.t('control.pin')
);

onMount(() => {
  let disposed = false;

  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const alwaysOnTop = await getCurrentWindow().isAlwaysOnTop();
    if (!disposed) isAlwaysOnTop = alwaysOnTop;
  })().catch((error) => logError('Failed to get always on top status', error));

  return () => {
    disposed = true;
  };
});

const toggleAlwaysOnTop = async () => {
  try {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const appWindow = getCurrentWindow();
    const nextValue = !isAlwaysOnTop;
    await appWindow.setAlwaysOnTop(nextValue);
    isAlwaysOnTop = nextValue;
  } catch (error) {
    logError('Failed to toggle always on top status', error);
  }
};

const openSettings = () => {
  window.location.href = '/settings';
};

const toggleTheme = () => {
  themeManager.setTheme(themeManager.isDarkMode ? 'light' : 'dark');
};
</script>

<Tooltip.Provider delayDuration={150}>
  <div class="flex items-center gap-1 px-2 [-webkit-app-region:no-drag]">
    {#if appUpdateManager.hasUpdate}
      <Button
        variant="ghost"
        size="icon-sm"
        data-testid="titlebar-update-button"
        aria-label={i18nManager.t('control.update_available', { version: appUpdateManager.latestVersion })}
        title={i18nManager.t('control.update_available', { version: appUpdateManager.latestVersion })}
        onclick={openSettings}
      >
        ↓
      </Button>
    {/if}
    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="ghost" size="icon-sm" aria-label={themeLabel} onclick={toggleTheme}>
            {#if themeManager.isDarkMode}
              <SunIcon aria-hidden="true" />
            {:else}
              <MoonIcon aria-hidden="true" />
            {/if}
          </Button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>{themeLabel}</Tooltip.Content>
    </Tooltip.Root>

    <Button
      variant="ghost"
      size="icon-sm"
      data-testid="titlebar-settings-button"
      aria-label={i18nManager.t('control.settings')}
      title={i18nManager.t('control.settings')}
      onclick={openSettings}
    >
      <span aria-hidden="true">⚙</span>
    </Button>

    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <Button
            {...props}
            variant={isAlwaysOnTop ? 'secondary' : 'ghost'}
            size="icon-sm"
            data-testid="titlebar-pin-button"
            aria-label={pinLabel}
            onclick={toggleAlwaysOnTop}
          >
            <PinIcon class={isAlwaysOnTop ? 'fill-current text-primary' : undefined} aria-hidden="true" />
          </Button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>{pinLabel}</Tooltip.Content>
    </Tooltip.Root>
  </div>
</Tooltip.Provider>
