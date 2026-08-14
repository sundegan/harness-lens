<script lang="ts">
import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
import AlertTriangleIcon from '@lucide/svelte/icons/triangle-alert';
import WrenchIcon from '@lucide/svelte/icons/wrench';
import { onMount } from 'svelte';
import * as Alert from '$lib/components/ui/alert';
import { Badge } from '$lib/components/ui/badge';
import { Button } from '$lib/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
import { Input } from '$lib/components/ui/input';
import { Skeleton } from '$lib/components/ui/skeleton';
import * as ToggleGroup from '$lib/components/ui/toggle-group';
import { i18nManager } from '$lib/i18n.svelte';
import { logWarn } from '$lib/logger';
import {
  defaultToolCallFilters,
  getToolCallAnalysis,
  getToolCallDetail,
  getToolCallFilterOptions,
  getToolCallPage,
  type ToolCallAnalysisData,
  type ToolCallDetail,
  type ToolCallFilterOptions,
  type ToolCallFilters,
  type ToolCallListItem,
  type ToolCallPage,
} from '$lib/tool-call-analysis';
import { createTrailingRefresh } from '$lib/trailing-refresh';
import { getWorkspaceNavigation } from '$lib/workspace-navigation.svelte';
import ToolCallComparisons from './ToolCallComparisons.svelte';
import ToolCallDetailView from './ToolCallDetail.svelte';
import ToolCallFiltersView from './ToolCallFilters.svelte';
import ToolCallStatusDistribution from './ToolCallStatusDistribution.svelte';
import ToolCallTable from './ToolCallTable.svelte';
import ToolCallTrend from './ToolCallTrend.svelte';

let { active = true }: { active?: boolean } = $props();

type ViewId = 'overview' | 'comparisons' | 'calls';

const navigation = getWorkspaceNavigation();
let filters = $state.raw<ToolCallFilters>(defaultToolCallFilters());
let analysis = $state.raw<ToolCallAnalysisData | null>(null);
let pageData = $state.raw<ToolCallPage | null>(null);
let filterOptions = $state.raw<ToolCallFilterOptions | null>(null);
let detail = $state.raw<ToolCallDetail | null>(null);
let view = $state<ViewId>('overview');
let page = $state(1);
let pageSize = $state(25);
let query = $state('');
let sortBy = $state('startedAtMs');
let sortDirection = $state<'asc' | 'desc'>('desc');
let loading = $state(true);
let pageLoading = $state(true);
let detailLoading = $state(false);
let error = $state('');
let rangePreset = $state('30');
let customStart = $state('');
let customEnd = $state('');
let rangeError = $state('');
let refreshWhenActive = $state(false);
let analysisSequence = 0;
let pageSequence = 0;
let detailSequence = 0;
const refresh = createTrailingRefresh(loadAll);

onMount(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  refresh.request(true);
  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { listen } = await import('@tauri-apps/api/event');
    const stop = await listen('analytics-updated', () => {
      if (disposed) return;
      if (active) refresh.request();
      else refreshWhenActive = true;
    });
    if (disposed) stop();
    else unlisten = stop;
  })().catch((cause) => logWarn('Failed to listen for tool-call updates', cause));
  return () => {
    disposed = true;
    analysisSequence += 1;
    pageSequence += 1;
    detailSequence += 1;
    refresh.cancel();
    unlisten?.();
  };
});

export function activate() {
  if (refreshWhenActive) {
    refreshWhenActive = false;
    refresh.request();
  }
}

