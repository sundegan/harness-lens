<script lang="ts">
import Clock3Icon from '@lucide/svelte/icons/clock-3';
import type { Snippet } from 'svelte';
import SyncStatusIndicator from '$lib/components/SyncStatusIndicator.svelte';
import { i18nManager } from '$lib/i18n.svelte';
import { syncStatusManager } from '$lib/sync-status.svelte';

let {
  title,
  description,
  titleId,
  icon,
  content,
}: {
  title: string;
  description: string;
  titleId: string;
  icon: Snippet;
  content?: Snippet;
} = $props();

function formatTime(value: number): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(value));
}
</script>

<section class="border-b bg-muted/15 px-5 py-2.5 sm:px-6" aria-labelledby={titleId}>
  <div class="flex flex-col justify-between gap-2 sm:flex-row sm:items-center">
    <div class="flex min-w-0 items-center gap-2">
      <span class="flex size-6 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
        {@render icon()}
      </span>
      <div class="min-w-0">
        <h2 id={titleId} class="text-sm font-semibold">{title}</h2>
        <p class="mt-0.5 text-xs text-muted-foreground">{description}</p>
      </div>
    </div>

    <div class="flex shrink-0 flex-wrap items-center justify-end gap-x-3 gap-y-1">
      <SyncStatusIndicator />
      {#if syncStatusManager.lastUpdatedAtMs !== null}
        <p class="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <Clock3Icon class="size-3.5" aria-hidden="true" />
          {i18nManager.t('sync.status.data_updated', {
            time: formatTime(syncStatusManager.lastUpdatedAtMs),
          })}
        </p>
      {/if}
    </div>
  </div>

  {@render content?.()}
</section>
