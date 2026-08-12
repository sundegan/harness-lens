<script lang="ts">
import { onMount } from 'svelte';
import * as Dialog from '$lib/components/ui/dialog';
import { Separator } from '$lib/components/ui/separator';
import { i18nManager } from '$lib/i18n.svelte';

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
  </Dialog.Content>
</Dialog.Root>
