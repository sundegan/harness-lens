<script lang="ts">
import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
import SearchIcon from '@lucide/svelte/icons/search';
import SparklesIcon from '@lucide/svelte/icons/sparkles';
import { onMount } from 'svelte';
import { getSkillAnalysis, type SkillAnalysisData, type SkillSummary } from '$lib/analytics';
import DataPagination from '$lib/components/data-pagination/DataPagination.svelte';
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
import { i18nManager } from '$lib/i18n.svelte';
import { logWarn } from '$lib/logger';
import { createTrailingRefresh } from '$lib/trailing-refresh';

let { active = true }: { active?: boolean } = $props();

let analysis = $state.raw<SkillAnalysisData | null>(null);
let loading = $state(true);
let error = $state('');
let query = $state('');
let page = $state(1);
let pageSize = $state(50);
let requestSequence = 0;
let refreshWhenActive = $state(false);
const skillRefresh = createTrailingRefresh(loadSkills);

const normalizedQuery = $derived(query.trim().toLocaleLowerCase());
const filteredSkills = $derived.by(() => {
  const skills = analysis?.skills ?? [];
  if (!normalizedQuery) return skills;
  return skills.filter((skill) => skill.name.toLocaleLowerCase().includes(normalizedQuery));
});
const pageCount = $derived(Math.max(1, Math.ceil(filteredSkills.length / pageSize)));
const visibleSkills = $derived(filteredSkills.slice((page - 1) * pageSize, page * pageSize));

const totalInvocations = $derived(
  (analysis?.skills ?? []).reduce((total, skill) => total + skill.invocationCount, 0)
);
const successfulInvocations = $derived(
  (analysis?.skills ?? []).reduce((total, skill) => total + skill.succeededCount, 0)
);
const overallSuccessRate = $derived(
  totalInvocations === 0 ? null : successfulInvocations / totalInvocations
);

async function loadSkills() {
  const sequence = ++requestSequence;
  loading = true;
  error = '';
  try {
    const result = await getSkillAnalysis();
    if (sequence !== requestSequence) return;
    analysis = result;
    const matchingSkillCount = normalizedQuery
      ? result.skills.filter((skill) => skill.name.toLocaleLowerCase().includes(normalizedQuery))
          .length
      : result.skills.length;
    page = Math.min(page, Math.max(1, Math.ceil(matchingSkillCount / pageSize)));
  } catch (cause) {
    if (sequence !== requestSequence) return;
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (sequence === requestSequence) loading = false;
  }
}

onMount(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;

  skillRefresh.request(true);
  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { listen } = await import('@tauri-apps/api/event');
    const stopListening = await listen('analytics-updated', () => {
      if (disposed) return;
      if (active) skillRefresh.request();
      else refreshWhenActive = true;
    });
    if (disposed) stopListening();
    else unlisten = stopListening;
  })().catch((cause) => logWarn('Failed to listen for analytics updates', cause));

  return () => {
    disposed = true;
    requestSequence += 1;
    skillRefresh.cancel();
    unlisten?.();
  };
});

export function activate() {
  if (refreshWhenActive) {
    refreshWhenActive = false;
    skillRefresh.request();
  }
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value);
}

function updateQuery(event: Event) {
  query = (event.currentTarget as HTMLInputElement).value;
  page = 1;
}

function goToPage(value: number) {
  if (value < 1 || value > pageCount || value === page) return;
  page = value;
}

function setPageSize(value: number) {
  if (value === pageSize) return;
  pageSize = value;
  page = 1;
}

function formatPercent(value: number | null): string {
  if (value === null) return '—';
  return new Intl.NumberFormat(undefined, {
    style: 'percent',
    maximumFractionDigits: 1,
  }).format(value);
}

