<script lang="ts">
import { i18nManager } from '$lib/i18n.svelte';
import { type SyncStatus, syncStatusManager } from '$lib/sync-status.svelte';

type SyncIndicator =
  | { kind: 'initializing' }
  | { kind: 'initial'; progress: number }
  | { kind: 'incremental' }
  | { kind: 'error' }
  | { kind: 'watching' };

const indicatorBaseClass =
  'inline-flex items-center gap-1.5 whitespace-nowrap rounded border px-1.5 py-0.5 text-[10px] font-medium tracking-wide';
const indicatorClass = {
  initializing: 'border-primary/25 bg-primary/10 text-primary',
  initial: 'border-primary/25 bg-primary/10 text-primary',
  incremental: 'border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  error: 'border-destructive/25 bg-destructive/10 text-destructive',
  watching: 'border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
} as const;
const dotClass = {
  initializing: 'bg-primary',
  initial: 'bg-primary',
  incremental: 'bg-amber-500',
  error: 'bg-destructive',
  watching: 'bg-emerald-500',
} as const;

function scanProgress(status: SyncStatus): number {
  if (status.estimatedTotalLines && status.estimatedTotalLines > 0) {
    return Math.min(100, (status.processedLines / status.estimatedTotalLines) * 100);
  }
  if (status.totalFiles > 0) {
    return Math.min(100, (status.processedFiles / status.totalFiles) * 100);
  }
  return 0;
}

const syncIndicator = $derived.by<SyncIndicator | null>(() => {
  if (syncStatusManager.databaseStatus === 'pending') return { kind: 'initializing' };
  if (syncStatusManager.databaseStatus === 'failed') return { kind: 'error' };
  const initialScan = syncStatusManager.activeStatuses.find(
    (status) => status.phase === 'initial_scan'
  );
  if (initialScan) return { kind: 'initial', progress: scanProgress(initialScan) };
  if (syncStatusManager.activeStatuses.length > 0) return { kind: 'incremental' };
  if (syncStatusManager.failedStatuses.length > 0) return { kind: 'error' };
  return syncStatusManager.watchingStatuses.length > 0 ? { kind: 'watching' } : null;
});
</script>

{#if syncIndicator}
  {@const labelKey =
    syncIndicator.kind === 'initializing'
      ? 'sync.status.initializing'
      : syncIndicator.kind === 'initial'
      ? 'sync.status.initial_active'
      : syncIndicator.kind === 'incremental'
        ? 'sync.status.incremental_active'
        : syncIndicator.kind === 'error'
          ? 'sync.status.error'
          : 'sync.status.watching'}
  {@const label = i18nManager.t(labelKey)}
  <span
    class={`${indicatorBaseClass} ${indicatorClass[syncIndicator.kind]}`}
    role="status"
    aria-live={syncIndicator.kind === 'watching' ? undefined : 'polite'}
    aria-label={label + (syncIndicator.kind === 'initial' ? ' ' + Math.round(syncIndicator.progress) + '%' : '')}
    data-testid="sync-status-indicator"
  >
    <span class={`size-1.5 rounded-full ${dotClass[syncIndicator.kind]}`} aria-hidden="true"></span>
    <span>{label}</span>
    {#if syncIndicator.kind === 'initial'}
      <span
        class="h-1 w-14 overflow-hidden rounded-full bg-primary/20"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={syncIndicator.progress}
      >
        <span
          class="block h-full rounded-full bg-primary transition-[width] duration-300"
          style:width={syncIndicator.progress + '%'}
        ></span>
      </span>
      <span class="tabular-nums">{Math.round(syncIndicator.progress)}%</span>
    {/if}
  </span>
{/if}
