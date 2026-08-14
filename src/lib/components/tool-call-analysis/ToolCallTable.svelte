<script lang="ts">
import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
import ArrowUpIcon from '@lucide/svelte/icons/arrow-up';
import SearchIcon from '@lucide/svelte/icons/search';
import DataPagination from '$lib/components/data-pagination/DataPagination.svelte';
import { Badge } from '$lib/components/ui/badge';
import { Button } from '$lib/components/ui/button';
import { Input } from '$lib/components/ui/input';
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
import type { ToolCallListItem, ToolCallPage } from '$lib/tool-call-analysis';

type StatusBadgeVariant = 'success' | 'warning' | 'outline' | 'destructive';

let {
  data,
  loading,
  query,
  onquery,
  onopen,
  onpage,
  onpagesize,
  sortBy,
  sortDirection,
  onsort,
}: {
  data: ToolCallPage | null;
  loading: boolean;
  query: string;
  onquery: (value: string) => void;
  onopen: (call: ToolCallListItem) => void;
  onpage: (page: number) => void;
  onpagesize: (size: number) => void;
  sortBy: string;
  sortDirection: 'asc' | 'desc';
  onsort: (field: string) => void;
} = $props();

let queryInput = $derived(query);

function submit(event: SubmitEvent) {
  event.preventDefault();
  onquery(queryInput.trim());
}

function openFromKeyboard(event: KeyboardEvent, call: ToolCallListItem) {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    onopen(call);
  }
}

function date(value: number | null): string {
  return value === null
    ? '—'
    : new Intl.DateTimeFormat(undefined, { dateStyle: 'short', timeStyle: 'short' }).format(value);
}

function sortLabel(field: string, label: string): string {
  if (sortBy !== field) return i18nManager.t('tool_calls.sort.label', { label });
  return i18nManager.t('tool_calls.sort.current', {
    label,
    direction: i18nManager.t(`tool_calls.sort.${sortDirection}`),
  });
}

function statusVariant(status: string): StatusBadgeVariant {
  if (status === 'failed' || status === 'cancelled' || status === 'declined') {
    return 'destructive';
  }
  if (status === 'completed' || status === 'succeeded') return 'success';
  if (status === 'in_progress' || status === 'active' || status === 'awaiting_approval') {
    return 'warning';
  }
  return 'outline';
}
</script>

