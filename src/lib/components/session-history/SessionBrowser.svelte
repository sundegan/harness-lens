<script lang="ts">
import ActivityIcon from '@lucide/svelte/icons/activity';
import ArchiveIcon from '@lucide/svelte/icons/archive';
import BoxesIcon from '@lucide/svelte/icons/boxes';
import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
import Clock3Icon from '@lucide/svelte/icons/clock-3';
import GitBranchIcon from '@lucide/svelte/icons/git-branch';
import InboxIcon from '@lucide/svelte/icons/inbox';
import Layers3Icon from '@lucide/svelte/icons/layers-3';
import SearchIcon from '@lucide/svelte/icons/search';
import WorkflowIcon from '@lucide/svelte/icons/workflow';
import { onMount } from 'svelte';
import DataPagination from '$lib/components/data-pagination/DataPagination.svelte';
import SessionDetailView from '$lib/components/session-history/SessionDetail.svelte';
import * as Alert from '$lib/components/ui/alert';
import { Badge } from '$lib/components/ui/badge';
import { Button } from '$lib/components/ui/button';
import * as Empty from '$lib/components/ui/empty';
import { Input } from '$lib/components/ui/input';
import { Skeleton } from '$lib/components/ui/skeleton';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '$lib/components/ui/table';
import * as ToggleGroup from '$lib/components/ui/toggle-group';
import { i18nManager } from '$lib/i18n.svelte';
import { logWarn } from '$lib/logger';
import {
  getSessionDetail,
  getSessionPage,
  type SessionDetail,
  type SessionListItem,
  type SessionPage,
} from '$lib/session-history';
import { createTrailingRefresh } from '$lib/trailing-refresh';

let { active = true }: { active?: boolean } = $props();

type ArchiveFilter = 'all' | 'current' | 'archived';
type StatusBadgeVariant = 'success' | 'warning' | 'outline' | 'destructive';

let pageData = $state.raw<SessionPage | null>(null);
let selectedDetail = $state.raw<SessionDetail | null>(null);
let loading = $state(true);
let detailLoading = $state(false);
let error = $state('');
let detailError = $state('');
let page = $state(1);
let pageSize = $state(25);
let queryInput = $state('');
let appliedQuery = $state('');
let archiveFilter = $state<ArchiveFilter>('all');
let requestSequence = 0;
let detailRequestSequence = 0;
let refreshWhenActive = $state(false);
const pageRefresh = createTrailingRefresh(loadPage);

const pageCount = $derived(Math.max(1, Math.ceil((pageData?.total ?? 0) / pageSize)));
const pageRuns = $derived(
  (pageData?.items ?? []).reduce((total, item) => total + item.invocationCount, 0)
);
const pageEvents = $derived(
  (pageData?.items ?? []).reduce((total, item) => total + item.eventCount, 0)
);
const pageTokens = $derived(
  (pageData?.items ?? []).reduce((total, item) => total + item.tokensUsed, 0)
);

onMount(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;

  pageRefresh.request(true);
  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { listen } = await import('@tauri-apps/api/event');
    const stopListening = await listen('analytics-updated', () => {
      if (disposed) return;
      if (active) pageRefresh.request();
      else refreshWhenActive = true;
    });
    if (disposed) stopListening();
    else unlisten = stopListening;
  })().catch((cause) => logWarn('Failed to listen for analytics updates', cause));

  return () => {
    disposed = true;
    requestSequence += 1;
    detailRequestSequence += 1;
    pageRefresh.cancel();
    unlisten?.();
  };
});

export function activate() {
  if (refreshWhenActive) {
    refreshWhenActive = false;
    pageRefresh.request();
  }
}

function archivedValue(): boolean | null {
  if (archiveFilter === 'current') return false;
  if (archiveFilter === 'archived') return true;
  return null;
}

async function loadPage() {
  const sequence = ++requestSequence;
  loading = true;
  error = '';
  try {
    const result = await getSessionPage({
      page,
      pageSize,
      query: appliedQuery || null,
      archived: archivedValue(),
    });
    if (sequence !== requestSequence) return;
    pageData = result;
    page = result.page;
    pageSize = result.pageSize;
  } catch (cause) {
    if (sequence !== requestSequence) return;
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (sequence === requestSequence) loading = false;
  }
}

