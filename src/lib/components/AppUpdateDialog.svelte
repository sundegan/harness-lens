<script lang="ts">
import AlertCircleIcon from '@lucide/svelte/icons/circle-alert';
import CheckCircle2Icon from '@lucide/svelte/icons/circle-check';
import DownloadIcon from '@lucide/svelte/icons/download';
import LoaderCircleIcon from '@lucide/svelte/icons/loader-circle';
import { Button } from '$lib/components/ui/button/index.js';
import * as Dialog from '$lib/components/ui/dialog/index.js';
import { i18nManager } from '$lib/i18n.svelte';
import { appUpdateManager } from '$lib/update.svelte';

const downloadedLabel = $derived(
  appUpdateManager.contentLength
    ? `${formatBytes(appUpdateManager.downloadedBytes)} / ${formatBytes(appUpdateManager.contentLength)}`
    : `${appUpdateManager.downloadProgress}%`
);

function formatBytes(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function closeDialog() {
  if (!appUpdateManager.isBusy) appUpdateManager.dialogOpen = false;
}
</script>

<Dialog.Root
  open={appUpdateManager.dialogOpen}
  onOpenChange={(open) => {
    if (open || !appUpdateManager.isBusy) appUpdateManager.dialogOpen = open;
  }}
>
  <Dialog.Content
    class="sm:max-w-md"
    data-testid="app-update-dialog"
    showCloseButton={!appUpdateManager.isBusy}
    closeLabel={i18nManager.t('about.close')}
  >
    <Dialog.Header>
      <div class="flex items-start gap-3 pr-8 text-left">
        {#if appUpdateManager.isBusy}
          <LoaderCircleIcon class="mt-0.5 size-5 shrink-0 animate-spin text-primary" aria-hidden="true" />
        {:else if appUpdateManager.status === 'ready'}
          <CheckCircle2Icon class="mt-0.5 size-5 shrink-0 text-emerald-600" aria-hidden="true" />
        {:else if appUpdateManager.status === 'error'}
          <AlertCircleIcon class="mt-0.5 size-5 shrink-0 text-destructive" aria-hidden="true" />
        {:else}
          <DownloadIcon class="mt-0.5 size-5 shrink-0 text-emerald-600" aria-hidden="true" />
        {/if}
        <div class="min-w-0">
          <Dialog.Title>
            {#if appUpdateManager.status === 'downloading'}
              {i18nManager.t('update.dialog.downloading_title')}
            {:else if appUpdateManager.status === 'installing'}
              {i18nManager.t('update.dialog.installing_title')}
            {:else if appUpdateManager.status === 'ready'}
              {i18nManager.t('update.dialog.ready_title')}
            {:else if appUpdateManager.status === 'error'}
              {i18nManager.t('update.dialog.error_title')}
            {:else}
              {i18nManager.t('update.dialog.title')}
            {/if}
          </Dialog.Title>
          <Dialog.Description class="mt-1.5">
            {#if appUpdateManager.status === 'downloading'}
              {i18nManager.t('update.dialog.downloading_description', { version: appUpdateManager.latestVersion })}
            {:else if appUpdateManager.status === 'installing'}
              {i18nManager.t('update.dialog.installing_description')}
            {:else if appUpdateManager.status === 'ready'}
              {i18nManager.t('update.dialog.ready_description', { version: appUpdateManager.latestVersion })}
            {:else if appUpdateManager.status === 'error'}
              {i18nManager.t('update.status.error', { error: appUpdateManager.error })}
            {:else}
              {i18nManager.t('update.dialog.description', { version: appUpdateManager.latestVersion })}
            {/if}
          </Dialog.Description>
        </div>
      </div>
    </Dialog.Header>

    {#if appUpdateManager.status === 'downloading'}
      <div class="space-y-2 rounded-2xl bg-muted/60 p-4">
        <div
          class="h-2 overflow-hidden rounded-full bg-background"
          role="progressbar"
          aria-label={downloadedLabel}
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={appUpdateManager.downloadProgress}
        >
          <div
            class="h-full rounded-full bg-primary transition-[width] duration-150"
            style={`width: ${appUpdateManager.downloadProgress}%`}
          ></div>
        </div>
        <div class="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span>{appUpdateManager.downloadProgress}%</span>
          <span>{downloadedLabel}</span>
        </div>
      </div>
    {:else if appUpdateManager.status === 'ready'}
      <div class="rounded-2xl border border-emerald-500/20 bg-emerald-500/5 px-4 py-3 text-sm text-muted-foreground">
        {i18nManager.t('update.dialog.restart_description')}
      </div>
    {/if}

    {#if appUpdateManager.status === 'ready'}
      <Dialog.Footer>
        <Button variant="outline" data-testid="update-later" onclick={closeDialog}>{i18nManager.t('update.action.later')}</Button>
        <Button class="update-dialog-action" data-testid="update-restart-now" onclick={() => void appUpdateManager.restartApp()}>
          {i18nManager.t('update.action.restart')}
        </Button>
      </Dialog.Footer>
    {:else if appUpdateManager.status === 'error'}
      <Dialog.Footer>
        <Button variant="outline" data-testid="update-later" onclick={closeDialog}>{i18nManager.t('update.action.later')}</Button>
        <Button data-testid="update-retry" onclick={() => void appUpdateManager.downloadUpdate()}>{i18nManager.t('update.action.retry')}</Button>
      </Dialog.Footer>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<style>
  :global(.update-dialog-action) {
    color: #16a34a;
    border-color: rgb(22 163 74 / 30%);
    background: rgb(22 163 74 / 8%);
    animation: update-button-pulse 2.4s infinite ease-in-out;
  }
  :global(.update-dialog-action:hover) {
    background: rgb(22 163 74 / 16%);
    border-color: rgb(22 163 74 / 45%);
  }
  :global(.dark .update-dialog-action) {
    color: #4ade80;
    border-color: rgb(74 222 128 / 25%);
    background: rgb(74 222 128 / 8%);
  }
  @keyframes update-button-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgb(22 163 74 / 35%); }
    50% { box-shadow: 0 0 0 5px rgb(22 163 74 / 0%); transform: scale(1.02); }
  }
</style>
