<script lang="ts">
import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
import SearchIcon from '@lucide/svelte/icons/search';
import SparklesIcon from '@lucide/svelte/icons/sparkles';
import { onMount } from 'svelte';
import { type AnalyticsSnapshot, getAnalyticsSnapshot, type SkillSummary } from '$lib/analytics';
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

let snapshot = $state.raw<AnalyticsSnapshot | null>(null);
let loading = $state(true);
let error = $state('');
let query = $state('');
let requestSequence = 0;

const normalizedQuery = $derived(query.trim().toLocaleLowerCase());
const visibleSkills = $derived.by(() => {
  const skills = snapshot?.skills ?? [];
  if (!normalizedQuery) return skills;
  return skills.filter((skill) => skill.name.toLocaleLowerCase().includes(normalizedQuery));
});

const totalInvocations = $derived(
  (snapshot?.skills ?? []).reduce((total, skill) => total + skill.invocationCount, 0)
);
const successfulInvocations = $derived(
  (snapshot?.skills ?? []).reduce((total, skill) => total + skill.succeededCount, 0)
);
const overallSuccessRate = $derived(
  totalInvocations === 0 ? null : successfulInvocations / totalInvocations
);

async function loadSkills() {
  const sequence = ++requestSequence;
  loading = snapshot === null;
  error = '';
  try {
    const result = await getAnalyticsSnapshot();
    if (sequence !== requestSequence) return;
    snapshot = result;
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

  void loadSkills();
  void (async () => {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;
    const { listen } = await import('@tauri-apps/api/event');
    const stopListening = await listen('analytics-updated', () => {
      if (!disposed) void loadSkills();
    });
    if (disposed) stopListening();
    else unlisten = stopListening;
  })().catch((cause) => logWarn('Failed to listen for analytics updates', cause));

  return () => {
    disposed = true;
    requestSequence += 1;
    unlisten?.();
  };
});

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(value);
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

<div class="flex min-w-0 flex-1 flex-col" data-testid="skill-analysis">
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
        <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
          {formatNumber(snapshot?.skills.length ?? 0)}
        </dd>
      </div>
      <div class="bg-card px-4 py-3.5">
        <dt class="text-[11px] font-medium text-muted-foreground">
          {i18nManager.t('dashboard.skill.invocations')}
        </dt>
        <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
          {formatNumber(totalInvocations)}
        </dd>
      </div>
      <div class="bg-card px-4 py-3.5">
        <dt class="text-[11px] font-medium text-muted-foreground">
          {i18nManager.t('dashboard.skill.success_rate')}
        </dt>
        <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
          {formatPercent(overallSuccessRate)}
        </dd>
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
        bind:value={query}
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
          <Button variant="outline" size="sm" onclick={() => void loadSkills()}>
            {i18nManager.t('sessions.retry')}
          </Button>
        </Alert.Action>
      </Alert.Root>
    </div>
  {:else if loading}
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
  {:else if visibleSkills.length === 0}
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
    <div class="min-h-0 flex-1 overflow-auto">
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
  {/if}
</div>
