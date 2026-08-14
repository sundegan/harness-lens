<script lang="ts">
import { onMount } from 'svelte';
import { Button } from '$lib/components/ui/button/index.js';
import * as Dialog from '$lib/components/ui/dialog';
import { Separator } from '$lib/components/ui/separator';
import { i18nManager } from '$lib/i18n.svelte';
import { appUpdateManager } from '$lib/update.svelte';

let appVersion = $state('');
let isOpen = $state(false);

onMount(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;

  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;

    const [{ getVersion }, { listen }] = await Promise.all([
      import('@tauri-apps/api/app'),
      import('@tauri-apps/api/event'),
    ]);
    appVersion = await getVersion();

    const stopListening = await listen('show-about', () => {
      isOpen = true;
    });
    if (disposed) {
      stopListening();
    } else {
      unlisten = stopListening;
    }
  })().catch(() => {
    appVersion = '';
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
});
</script>

<Dialog.Root bind:open={isOpen}>
  <Dialog.Content class="sm:max-w-md" closeLabel={i18nManager.t('about.close')}>
    <Dialog.Header>
      <div class="flex items-start gap-4 pr-8 text-left">
        <img class="size-16 shrink-0" src="/app-icon.png" alt="" />
        <div class="min-w-0 pt-1">
          <Dialog.Title class="text-xl">{i18nManager.t('about.title')}</Dialog.Title>
          <Dialog.Description class="mt-1.5">{i18nManager.t('about.desc')}</Dialog.Description>
        </div>
      </div>
    </Dialog.Header>

    <Separator />

    <dl class="flex items-center justify-between gap-4 rounded-2xl bg-muted/60 px-4 py-3">
      <dt class="text-sm font-medium text-muted-foreground">{i18nManager.t('about.version')}</dt>
      <dd class="text-sm font-semibold">{appVersion || i18nManager.t('about.unknown')}</dd>
    </dl>

    <div class="mt-4 flex items-center justify-between gap-3">
      <p class="text-xs text-muted-foreground">
        {#if appUpdateManager.status === 'checking'}
          {i18nManager.t('update.status.checking')}
        {:else if appUpdateManager.status === 'downloading'}
          {i18nManager.t('update.status.downloading')}
        {:else if appUpdateManager.status === 'installing'}
          {i18nManager.t('update.status.installing')}
        {:else if appUpdateManager.status === 'ready'}
          {i18nManager.t('update.status.ready')}
        {:else if appUpdateManager.hasUpdate}
          {i18nManager.t('update.status.available', { version: appUpdateManager.latestVersion })}
        {:else if appUpdateManager.status === 'latest'}
          {i18nManager.t('update.status.latest', { currentVersion: appUpdateManager.currentVersion || appVersion })}
        {:else}
          {i18nManager.t('update.status.idle')}
        {/if}
      </p>
      {#if appUpdateManager.status === 'ready'}
        <Button size="sm" onclick={() => appUpdateManager.openUpdateDialog()}>{i18nManager.t('update.action.restart')}</Button>
      {:else if appUpdateManager.status === 'available'}
        <Button size="sm" onclick={() => appUpdateManager.openUpdateDialog()}>{i18nManager.t('update.action.download')}</Button>
      {:else}
        <Button variant="outline" size="sm" disabled={appUpdateManager.isBusy} onclick={() => void appUpdateManager.checkForUpdates()}>
          {appUpdateManager.status === 'checking' ? i18nManager.t('update.action.checking') : i18nManager.t('update.action.check_now')}
        </Button>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