async function openSession(session: SessionListItem) {
  const sequence = ++detailRequestSequence;
  selectedDetail = null;
  detailError = '';
  detailLoading = true;
  try {
    const result = await getSessionDetail(session.id);
    if (sequence !== detailRequestSequence) return;
    if (result) {
      selectedDetail = result;
    } else {
      detailError = i18nManager.t('sessions.detail.not_found');
    }
  } catch (cause) {
    if (sequence !== detailRequestSequence) return;
    detailError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (sequence === detailRequestSequence) detailLoading = false;
  }
}

function closeDetail() {
  detailRequestSequence += 1;
  selectedDetail = null;
  detailError = '';
  detailLoading = false;
}

function submitSearch(event: SubmitEvent) {
  event.preventDefault();
  appliedQuery = queryInput.trim();
  page = 1;
  refreshPageImmediately();
}

function setArchiveFilter(value: ArchiveFilter) {
  if (archiveFilter === value) return;
  archiveFilter = value;
  page = 1;
  refreshPageImmediately();
}

function goToPage(value: number) {
  if (value < 1 || value > pageCount || value === page) return;
  page = value;
  refreshPageImmediately();
}

function setPageSize(value: number) {
  if (value === pageSize) return;
  pageSize = value;
  page = 1;
  refreshPageImmediately();
}

function refreshPageImmediately() {
  requestSequence += 1;
  pageRefresh.request(true);
}

function handleRowKeydown(event: KeyboardEvent, session: SessionListItem) {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    void openSession(session);
  }
}

function displayTitle(session: SessionListItem): string {
  return session.title.trim() || i18nManager.t('sessions.untitled');
}

function projectInitial(session: SessionListItem): string {
  const value = session.projectName.trim() || displayTitle(session);
  return value.slice(0, 1).toLocaleUpperCase() || 'H';
}

function formatDate(value: number | null): string {
  if (value === null) return '—';
  return new Intl.DateTimeFormat(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
}

function formatTime(value: number | null): string {
  if (value === null) return '—';
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { notation: 'compact' }).format(value);
}

function statusLabel(status: string): string {
  const key = `sessions.status.${status}`;
  const translated = i18nManager.t(key);
  return translated === key ? status.replaceAll('_', ' ') : translated;
}

function statusVariant(status: string): StatusBadgeVariant {
  if (status === 'failed' || status === 'cancelled') return 'destructive';
  if (status === 'succeeded' || status === 'completed') return 'success';
  if (status === 'in_progress' || status === 'active' || status === 'awaiting_approval') {
    return 'warning';
  }
  return 'outline';
}

function statusDotClass(status: string): string {
  if (status === 'failed' || status === 'cancelled') return 'bg-destructive';
  if (status === 'succeeded' || status === 'completed') return 'bg-success';
  if (status === 'in_progress' || status === 'active' || status === 'awaiting_approval') {
    return 'bg-warning';
  }
  return 'bg-muted-foreground';
}
</script>

