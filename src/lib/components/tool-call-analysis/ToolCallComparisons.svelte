<script lang="ts">
import DataPagination from '$lib/components/data-pagination/DataPagination.svelte';
import { Badge } from '$lib/components/ui/badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '$lib/components/ui/table';
import * as Tooltip from '$lib/components/ui/tooltip';
import { i18nManager } from '$lib/i18n.svelte';
import type { ToolCallComparison } from '$lib/tool-call-analysis';

let {
  title,
  rows,
  emptyText = i18nManager.t('tool_calls.filter.no_options'),
  dimensionLabel = i18nManager.t('tool_calls.comparison.dimension'),
  showScopeColumns = false,
  showRepeatColumn = false,
  showHeaderHelp = false,
  testId = undefined,
  paginated = false,
  idPrefix = 'tool-call-comparison',
}: {
  title: string;
  rows: ToolCallComparison[];
  emptyText?: string;
  dimensionLabel?: string;
  showScopeColumns?: boolean;
  showRepeatColumn?: boolean;
  showHeaderHelp?: boolean;
  testId?: string;
  paginated?: boolean;
  idPrefix?: string;
} = $props();

let page = $state(1);
let pageSize = $state(10);
const pageCount = $derived(Math.max(1, Math.ceil(rows.length / pageSize)));
const currentPage = $derived(Math.min(page, pageCount));
const visibleRows = $derived(
  paginated ? rows.slice((currentPage - 1) * pageSize, currentPage * pageSize) : rows
);

function percent(value: number | null): string {
  return value === null ? '—' : `${(value * 100).toFixed(1)}%`;
}

function duration(value: number | null): string {
  if (value === null) return '—';
  if (value < 1000) return `${Math.round(value)} ms`;
  return `${(value / 1000).toFixed(1)} s`;
}
</script>

{#snippet header(label: string, help: string, align: 'left' | 'right' = 'right')}
  {#if showHeaderHelp}
    <Tooltip.Root delayDuration={0} disableHoverableContent>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span
            {...props}
            class={[
              'flex h-9 w-full cursor-help items-center px-3 text-xs font-medium',
              align === 'right' ? 'justify-end text-right' : 'justify-start text-left',
            ]}
          >{label}</span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content class="max-w-72 leading-relaxed" side="top" sideOffset={5}>{help}</Tooltip.Content>
    </Tooltip.Root>
  {:else}
    {label}
  {/if}
{/snippet}

<section class="min-w-0" data-testid={testId}>
  <div class="mb-2 flex items-center justify-between gap-3">
    <h3 class="text-sm font-semibold">{title}</h3>
    <Badge variant="outline">{rows.length}</Badge>
  </div>
  <Tooltip.Provider delayDuration={0} skipDelayDuration={0} disableHoverableContent>
    <div class="overflow-hidden rounded-xl border">
      <Table class={showScopeColumns ? 'min-w-[60rem] table-fixed text-xs' : 'min-w-[40rem] table-fixed text-xs'}>
      {#if showScopeColumns}
        <colgroup>
          <col class="w-[24%]" />
          <col class="w-[9%]" />
          <col class="w-[9%]" />
          <col class="w-[9%]" />
          <col class="w-[19%]" />
          <col class="w-[10%]" />
          <col class="w-[11%]" />
          {#if showRepeatColumn}<col class="w-[9%]" />{/if}
        </colgroup>
      {:else}
        <colgroup>
          <col class="w-[30%]" />
          <col class="w-[12%]" />
          <col class="w-[25%]" />
          <col class="w-[15%]" />
          <col class="w-[18%]" />
        </colgroup>
      {/if}
      <TableHeader class="bg-muted/40">
        <TableRow>
          <TableHead class={showHeaderHelp ? 'h-9 p-0 text-xs' : 'h-9 text-xs'}>
            {@render header(dimensionLabel, i18nManager.t('tool_calls.comparison.help.tool_name'), 'left')}
          </TableHead>
          <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
            {@render header(i18nManager.t('tool_calls.comparison.calls'), i18nManager.t('tool_calls.comparison.help.calls'))}
          </TableHead>
          {#if showScopeColumns}
            <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
              {@render header(i18nManager.t('tool_calls.comparison.sessions'), i18nManager.t('tool_calls.comparison.help.sessions'))}
            </TableHead>
            <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
              {@render header(i18nManager.t('tool_calls.comparison.projects'), i18nManager.t('tool_calls.comparison.help.projects'))}
            </TableHead>
          {/if}
          <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
            {@render header(i18nManager.t('tool_calls.comparison.outcomes'), i18nManager.t('tool_calls.comparison.help.outcomes'))}
          </TableHead>
          <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
            {@render header(i18nManager.t('tool_calls.comparison.success'), i18nManager.t('tool_calls.comparison.help.success'))}
          </TableHead>
          <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
            {@render header(i18nManager.t('tool_calls.comparison.duration'), i18nManager.t('tool_calls.comparison.help.duration'))}
          </TableHead>
          {#if showRepeatColumn}
            <TableHead class={showHeaderHelp ? 'h-9 p-0 text-right text-xs' : 'h-9 text-right text-xs'}>
              {@render header(i18nManager.t('tool_calls.comparison.repeats'), i18nManager.t('tool_calls.comparison.help.repeats'))}
            </TableHead>
          {/if}
        </TableRow>
      </TableHeader>
      <TableBody>
        {#if rows.length === 0}
          <TableRow><TableCell colspan={5 + (showScopeColumns ? 2 : 0) + (showRepeatColumn ? 1 : 0)} class="h-24 text-center text-muted-foreground">{emptyText}</TableCell></TableRow>
        {:else}
          {#each visibleRows as row (row.key)}
            <TableRow>
              <TableCell class="px-3 py-2.5">
                <p class="max-w-56 truncate font-medium" title={row.label || i18nManager.t('tool_calls.unknown')}>{row.label || i18nManager.t('tool_calls.unknown')}</p>
              </TableCell>
              <TableCell class="px-3 text-right tabular-nums">{row.callCount}</TableCell>
              {#if showScopeColumns}
                <TableCell class="px-3 text-right tabular-nums">{row.sessionCount}</TableCell>
                <TableCell class="px-3 text-right tabular-nums">{row.projectCount}</TableCell>
              {/if}
              <TableCell class="px-3 text-right whitespace-nowrap tabular-nums">{row.failedCount} / {row.declinedCount} / {row.cancelledCount}</TableCell>
              <TableCell class="px-3 text-right tabular-nums">{percent(row.successRate)}</TableCell>
              <TableCell class="px-3 text-right whitespace-nowrap tabular-nums">{duration(row.averageDurationMs)}</TableCell>
              {#if showRepeatColumn}
                <TableCell class="px-3 text-right tabular-nums">{row.exactRepeatCount}</TableCell>
              {/if}
            </TableRow>
          {/each}
        {/if}
      </TableBody>
      </Table>
    </div>
  </Tooltip.Provider>
  {#if paginated && rows.length > 0}
    <DataPagination
      page={currentPage}
      {pageSize}
      totalCount={rows.length}
      totalLabel={i18nManager.t('tool_calls.comparison.total', { count: rows.length })}
      {idPrefix}
      pageSizeOptions={[10, 25, 50]}
      onPageChange={(value) => (page = value)}
      onPageSizeChange={(value) => { pageSize = value; page = 1; }}
    />
  {/if}
</section>
