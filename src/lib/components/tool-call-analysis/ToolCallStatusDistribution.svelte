<script lang="ts">
import * as Tooltip from '$lib/components/ui/tooltip';
import { i18nManager } from '$lib/i18n.svelte';
import type { ToolCallComparison } from '$lib/tool-call-analysis';

let { rows }: { rows: ToolCallComparison[] } = $props();

const total = $derived(rows.reduce((sum, row) => sum + row.callCount, 0));

function percent(count: number): string {
  return total === 0 ? '0%' : `${((count / total) * 100).toFixed(1)}%`;
}

function duration(value: number | null): string {
  if (value === null) return i18nManager.t('tool_calls.status.duration_unavailable');
  if (value < 1000) return `${Math.round(value)} ms`;
  return `${(value / 1000).toFixed(1)} s`;
}

function color(status: string): string {
  if (status === 'completed') return 'bg-success';
  if (status === 'failed') return 'bg-destructive';
  if (status === 'declined') return 'bg-warning';
  if (status === 'cancelled') return 'bg-muted-foreground';
  if (status === 'in_progress' || status === 'awaiting_approval') return 'bg-primary';
  return 'bg-muted-foreground/50';
}
</script>

<section class="min-w-0" data-testid="tool-call-status-distribution">
  <div class="mb-2 flex items-center justify-between gap-3">
    <h3 class="text-sm font-semibold">{i18nManager.t('tool_calls.comparison.status')}</h3>
    <span class="text-xs tabular-nums text-muted-foreground">
      {i18nManager.t('tool_calls.status.total', { count: total })}
    </span>
  </div>

  {#if rows.length === 0}
    <div class="flex h-36 items-center justify-center rounded-xl border text-xs text-muted-foreground">
      {i18nManager.t('tool_calls.filter.no_options')}
    </div>
  {:else}
    <ul class="max-h-56 overflow-y-auto rounded-xl border" aria-label={i18nManager.t('tool_calls.status.aria', { count: total })}>
      {#each rows as row (row.key)}
        <li class="border-b px-3 py-2 last:border-b-0">
          <div class="flex min-w-0 items-center justify-between gap-3 text-xs">
            <div class="flex min-w-0 items-center gap-2">
              <span class={['size-2 shrink-0 rounded-full', color(row.key)]}></span>
              <span class="truncate font-medium">{i18nManager.t(`tool_calls.status.${row.key}`)}</span>
            </div>
            <div class="flex shrink-0 items-baseline gap-1.5 tabular-nums">
              <span class="font-semibold">{i18nManager.t('tool_calls.status.calls', { count: row.callCount })}</span>
              <span class="text-[11px] text-muted-foreground">{percent(row.callCount)}</span>
            </div>
          </div>
          <Tooltip.Provider delayDuration={0} skipDelayDuration={0} disableHoverableContent>
            <dl class="mt-2 grid grid-cols-3 gap-1.5 pl-4 text-[11px]">
              <Tooltip.Root delayDuration={0} disableHoverableContent>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <div {...props} class="min-w-0 cursor-help rounded-md bg-muted/50 px-2 py-1">
                      <dt class="truncate text-muted-foreground">{i18nManager.t('tool_calls.status.sessions')}</dt>
                      <dd class="mt-0.5 truncate font-medium tabular-nums text-foreground">{row.sessionCount}</dd>
                    </div>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content side="top" sideOffset={5}>{i18nManager.t('tool_calls.status.sessions_hint')}</Tooltip.Content>
              </Tooltip.Root>
              <Tooltip.Root delayDuration={0} disableHoverableContent>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <div {...props} class="min-w-0 cursor-help rounded-md bg-muted/50 px-2 py-1">
                      <dt class="truncate text-muted-foreground">{i18nManager.t('tool_calls.status.projects')}</dt>
                      <dd class="mt-0.5 truncate font-medium tabular-nums text-foreground">{row.projectCount}</dd>
                    </div>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content side="top" sideOffset={5}>{i18nManager.t('tool_calls.status.projects_hint')}</Tooltip.Content>
              </Tooltip.Root>
              <Tooltip.Root delayDuration={0} disableHoverableContent>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <div {...props} class="min-w-0 cursor-help rounded-md bg-muted/50 px-2 py-1">
                      <dt class="truncate text-muted-foreground">{i18nManager.t('tool_calls.status.average_duration')}</dt>
                      <dd class="mt-0.5 truncate font-medium tabular-nums text-foreground">{duration(row.averageDurationMs)}</dd>
                    </div>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content side="top" sideOffset={5}>{i18nManager.t('tool_calls.status.duration_hint')}</Tooltip.Content>
              </Tooltip.Root>
            </dl>
          </Tooltip.Provider>
        </li>
      {/each}
    </ul>
  {/if}
</section>