async function loadAll() {
  const sequence = ++analysisSequence;
  const currentPageSequence = ++pageSequence;
  const shouldLoadPage = view === 'calls' || pageData !== null;
  loading = true;
  pageLoading = shouldLoadPage;
  error = '';
  const nextPageRequest = {
    ...filters,
    page,
    pageSize,
    query: query || null,
    sortBy,
    sortDirection,
  };
  const analysisRequest = getToolCallAnalysis(filters)
    .then((result) => {
      if (sequence === analysisSequence) analysis = result;
    })
    .catch((cause) => {
      if (sequence === analysisSequence) {
        error = cause instanceof Error ? cause.message : String(cause);
      }
    })
    .finally(() => {
      if (sequence === analysisSequence) loading = false;
    });
  const pageRequest = shouldLoadPage
    ? getToolCallPage(nextPageRequest)
        .then((result) => {
          if (currentPageSequence !== pageSequence) return;
          pageData = result;
          page = result.page;
          pageSize = result.pageSize;
        })
        .catch((cause) => {
          if (currentPageSequence === pageSequence) {
            error = cause instanceof Error ? cause.message : String(cause);
          }
        })
        .finally(() => {
          if (currentPageSequence === pageSequence) pageLoading = false;
        })
    : Promise.resolve();

  await Promise.all([analysisRequest, pageRequest]);
  if (sequence !== analysisSequence) return;
  try {
    const nextOptions = await getToolCallFilterOptions(filters);
    if (sequence === analysisSequence) filterOptions = nextOptions;
  } catch (cause) {
    if (sequence === analysisSequence) {
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }
}

function setView(nextView: ViewId) {
  view = nextView;
  if (nextView === 'calls' && pageData === null && !pageLoading) void loadPage();
}

async function loadPage() {
  const sequence = ++pageSequence;
  pageLoading = true;
  error = '';
  try {
    const result = await getToolCallPage({
      ...filters,
      page,
      pageSize,
      query: query || null,
      sortBy,
      sortDirection,
    });
    if (sequence !== pageSequence) return;
    pageData = result;
    page = result.page;
    pageSize = result.pageSize;
  } catch (cause) {
    if (sequence === pageSequence) error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (sequence === pageSequence) pageLoading = false;
  }
}

function setRange(days: number | null) {
  filters = {
    ...filters,
    startAtMs: days === null ? null : Date.now() - days * 86_400_000,
    endAtMs: null,
  };
  page = 1;
  refresh.request(true);
}

function applyCustomRange() {
  const start = Date.parse(customStart);
  const end = Date.parse(customEnd);
  if (!Number.isFinite(start) || !Number.isFinite(end) || start >= end) {
    rangeError = i18nManager.t('tool_calls.range.invalid');
    return;
  }
  rangeError = '';
  filters = { ...filters, startAtMs: start, endAtMs: end };
  page = 1;
  refresh.request(true);
}

function setBucket(value: string) {
  if (!['hour', 'day', 'week', 'month'].includes(value)) return;
  filters = { ...filters, bucket: value as ToolCallFilters['bucket'] };
  refresh.request(true);
}

function setSort(field: string) {
  if (sortBy === field) sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
  else {
    sortBy = field;
    sortDirection = 'asc';
  }
  page = 1;
  void loadPage();
}

async function openDetail(call: ToolCallListItem) {
  const sequence = ++detailSequence;
  detailLoading = true;
  error = '';
  try {
    const result = await getToolCallDetail(call.id);
    if (sequence === detailSequence) detail = result;
  } catch (cause) {
    if (sequence === detailSequence) error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (sequence === detailSequence) detailLoading = false;
  }
}

function percent(value: number | null): string {
  return value === null ? '—' : `${(value * 100).toFixed(1)}%`;
}

function duration(value: number | null): string {
  return value === null
    ? '—'
    : value < 1000
      ? `${Math.round(value)} ms`
      : `${(value / 1000).toFixed(1)} s`;
}
</script>

<div class="flex min-w-0 flex-1 flex-col" data-testid="tool-call-analysis">
  <section class="border-b bg-muted/15 px-5 py-5 sm:px-6" aria-labelledby="tool-call-overview-title">
    <div class="flex flex-col justify-between gap-3 xl:flex-row xl:items-end">
      <div class="flex items-center gap-2">
        <span class="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
          <WrenchIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
        </span>
        <div>
          <h2 id="tool-call-overview-title" class="text-sm font-semibold">{i18nManager.t('tool_calls.title')}</h2>
          <p class="mt-0.5 text-xs text-muted-foreground">{i18nManager.t('tool_calls.description')}</p>
        </div>
      </div>
      <div class="flex flex-wrap items-center gap-2">
      <ToggleGroup.Root class="bg-muted/30 p-0.5" type="single" value={rangePreset} variant="outline" size="sm" onValueChange={(value) => {
        if (!value) return; rangePreset = value;
        if (value === '1') setRange(1); else if (value === '7') setRange(7); else if (value === '30') setRange(30); else if (value === '90') setRange(90); else if (value === 'all') setRange(null);
      }}>
        <ToggleGroup.Item class="h-7 shadow-none" value="1">{i18nManager.t('tool_calls.range.24h')}</ToggleGroup.Item><ToggleGroup.Item class="h-7 shadow-none" value="7">{i18nManager.t('tool_calls.range.7d')}</ToggleGroup.Item>
        <ToggleGroup.Item class="h-7 shadow-none" value="30">{i18nManager.t('tool_calls.range.30d')}</ToggleGroup.Item><ToggleGroup.Item class="h-7 shadow-none" value="90">{i18nManager.t('tool_calls.range.90d')}</ToggleGroup.Item><ToggleGroup.Item class="h-7 shadow-none" value="all">{i18nManager.t('tool_calls.range.all')}</ToggleGroup.Item><ToggleGroup.Item class="h-7 shadow-none" value="custom">{i18nManager.t('tool_calls.range.custom')}</ToggleGroup.Item>
      </ToggleGroup.Root>
      <Button variant="outline" size="sm" onclick={() => refresh.request(true)} disabled={loading}>
        <RefreshCwIcon data-icon="inline-start" />{i18nManager.t('tool_calls.refresh')}
      </Button>
      </div>
    </div>

  </section>

  <nav class="flex gap-1 border-b bg-card px-5 py-2 sm:px-6" aria-label={i18nManager.t('tool_calls.view.label')}>
    {#each [['overview', i18nManager.t('tool_calls.view.overview')], ['comparisons', i18nManager.t('tool_calls.view.comparisons')], ['calls', i18nManager.t('tool_calls.view.calls')]] as tab (tab[0])}
      <Button data-testid={`tool-call-view-${tab[0]}`} variant={view === tab[0] ? 'secondary' : 'ghost'} size="sm" onclick={() => setView(tab[0] as ViewId)}>{tab[1]}</Button>
    {/each}
  </nav>

  <ToolCallFiltersView
    {filters}
    options={filterOptions}
    onchange={(value) => {
      filters = value;
      page = 1;
      refresh.request(true);
    }}
  />

  {#if rangePreset === 'custom'}
    <form class="flex flex-wrap items-end gap-2 border-b bg-card px-5 py-2.5 sm:px-6" onsubmit={(event) => { event.preventDefault(); applyCustomRange(); }}>
      <label class="grid gap-1 text-[11px] text-muted-foreground">{i18nManager.t('tool_calls.range.start')}<Input class="h-8 text-xs" type="datetime-local" bind:value={customStart} aria-invalid={rangeError ? 'true' : undefined} /></label>
      <label class="grid gap-1 text-[11px] text-muted-foreground">{i18nManager.t('tool_calls.range.end')}<Input class="h-8 text-xs" type="datetime-local" bind:value={customEnd} aria-invalid={rangeError ? 'true' : undefined} /></label>
      <Button type="submit" size="sm">{i18nManager.t('tool_calls.range.apply')}</Button>
      {#if rangeError}<p class="text-xs text-destructive" role="alert">{rangeError}</p>{/if}
    </form>
  {/if}

  <main class="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-5 sm:p-6">
    {#if error}
      <Alert.Root variant="destructive"><AlertTriangleIcon /><Alert.Title>{i18nManager.t('tool_calls.load_error')}</Alert.Title><Alert.Description>{error}</Alert.Description></Alert.Root>
    {/if}
    {#if loading && !analysis}
      <div class="grid gap-3 md:grid-cols-2">{#each [1, 2] as item (item)}<Skeleton class="h-44" />{/each}</div>
    {:else if analysis}
      {#if view === 'overview'}
        <dl class="grid grid-cols-2 gap-px overflow-hidden rounded-xl border bg-border sm:grid-cols-3 xl:grid-cols-6">
          {#each [
            { label: i18nManager.t('tool_calls.metric.calls'), value: String(analysis.summary.callCount), note: '' },
            { label: i18nManager.t('tool_calls.metric.success'), value: percent(analysis.summary.successRate.rate), note: i18nManager.t('tool_calls.metric.terminal', { count: analysis.summary.successRate.denominator }) },
            { label: i18nManager.t('tool_calls.metric.projects'), value: String(analysis.summary.projectCount), note: '' },
            { label: i18nManager.t('tool_calls.metric.sessions'), value: String(analysis.summary.sessionCount), note: '' },
            { label: i18nManager.t('tool_calls.metric.tools'), value: String(analysis.summary.toolCount), note: '' },
            { label: i18nManager.t('tool_calls.metric.average_duration'), value: duration(analysis.summary.averageDurationMs), note: '' },
          ] as metric (metric.label)}
            <div class="min-w-0 bg-card px-4 py-3.5">
              <dt class="truncate text-[11px] font-medium text-muted-foreground">{metric.label}</dt>
              <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">{metric.value}</dd>
              {#if metric.note}<dd class="mt-0.5 truncate text-[11px] text-muted-foreground" title={metric.note}>{metric.note}</dd>{/if}
            </div>
          {/each}
        </dl>
        <div class="grid items-start gap-4 md:grid-cols-2">
          <Card size="sm" class="rounded-xl shadow-none">
            <CardHeader class="flex items-center justify-between gap-3">
              <CardTitle class="text-sm font-semibold">{i18nManager.t('tool_calls.trend.title')}</CardTitle>
              <ToggleGroup.Root class="ml-auto shrink-0" type="single" value={filters.bucket} variant="outline" size="sm" onValueChange={setBucket}>
                <ToggleGroup.Item class="h-6 min-w-6 px-1.5 text-[11px]" value="hour">{i18nManager.t('tool_calls.bucket.hour')}</ToggleGroup.Item><ToggleGroup.Item class="h-6 min-w-6 px-1.5 text-[11px]" value="day">{i18nManager.t('tool_calls.bucket.day')}</ToggleGroup.Item><ToggleGroup.Item class="h-6 min-w-6 px-1.5 text-[11px]" value="week">{i18nManager.t('tool_calls.bucket.week')}</ToggleGroup.Item><ToggleGroup.Item class="h-6 min-w-6 px-1.5 text-[11px]" value="month">{i18nManager.t('tool_calls.bucket.month')}</ToggleGroup.Item>
              </ToggleGroup.Root>
            </CardHeader>
            <CardContent><ToolCallTrend points={analysis.timeTrend} /></CardContent>
          </Card>
          <Card size="sm" class="rounded-xl shadow-none">
            <CardContent><ToolCallStatusDistribution rows={analysis.statusDistribution} /></CardContent>
          </Card>
        </div>
        <ToolCallComparisons
          testId="tool-call-ranking"
          title={i18nManager.t('tool_calls.comparison.tools')}
          dimensionLabel={i18nManager.t('tool_calls.comparison.tool_name')}
          showScopeColumns
          showRepeatColumn
          showHeaderHelp
          rows={analysis.toolRanking}
          paginated
          idPrefix="tool-ranking"
        />
      {:else if view === 'comparisons'}
        <div class="grid gap-4 xl:grid-cols-2">
          <ToolCallComparisons testId="tool-call-comparison-provider" title={i18nManager.t('tool_calls.comparison.provider')} rows={analysis.providerComparison} paginated idPrefix="provider-comparison" />
          <ToolCallComparisons testId="tool-call-comparison-mcp" title={i18nManager.t('tool_calls.comparison.mcp')} rows={analysis.mcpServerComparison} emptyText={i18nManager.t('tool_calls.comparison.mcp_empty')} paginated idPrefix="mcp-comparison" />
          <div class="min-w-0 xl:col-span-2">
            <ToolCallComparisons testId="tool-call-comparison-project" title={i18nManager.t('tool_calls.comparison.project')} rows={analysis.projectComparison} paginated idPrefix="project-comparison" />
          </div>
        </div>
      {:else}
        <ToolCallTable
          data={pageData}
          loading={pageLoading}
          {query}
          onquery={(value) => { query = value; page = 1; void loadPage(); }}
          onopen={(call) => void openDetail(call)}
          onpage={(value) => { page = value; void loadPage(); }}
          onpagesize={(value) => { pageSize = value; page = 1; void loadPage(); }}
          {sortBy}
          {sortDirection}
          onsort={setSort}
        />
      {/if}
    {/if}
  </main>
</div>

{#if detailLoading}<div class="sr-only" aria-live="polite">{i18nManager.t('tool_calls.loading_detail')}</div>{/if}
{#if detail}
  <ToolCallDetailView
    {detail}
    onclose={() => { detailSequence += 1; detail = null; }}
    onviewsession={(sessionId, eventId) => navigation.navigate({ module: 'sessions', sessionId, eventId })}
  />
{/if}