<section class="flex min-h-0 flex-col gap-3" data-testid="tool-call-table">
  <form class="flex max-w-lg gap-2" onsubmit={submit}>
    <Input class="bg-muted/20 text-xs shadow-none" bind:value={queryInput} placeholder={i18nManager.t('tool_calls.table.search_placeholder')} aria-label={i18nManager.t('tool_calls.table.search_label')} />
    <Button type="submit" variant="outline" size="xs"><SearchIcon data-icon="inline-start" />{i18nManager.t('tool_calls.table.search')}</Button>
  </form>

  <Tooltip.Provider delayDuration={0} skipDelayDuration={0} disableHoverableContent>
    <div class="min-h-0 overflow-auto rounded-xl border">
      <Table class="min-w-[72rem] table-fixed text-xs">
      <colgroup>
        <col class="w-[17%]" />
        <col class="w-[11%]" />
        <col class="w-[13%]" />
        <col class="w-[23%]" />
        <col class="w-[11%]" />
        <col class="w-[10%]" />
        <col class="w-[15%]" />
      </colgroup>
      <TableHeader class="sticky top-0 z-10 bg-muted/40 backdrop-blur-sm">
        <TableRow>
          <TableHead class="h-9"><Button class="px-0 text-xs" variant="ghost" size="xs" data-testid="tool-call-header-tool" aria-label={sortLabel('toolName', i18nManager.t('tool_calls.table.tool'))} onclick={() => onsort('toolName')}>{i18nManager.t('tool_calls.table.tool')}{#if sortBy === 'toolName'}{#if sortDirection === 'asc'}<ArrowUpIcon data-icon="inline-end" />{:else}<ArrowDownIcon data-icon="inline-end" />{/if}{/if}</Button></TableHead>
          <TableHead class="h-9"><Button class="px-0 text-xs" variant="ghost" size="xs" data-testid="tool-call-header-provider" aria-label={sortLabel('provider', i18nManager.t('tool_calls.table.provider'))} onclick={() => onsort('provider')}>{i18nManager.t('tool_calls.table.provider')}{#if sortBy === 'provider'}{#if sortDirection === 'asc'}<ArrowUpIcon data-icon="inline-end" />{:else}<ArrowDownIcon data-icon="inline-end" />{/if}{/if}</Button></TableHead>
          <TableHead class="h-9 text-xs">{i18nManager.t('tool_calls.table.mcp_server')}</TableHead>
          <TableHead class="h-9 text-xs">{i18nManager.t('tool_calls.table.project_session')}</TableHead>
          <TableHead class="h-9"><Button class="px-0 text-xs" variant="ghost" size="xs" aria-label={sortLabel('status', i18nManager.t('tool_calls.table.status'))} onclick={() => onsort('status')}>{i18nManager.t('tool_calls.table.status')}{#if sortBy === 'status'}{#if sortDirection === 'asc'}<ArrowUpIcon data-icon="inline-end" />{:else}<ArrowDownIcon data-icon="inline-end" />{/if}{/if}</Button></TableHead>
          <TableHead class="h-9 text-right"><Button class="ml-auto px-0 text-xs" variant="ghost" size="xs" aria-label={sortLabel('durationMs', i18nManager.t('tool_calls.table.duration'))} onclick={() => onsort('durationMs')}>{i18nManager.t('tool_calls.table.duration')}{#if sortBy === 'durationMs'}{#if sortDirection === 'asc'}<ArrowUpIcon data-icon="inline-end" />{:else}<ArrowDownIcon data-icon="inline-end" />{/if}{/if}</Button></TableHead>
          <TableHead class="h-9 pr-4 text-right"><Button class="ml-auto px-0 text-xs" variant="ghost" size="xs" aria-label={sortLabel('startedAtMs', i18nManager.t('tool_calls.table.time'))} onclick={() => onsort('startedAtMs')}>{i18nManager.t('tool_calls.table.time')}{#if sortBy === 'startedAtMs'}{#if sortDirection === 'asc'}<ArrowUpIcon data-icon="inline-end" />{:else}<ArrowDownIcon data-icon="inline-end" />{/if}{/if}</Button></TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {#if loading && !data}
          <TableRow><TableCell colspan={7} class="h-32 text-center text-muted-foreground">{i18nManager.t('tool_calls.table.loading')}</TableCell></TableRow>
        {:else if !data?.items.length}
          <TableRow><TableCell colspan={7} class="h-32 text-center text-muted-foreground">{i18nManager.t('tool_calls.table.empty')}</TableCell></TableRow>
        {:else}
          {#each data.items as call (call.id)}
            <TableRow
              data-testid={`tool-call-row-${call.id}`}
              class="cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              tabindex={0}
              onclick={() => onopen(call)}
              onkeydown={(event) => openFromKeyboard(event, call)}
            >
              <TableCell class="py-2 font-medium"><span data-testid={`tool-call-tool-${call.id}`}>{call.toolName}</span></TableCell>
              <TableCell class="py-2"><span data-testid={`tool-call-provider-${call.id}`}>{call.provider}</span></TableCell>
              <TableCell class="max-w-40 truncate py-2">{call.mcpServer || i18nManager.t('tool_calls.unknown')}</TableCell>
              <TableCell class="py-2">
                <p class="max-w-44 truncate">{call.projectName || i18nManager.t('tool_calls.unknown_project')}</p>
                <Tooltip.Root delayDuration={0} disableHoverableContent>
                  <Tooltip.Trigger>
                    {#snippet child({ props })}
                      <p {...props} class="max-w-48 truncate text-xs text-muted-foreground" data-testid={`tool-call-session-title-${call.id}`}>{call.sessionTitle || i18nManager.t('tool_calls.untitled_session')}</p>
                    {/snippet}
                  </Tooltip.Trigger>
                  <Tooltip.Content class="max-w-sm whitespace-normal break-words leading-relaxed" side="top" sideOffset={4} data-testid={`tool-call-session-title-tooltip-${call.id}`}>{call.sessionTitle || i18nManager.t('tool_calls.untitled_session')}</Tooltip.Content>
                </Tooltip.Root>
              </TableCell>
              <TableCell class="py-2"><Badge variant={statusVariant(call.status)} data-testid={`tool-call-status-${call.id}`}>{call.status}</Badge></TableCell>
              <TableCell class="py-2 text-right tabular-nums">{call.durationMs === null ? '—' : `${call.durationMs} ms`}</TableCell>
              <TableCell class="py-2 pr-4 text-right whitespace-nowrap text-muted-foreground">{date(call.startedAtMs ?? call.completedAtMs)}</TableCell>
            </TableRow>
          {/each}
        {/if}
      </TableBody>
      </Table>
    </div>
  </Tooltip.Provider>

  {#if data}
    <DataPagination
      page={data.page}
      pageSize={data.pageSize}
      totalCount={data.total}
      totalLabel={i18nManager.t('tool_calls.table.total', { count: data.total })}
      idPrefix="tool-calls"
      {loading}
      onPageChange={onpage}
      onPageSizeChange={onpagesize}
    />
  {/if}
</section>