{#if selectedDetail}
  <SessionDetailView detail={selectedDetail} onback={closeDetail} />
{:else if detailLoading}
  <div class="flex min-h-[36rem] w-full flex-col p-5 sm:p-6" role="status">
    <span class="sr-only">{i18nManager.t('sessions.detail.loading')}</span>
    <Skeleton class="h-8 w-36" />
    <div class="mt-5 rounded-xl border p-5">
      <Skeleton class="h-7 w-2/5" />
      <Skeleton class="mt-3 h-4 w-28" />
      <div class="mt-6 grid grid-cols-2 gap-3 lg:grid-cols-4">
        {#each ['a', 'b', 'c', 'd'] as item (item)}
          <Skeleton class="h-20" />
        {/each}
      </div>
    </div>
    <div class="mt-6 flex flex-col gap-3">
      <Skeleton class="h-12 w-full" />
      <Skeleton class="h-28 w-full" />
      <Skeleton class="h-20 w-full" />
    </div>
  </div>
{:else if detailError}
  <Empty.Root class="min-h-[32rem]">
    <Empty.Header>
      <Empty.Media variant="icon" class="bg-destructive/10 text-destructive">
        <CircleAlertIcon strokeWidth={1.7} aria-hidden="true" />
      </Empty.Media>
      <Empty.Title class="text-base">{i18nManager.t('sessions.detail.load_error')}</Empty.Title>
      <Empty.Description>{detailError}</Empty.Description>
    </Empty.Header>
    <Empty.Content>
      <Button variant="outline" size="sm" onclick={closeDetail}>
        {i18nManager.t('sessions.detail.back')}
      </Button>
    </Empty.Content>
  </Empty.Root>
{:else}
  <div
    class="flex min-w-0 flex-1 flex-col"
    data-testid="session-browser"
    aria-busy={loading}
  >
    <section class="border-b bg-muted/15 px-5 py-5 sm:px-6" aria-labelledby="session-index-title">
      <div class="flex flex-col justify-between gap-3 sm:flex-row sm:items-end">
        <div>
          <div class="flex items-center gap-2">
            <span class="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Layers3Icon class="size-4" strokeWidth={1.8} aria-hidden="true" />
            </span>
            <div>
              <h2 id="session-index-title" class="text-sm font-semibold">
                {i18nManager.t('sessions.overview.title')}
              </h2>
              <p class="mt-0.5 text-xs text-muted-foreground">
                {i18nManager.t('sessions.overview.description')}
              </p>
            </div>
          </div>
        </div>
        {#if pageData}
          <p class="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <Clock3Icon class="size-3.5" aria-hidden="true" />
            {i18nManager.t('sessions.overview.indexed_at', {
              time: formatTime(pageData.generatedAtMs),
            })}
          </p>
        {/if}
      </div>

      <dl class="mt-4 grid grid-cols-2 gap-px overflow-hidden rounded-xl border bg-border lg:grid-cols-4">
        <div class="bg-card px-4 py-3.5">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <BoxesIcon class="size-3.5 text-primary" aria-hidden="true" />
            {i18nManager.t('sessions.overview.results')}
          </dt>
          {#if pageData}
            <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
              {formatNumber(pageData.total)}
            </dd>
          {:else}
            <Skeleton class="mt-2 h-6 w-16" />
          {/if}
        </div>
        <div class="bg-card px-4 py-3.5">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <WorkflowIcon class="size-3.5 text-primary" aria-hidden="true" />
            {i18nManager.t('sessions.overview.runs')}
          </dt>
          {#if pageData}
            <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
              {formatNumber(pageRuns)}
            </dd>
          {:else}
            <Skeleton class="mt-2 h-6 w-14" />
          {/if}
        </div>
        <div class="bg-card px-4 py-3.5">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <ActivityIcon class="size-3.5 text-primary" aria-hidden="true" />
            {i18nManager.t('sessions.overview.events')}
          </dt>
          {#if pageData}
            <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
              {formatNumber(pageEvents)}
            </dd>
          {:else}
            <Skeleton class="mt-2 h-6 w-20" />
          {/if}
        </div>
        <div class="bg-card px-4 py-3.5">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <span class="font-mono text-xs font-semibold text-primary">T</span>
            {i18nManager.t('sessions.overview.tokens')}
          </dt>
          {#if pageData}
            <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
              {formatNumber(pageTokens)}
            </dd>
          {:else}
            <Skeleton class="mt-2 h-6 w-24" />
          {/if}
        </div>
      </dl>
    </section>

    <div class="flex flex-col gap-3 border-b bg-card px-5 py-3.5 lg:flex-row lg:items-center lg:justify-between">
      <form class="relative w-full max-w-lg" role="search" onsubmit={submitSearch}>
        <SearchIcon
          class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          bind:value={queryInput}
          class="h-9 bg-muted/20 pl-9 shadow-none"
          aria-label={i18nManager.t('sessions.search')}
          placeholder={i18nManager.t('sessions.search_placeholder')}
        />
        <Button type="submit" class="sr-only">{i18nManager.t('sessions.search_action')}</Button>
      </form>

      <ToggleGroup.Root
        type="single"
        bind:value={
          () => archiveFilter,
          (value) => {
            if (value) setArchiveFilter(value as ArchiveFilter);
          }
        }
        variant="outline"
        size="sm"
        class="bg-muted/30 p-0.5"
        aria-label={i18nManager.t('sessions.filter.label')}
      >
        {#each ['all', 'current', 'archived'] as value (value)}
          {@const filter = value as ArchiveFilter}
          <ToggleGroup.Item
            value={filter}
            class="h-7 shadow-none"
            data-testid={`session-filter-${filter}`}
          >
            {i18nManager.t(`sessions.filter.${filter}`)}
          </ToggleGroup.Item>
        {/each}
      </ToggleGroup.Root>
    </div>

    {#if error}
      <div class="flex flex-1 items-center justify-center p-6">
        <Alert.Root variant="destructive" class="max-w-xl">
          <CircleAlertIcon aria-hidden="true" />
          <Alert.Title>{i18nManager.t('sessions.load_error')}</Alert.Title>
          <Alert.Description>{error}</Alert.Description>
          <Alert.Action>
            <Button variant="outline" size="sm" onclick={refreshPageImmediately}>
              {i18nManager.t('sessions.retry')}
            </Button>
          </Alert.Action>
        </Alert.Root>
      </div>
    {:else if loading && !pageData}
      <div class="flex-1" role="status">
        <span class="sr-only">{i18nManager.t('sessions.loading')}</span>
        <div class="grid grid-cols-[minmax(18rem,2fr)_1fr_1fr_0.7fr_0.6fr] gap-5 border-b px-5 py-3">
          {#each ['a', 'b', 'c', 'd', 'e'] as item (item)}
            <Skeleton class="h-3 w-20" />
          {/each}
        </div>
        {#each ['a', 'b', 'c', 'd', 'e', 'f'] as item (item)}
          <div class="flex items-center gap-4 border-b px-5 py-4">
            <Skeleton class="size-9 shrink-0 rounded-lg" />
            <div class="min-w-0 flex-1">
              <Skeleton class="h-4 w-2/5" />
              <Skeleton class="mt-2 h-3 w-1/4" />
            </div>
            <Skeleton class="hidden h-5 w-28 sm:block" />
            <Skeleton class="hidden h-5 w-20 lg:block" />
          </div>
        {/each}
      </div>
    {:else if pageData?.items.length === 0}
      <Empty.Root class="min-h-[24rem]">
        <Empty.Header>
          <Empty.Media variant="icon">
          {#if archiveFilter === 'archived'}
              <ArchiveIcon strokeWidth={1.6} aria-hidden="true" />
          {:else}
              <InboxIcon strokeWidth={1.6} aria-hidden="true" />
          {/if}
          </Empty.Media>
          <Empty.Title class="text-base">{i18nManager.t('sessions.empty')}</Empty.Title>
          <Empty.Description>
          {appliedQuery
            ? i18nManager.t('sessions.empty_filtered')
            : i18nManager.t('sessions.empty_desc')}
          </Empty.Description>
        </Empty.Header>
      </Empty.Root>
    {:else}
      <div class="relative min-h-0 flex-1">
        {#if loading}
          <Skeleton class="absolute inset-x-0 top-0 z-20 h-0.5 rounded-none" />
        {/if}
        <Table class="min-w-[60rem]">
          <TableHeader class="sticky top-0 z-10 bg-muted/40 backdrop-blur-sm">
            <TableRow class="hover:bg-transparent">
              <TableHead class="h-10 pl-5 text-[11px] uppercase tracking-wide">
                {i18nManager.t('sessions.column.session')}
              </TableHead>
              <TableHead class="h-10 text-[11px] uppercase tracking-wide">
                {i18nManager.t('sessions.column.model')}
              </TableHead>
              <TableHead class="h-10 text-[11px] uppercase tracking-wide">
                {i18nManager.t('sessions.column.activity')}
              </TableHead>
              <TableHead class="h-10 text-[11px] uppercase tracking-wide">
                {i18nManager.t('sessions.column.updated')}
              </TableHead>
              <TableHead class="h-10 text-right text-[11px] uppercase tracking-wide">
                {i18nManager.t('sessions.column.tokens')}
              </TableHead>
              <TableHead class="h-10 pr-5 text-right text-[11px] uppercase tracking-wide">
                {i18nManager.t('sessions.column.status')}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {#each pageData?.items ?? [] as session (session.id)}
              <TableRow
                class="group cursor-pointer border-b-border/60 transition-colors duration-150 focus-visible:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50 motion-reduce:transition-none"
                tabindex={0}
                onclick={() => void openSession(session)}
                onkeydown={(event) => handleRowKeydown(event, session)}
              >
                <TableCell class="max-w-[28rem] border-l-2 border-l-transparent py-3.5 pl-4 group-hover:border-l-primary">
                  <div class="flex min-w-0 items-center gap-3">
                    <span class="flex size-9 shrink-0 items-center justify-center rounded-lg border bg-muted/30 text-xs font-semibold text-foreground shadow-xs">
                      {projectInitial(session)}
                    </span>
                    <div class="min-w-0">
                      <p class="truncate text-sm font-medium">{displayTitle(session)}</p>
                      <div class="mt-1 flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
                        <span class="truncate">
                          {session.projectName || i18nManager.t('sessions.unknown_project')}
                        </span>
                        {#if session.gitBranch}
                          <span class="text-border">/</span>
                          <span class="inline-flex min-w-0 items-center gap-1 truncate font-mono">
                            <GitBranchIcon class="size-3 shrink-0" aria-hidden="true" />
                            {session.gitBranch}
                          </span>
                        {/if}
                      </div>
                    </div>
                  </div>
                </TableCell>
                <TableCell class="max-w-52">
                  <div class="min-w-0">
                    <code class="block truncate text-xs font-medium">{session.model ?? '—'}</code>
                    <p class="mt-1 truncate text-[11px] text-muted-foreground">
                      {session.modelProvider ?? session.provider}
                    </p>
                  </div>
                </TableCell>
                <TableCell>
                  <div class="flex items-center gap-3 text-xs">
                    <span class="inline-flex items-center gap-1.5">
                      <WorkflowIcon class="size-3.5 text-muted-foreground" aria-hidden="true" />
                      <strong class="font-medium tabular-nums">{formatNumber(session.invocationCount)}</strong>
                      <span class="text-muted-foreground">{i18nManager.t('sessions.activity.runs')}</span>
                    </span>
                    <span class="inline-flex items-center gap-1.5">
                      <ActivityIcon class="size-3.5 text-muted-foreground" aria-hidden="true" />
                      <strong class="font-medium tabular-nums">{formatNumber(session.eventCount)}</strong>
                      <span class="text-muted-foreground">{i18nManager.t('sessions.activity.events')}</span>
                    </span>
                  </div>
                  {#if session.skillInvocationCount > 0}
                    <p class="mt-1.5 text-[11px] text-muted-foreground">
                      {i18nManager.t('sessions.activity.skills', {
                        count: session.skillInvocationCount,
                      })}
                    </p>
                  {/if}
                </TableCell>
                <TableCell>
                  <time class="text-xs text-muted-foreground">{formatDate(session.updatedAtMs)}</time>
                </TableCell>
                <TableCell class="text-right font-medium tabular-nums">
                  {formatNumber(session.tokensUsed)}
                </TableCell>
                <TableCell class="pr-5 text-right">
                  <Badge variant={statusVariant(session.status)} class="gap-1.5">
                    <span class={['size-1.5 rounded-full', statusDotClass(session.status)].join(' ')}></span>
                    {statusLabel(session.status)}
                  </Badge>
                </TableCell>
              </TableRow>
            {/each}
          </TableBody>
        </Table>
      </div>

      <DataPagination
        idPrefix="sessions"
        {page}
        {pageSize}
        totalCount={pageData?.total ?? 0}
        totalLabel={i18nManager.t('sessions.pagination.results', {
          count: pageData?.total ?? 0,
        })}
        {loading}
        onPageChange={goToPage}
        onPageSizeChange={setPageSize}
      />
    {/if}
  </div>
{/if}