function formatDuration(value: number | null): string {
  if (value === null) return '—';
  const seconds = Math.max(value, 0) / 1000;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)}s`;
  const minutes = seconds / 60;
  return `${minutes.toFixed(minutes < 10 ? 1 : 0)}m`;
}

function successVariant(skill: SkillSummary): 'success' | 'warning' | 'outline' {
  if (skill.successRate === null) return 'outline';
  if (skill.successRate >= 0.8) return 'success';
  if (skill.successRate >= 0.5) return 'warning';
  return 'outline';
}
</script>

<div
  class="flex min-w-0 flex-1 flex-col"
  data-testid="skill-analysis"
  aria-busy={loading}
>
  <section class="border-b bg-muted/15 px-5 py-5 sm:px-6" aria-labelledby="skill-overview-title">
    <div class="flex items-center gap-2">
      <span class="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <SparklesIcon aria-hidden="true" />
      </span>
      <div>
        <h2 id="skill-overview-title" class="text-sm font-semibold">
          {i18nManager.t('main.nav.skills')}
        </h2>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {i18nManager.t('main.nav.skills_desc')}
        </p>
      </div>
    </div>

    <dl class="mt-4 grid grid-cols-1 gap-px overflow-hidden rounded-xl border bg-border sm:grid-cols-3">
      <div class="bg-card px-4 py-3.5">
        <dt class="text-[11px] font-medium text-muted-foreground">
          {i18nManager.t('dashboard.summary.skills')}
        </dt>
        {#if analysis}
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatNumber(analysis.skills.length)}
          </dd>
        {:else}
          <Skeleton class="mt-2 h-6 w-14" />
        {/if}
      </div>
      <div class="bg-card px-4 py-3.5">
        <dt class="text-[11px] font-medium text-muted-foreground">
          {i18nManager.t('dashboard.skill.invocations')}
        </dt>
        {#if analysis}
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatNumber(totalInvocations)}
          </dd>
        {:else}
          <Skeleton class="mt-2 h-6 w-16" />
        {/if}
      </div>
      <div class="bg-card px-4 py-3.5">
        <dt class="text-[11px] font-medium text-muted-foreground">
          {i18nManager.t('dashboard.skill.success_rate')}
        </dt>
        {#if analysis}
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatPercent(overallSuccessRate)}
          </dd>
        {:else}
          <Skeleton class="mt-2 h-6 w-12" />
        {/if}
      </div>
    </dl>
  </section>

  <div class="border-b bg-card px-5 py-3.5">
    <div class="relative w-full max-w-lg">
      <SearchIcon
        class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground"
        aria-hidden="true"
      />
      <Input
        value={query}
        oninput={updateQuery}
        class="h-9 bg-muted/20 pl-9 shadow-none"
        aria-label={i18nManager.t('dashboard.search.label')}
        placeholder={i18nManager.t('dashboard.search.skills')}
      />
    </div>
  </div>

  {#if error}
    <div class="flex flex-1 items-center justify-center p-6">
      <Alert.Root variant="destructive" class="max-w-xl">
        <CircleAlertIcon aria-hidden="true" />
        <Alert.Title>{i18nManager.t('dashboard.sync.error')}</Alert.Title>
        <Alert.Description>{error}</Alert.Description>
        <Alert.Action>
          <Button variant="outline" size="sm" onclick={() => skillRefresh.request(true)}>
            {i18nManager.t('sessions.retry')}
          </Button>
        </Alert.Action>
      </Alert.Root>
    </div>
  {:else if loading && !analysis}
    <div class="flex-1" role="status">
      <span class="sr-only">{i18nManager.t('dashboard.loading')}</span>
      {#each ['a', 'b', 'c', 'd', 'e'] as item (item)}
        <div class="flex items-center gap-4 border-b px-5 py-4">
          <Skeleton class="h-4 w-1/4" />
          <Skeleton class="ml-auto h-4 w-16" />
          <Skeleton class="h-5 w-20" />
        </div>
      {/each}
    </div>
  {:else if filteredSkills.length === 0}
    <Empty.Root class="min-h-[24rem]">
      <Empty.Header>
        <Empty.Media variant="icon"><SparklesIcon aria-hidden="true" /></Empty.Media>
        <Empty.Title>{i18nManager.t('dashboard.empty.skills')}</Empty.Title>
        {#if normalizedQuery}
          <Empty.Description>{i18nManager.t('dashboard.empty.filtered')}</Empty.Description>
        {/if}
      </Empty.Header>
    </Empty.Root>
  {:else}
    <div class="relative min-h-0 flex-1 overflow-auto">
      {#if loading}
        <Skeleton class="sticky top-0 z-20 h-0.5 rounded-none" />
      {/if}
      <Table class="min-w-[56rem]">
        <TableHeader class="sticky top-0 z-10 bg-muted/40 backdrop-blur-sm">
          <TableRow class="hover:bg-transparent">
            <TableHead class="h-10 pl-5">{i18nManager.t('dashboard.skill.name')}</TableHead>
            <TableHead class="h-10 text-right">
              {i18nManager.t('dashboard.skill.invocations')}
            </TableHead>
            <TableHead class="h-10">{i18nManager.t('dashboard.skill.outcomes')}</TableHead>
            <TableHead class="h-10 text-right">
              {i18nManager.t('dashboard.skill.average_duration')}
            </TableHead>
            <TableHead class="h-10 pr-5 text-right">
              {i18nManager.t('dashboard.skill.average_tokens')}
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {#each visibleSkills as skill (skill.name)}
            <TableRow>
              <TableCell class="py-3.5 pl-5 font-medium">{skill.name}</TableCell>
              <TableCell class="text-right tabular-nums">
                {formatNumber(skill.invocationCount)}
              </TableCell>
              <TableCell>
                <div class="flex items-center gap-2">
                  <Badge variant={successVariant(skill)}>{formatPercent(skill.successRate)}</Badge>
                  <span class="text-[11px] text-muted-foreground">
                    {i18nManager.t('dashboard.skill.outcome_values', {
                      succeeded: skill.succeededCount,
                      failed: skill.failedCount,
                      cancelled: skill.cancelledCount,
                      unknown: skill.unknownCount,
                    })}
                  </span>
                </div>
              </TableCell>
              <TableCell class="text-right tabular-nums">
                {formatDuration(skill.averageDurationMs)}
              </TableCell>
              <TableCell class="pr-5 text-right tabular-nums">
                {skill.averageTokens === null ? '—' : formatNumber(skill.averageTokens)}
              </TableCell>
            </TableRow>
          {/each}
        </TableBody>
      </Table>
    </div>
    <DataPagination
      idPrefix="skills"
      {page}
      {pageSize}
      totalCount={filteredSkills.length}
      totalLabel={i18nManager.t('dashboard.pagination.results', {
        count: filteredSkills.length,
      })}
      {loading}
      onPageChange={goToPage}
      onPageSizeChange={setPageSize}
    />
  {/if}
</div>
