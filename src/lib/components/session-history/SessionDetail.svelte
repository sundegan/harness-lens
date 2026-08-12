<script lang="ts">
import ActivityIcon from '@lucide/svelte/icons/activity';
import ArrowLeftIcon from '@lucide/svelte/icons/arrow-left';
import BotIcon from '@lucide/svelte/icons/bot';
import BoxesIcon from '@lucide/svelte/icons/boxes';
import BrainIcon from '@lucide/svelte/icons/brain';
import CheckCircle2Icon from '@lucide/svelte/icons/check-circle-2';
import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
import Clock3Icon from '@lucide/svelte/icons/clock-3';
import Code2Icon from '@lucide/svelte/icons/code-2';
import GitBranchIcon from '@lucide/svelte/icons/git-branch';
import Layers3Icon from '@lucide/svelte/icons/layers-3';
import MessageSquareIcon from '@lucide/svelte/icons/message-square';
import TerminalSquareIcon from '@lucide/svelte/icons/square-terminal';
import UserIcon from '@lucide/svelte/icons/user';
import WorkflowIcon from '@lucide/svelte/icons/workflow';
import { SvelteMap } from 'svelte/reactivity';
import { Badge } from '$lib/components/ui/badge';
import { Button } from '$lib/components/ui/button';
import { Card, CardContent } from '$lib/components/ui/card';
import * as Collapsible from '$lib/components/ui/collapsible';
import { i18nManager } from '$lib/i18n.svelte';
import type {
  ContentBlock,
  MessageEventValue,
  NormalizedEventData,
  ReasoningEventValue,
  SessionDetail,
  SessionEventItem,
  ToolCallEventValue,
  ToolResultEventValue,
} from '$lib/session-history';

type ToolTimelineItem = {
  kind: 'tool';
  event: SessionEventItem;
  call: ToolCallEventValue;
  result: SessionEventItem | null;
  resultValue: ToolResultEventValue | null;
};

type MessageTimelineItem = {
  kind: 'message';
  event: SessionEventItem;
  value: MessageEventValue;
};

type ReasoningTimelineItem = {
  kind: 'reasoning';
  event: SessionEventItem;
  value: ReasoningEventValue;
};

type TimelineItem = ToolTimelineItem | MessageTimelineItem | ReasoningTimelineItem;

type InvocationGroup = {
  id: string | null;
  items: TimelineItem[];
  technicalEvents: SessionEventItem[];
};

let { detail, onback }: { detail: SessionDetail; onback: () => void } = $props();

const title = $derived(detail.session.title.trim() || i18nManager.t('sessions.untitled'));
const groups = $derived(buildInvocationGroups(detail.events));

function eventValue<T>(data: NormalizedEventData): T | null {
  return data.value && typeof data.value === 'object' ? (data.value as T) : null;
}

function buildInvocationGroups(events: SessionEventItem[]): InvocationGroup[] {
  const groupMap = new SvelteMap<string, { id: string | null; events: SessionEventItem[] }>();
  const groupOrder: string[] = [];

  for (const event of events) {
    const key = event.invocationId ?? '__session__';
    if (!groupMap.has(key)) {
      groupMap.set(key, { id: event.invocationId, events: [] });
      groupOrder.push(key);
    }
    groupMap.get(key)?.events.push(event);
  }

  return groupOrder.map((key) => {
    const group = groupMap.get(key);
    if (!group) return { id: null, items: [], technicalEvents: [] };

    const finalResults = new SvelteMap<string, SessionEventItem>();
    const lastToolCallIds = new SvelteMap<string, string>();
    for (const event of group.events) {
      if (event.event.data.type === 'tool_call') {
        const value = eventValue<ToolCallEventValue>(event.event.data);
        if (value?.call_id) lastToolCallIds.set(value.call_id, event.id);
      } else if (event.event.data.type === 'tool_result') {
        const value = eventValue<ToolResultEventValue>(event.event.data);
        if (value?.call_id) finalResults.set(value.call_id, event);
      }
    }

    const pairedResultIds = new Set(
      [...finalResults.entries()]
        .filter(([callId]) => lastToolCallIds.has(callId))
        .map(([, event]) => event.id)
    );
    const items: TimelineItem[] = [];
    const technicalEvents: SessionEventItem[] = [];

    for (const event of group.events) {
      const { data } = event.event;
      if (data.type === 'message') {
        const value = eventValue<MessageEventValue>(data);
        if (value) items.push({ kind: 'message', event, value });
      } else if (data.type === 'reasoning') {
        const value = eventValue<ReasoningEventValue>(data);
        if (value) items.push({ kind: 'reasoning', event, value });
      } else if (data.type === 'tool_call') {
        const call = eventValue<ToolCallEventValue>(data);
        if (!call) {
          technicalEvents.push(event);
          continue;
        }
        const result =
          lastToolCallIds.get(call.call_id) === event.id
            ? (finalResults.get(call.call_id) ?? null)
            : null;
        items.push({
          kind: 'tool',
          event,
          call,
          result,
          resultValue: result ? eventValue<ToolResultEventValue>(result.event.data) : null,
        });
      } else if (!pairedResultIds.has(event.id)) {
        technicalEvents.push(event);
      }
    }

    return { id: group.id, items, technicalEvents };
  });
}

function textFromContent(content: ContentBlock[] | undefined): string {
  if (!content) return '';
  return content
    .map((block) => {
      if (block.type === 'text') return block.text;
      if (block.type === 'resource') return block.text ?? block.uri;
      if (block.type === 'resource_link') return block.title ?? block.name ?? block.uri;
      if (block.type === 'image') return i18nManager.t('sessions.detail.image');
      if (block.type === 'audio') return i18nManager.t('sessions.detail.audio');
      return i18nManager.t('sessions.detail.attachment');
    })
    .filter(Boolean)
    .join('\n');
}

function roleLabel(role: string): string {
  if (role === 'user') return i18nManager.t('sessions.detail.role.user');
  if (role === 'assistant') return i18nManager.t('sessions.detail.role.assistant');
  if (role === 'developer') return i18nManager.t('sessions.detail.role.developer');
  if (role === 'system') return i18nManager.t('sessions.detail.role.system');
  if (role === 'tool') return i18nManager.t('sessions.detail.role.tool');
  return role || i18nManager.t('sessions.detail.role.unknown');
}

function isPrimaryMessage(role: string, content: ContentBlock[] | undefined): boolean {
  return (
    (role === 'user' || role === 'assistant') && textFromContent(content).trim().length <= 1600
  );
}

function messagePreview(content: ContentBlock[] | undefined): string {
  const text = textFromContent(content).replace(/\s+/g, ' ').trim();
  if (!text) return i18nManager.t('sessions.detail.empty_message');
  return text.length > 120 ? `${text.slice(0, 120)}…` : text;
}

function statusLabel(status: string): string {
  const key = `sessions.status.${status}`;
  const translated = i18nManager.t(key);
  return translated === key ? status.replaceAll('_', ' ') : translated;
}

function eventTypeLabel(type: string): string {
  const key = `sessions.event.${type}`;
  const translated = i18nManager.t(key);
  return translated === key ? type.replaceAll('_', ' ') : translated;
}

function formatDate(value: number | null): string {
  if (value === null) return i18nManager.t('sessions.unknown');
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  }).format(new Date(value));
}

function formatTime(value: number | null): string {
  if (value === null) return '';
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(value));
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { notation: 'compact' }).format(value);
}

function formatDuration(value: number | null | undefined): string {
  if (value === null || value === undefined) return '';
  if (value < 1000) return `${value} ms`;
  return `${(value / 1000).toFixed(value < 10000 ? 1 : 0)} s`;
}

function formatJson(value: unknown): string {
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function toolStatus(result: ToolResultEventValue | null, call: ToolCallEventValue): string {
  return result?.status ?? call.status;
}

function statusVariant(status: string): 'success' | 'warning' | 'outline' | 'destructive' {
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

<div class="flex w-full flex-col bg-workspace">
  <div class="border-b bg-card px-5 py-3 sm:px-6">
    <div class="mx-auto w-full max-w-6xl">
      <Button variant="ghost" size="sm" class="-ml-2 text-muted-foreground" onclick={onback}>
        <ArrowLeftIcon data-icon="inline-start" />
        {i18nManager.t('sessions.detail.back')}
      </Button>
    </div>
  </div>

  <div class="mx-auto flex w-full max-w-6xl flex-col gap-6 p-5 pb-10 sm:p-6 sm:pb-12">
    <header class="overflow-hidden rounded-2xl border border-t-2 border-t-primary bg-card shadow-xs">
      <div class="flex flex-col justify-between gap-5 px-5 pt-5 pb-4 sm:flex-row sm:items-start sm:px-6 sm:pt-6">
        <div class="min-w-0">
          <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
            <span class="truncate font-medium">
              {detail.session.projectName || i18nManager.t('sessions.unknown_project')}
            </span>
            <span>/</span>
            <span>{i18nManager.t('sessions.detail.session_label')}</span>
          </div>
          <h2 class="mt-2 break-words text-2xl font-semibold tracking-tight">{title}</h2>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <Badge variant="secondary" class="font-mono">{detail.session.model ?? '—'}</Badge>
            <Badge variant="outline">{detail.session.modelProvider ?? detail.session.provider}</Badge>
            {#if detail.session.gitBranch}
              <Badge variant="outline" class="gap-1.5 font-mono font-normal">
                <GitBranchIcon class="size-3" aria-hidden="true" />
                {detail.session.gitBranch}
              </Badge>
            {/if}
          </div>
        </div>
        <Badge variant={statusVariant(detail.session.status)} class="mt-0.5 gap-1.5 px-2.5 py-1">
          <span
            class={[
              'size-1.5 rounded-full',
              detail.session.status === 'succeeded' || detail.session.status === 'completed'
                ? 'bg-success'
                : detail.session.status === 'failed' || detail.session.status === 'cancelled'
                  ? 'bg-destructive'
                  : 'bg-warning',
            ].join(' ')}
          ></span>
          {statusLabel(detail.session.status)}
        </Badge>
      </div>

      <dl class="grid grid-cols-2 gap-px border-y bg-border lg:grid-cols-4">
        <div class="bg-card px-5 py-4">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <WorkflowIcon class="size-3.5 text-primary" aria-hidden="true" />
            {i18nManager.t('sessions.column.runs')}
          </dt>
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatNumber(detail.session.invocationCount)}
          </dd>
        </div>
        <div class="bg-card px-5 py-4">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <ActivityIcon class="size-3.5 text-primary" aria-hidden="true" />
            {i18nManager.t('sessions.detail.events')}
          </dt>
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatNumber(detail.session.eventCount)}
          </dd>
        </div>
        <div class="bg-card px-5 py-4">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <BoxesIcon class="size-3.5 text-primary" aria-hidden="true" />
            {i18nManager.t('sessions.detail.skills')}
          </dt>
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatNumber(detail.session.skillInvocationCount)}
          </dd>
        </div>
        <div class="bg-card px-5 py-4">
          <dt class="flex items-center gap-2 text-[11px] font-medium text-muted-foreground">
            <span class="font-mono text-xs font-semibold text-primary">T</span>
            {i18nManager.t('sessions.column.tokens')}
          </dt>
          <dd class="mt-1.5 text-xl font-semibold tracking-tight tabular-nums">
            {formatNumber(detail.session.tokensUsed)}
          </dd>
        </div>
      </dl>

      <div class="flex min-w-0 flex-col gap-2 bg-muted/20 px-5 py-3 text-[11px] text-muted-foreground sm:flex-row sm:items-center sm:justify-between sm:px-6">
        {#if detail.session.cwd}
          <span class="min-w-0 truncate font-mono">{detail.session.cwd}</span>
        {:else}
          <span>{i18nManager.t('sessions.detail.provider')}: {detail.session.provider}</span>
        {/if}
        <span class="shrink-0">
          {i18nManager.t('sessions.detail.updated')}: {formatDate(detail.session.updatedAtMs)}
        </span>
      </div>
    </header>

    {#if detail.events.length === 0}
      <Card class="py-12 shadow-none">
        <CardContent class="flex flex-col items-center text-center">
          <MessageSquareIcon class="size-7 text-muted-foreground" strokeWidth={1.5} />
          <p class="mt-3 text-sm font-medium">{i18nManager.t('sessions.detail.no_events')}</p>
          <p class="mt-1 max-w-sm text-xs leading-5 text-muted-foreground">
            {i18nManager.t('sessions.detail.no_events_desc')}
          </p>
        </CardContent>
      </Card>
    {:else}
      <section aria-labelledby="execution-record-title">
        <div class="mb-4 flex items-end justify-between gap-4">
          <div class="flex items-center gap-3">
            <span class="flex size-9 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Layers3Icon class="size-4" strokeWidth={1.8} aria-hidden="true" />
            </span>
            <div>
              <h2 id="execution-record-title" class="text-sm font-semibold">
                {i18nManager.t('sessions.detail.timeline_title')}
              </h2>
              <p class="mt-0.5 text-xs text-muted-foreground">
                {i18nManager.t('sessions.detail.timeline_description')}
              </p>
            </div>
          </div>
          <Badge variant="outline">
            {i18nManager.t('sessions.detail.run_count', { count: groups.length })}
          </Badge>
        </div>

        <div class="flex flex-col gap-5">
      {#each groups as group, groupIndex (`${group.id ?? 'session'}-${groupIndex}`)}
          <section
            class="overflow-hidden rounded-xl border bg-card shadow-xs"
            aria-labelledby={`invocation-${groupIndex}`}
          >
            <div class="flex items-center justify-between gap-4 border-b bg-muted/25 px-4 py-3 sm:px-5">
              <div class="flex min-w-0 items-center gap-3">
                <span class="flex size-7 items-center justify-center rounded-md border bg-card font-mono text-[10px] font-semibold text-primary">
                  {String(groupIndex + 1).padStart(2, '0')}
                </span>
                <div class="min-w-0">
            <h3
              id={`invocation-${groupIndex}`}
                    class="truncate text-xs font-semibold"
            >
              {group.id
                ? i18nManager.t('sessions.detail.invocation', { count: groupIndex + 1 })
                : i18nManager.t('sessions.detail.session_events')}
            </h3>
                  {#if group.id}
                    <p class="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
                      {group.id}
                    </p>
                  {/if}
                </div>
              </div>
              <span class="shrink-0 text-[11px] text-muted-foreground">
                {i18nManager.t('sessions.detail.group_items', {
                  count: group.items.length + group.technicalEvents.length,
                })}
              </span>
          </div>

            <div class="relative mx-4 my-5 ml-8 flex flex-col gap-3 border-l border-primary/20 pl-8 sm:mx-5 sm:ml-9">
            {#each group.items as item (item.event.id)}
              <article class="relative">
                <span
                  class={[
                    'absolute top-3.5 -left-[2.9rem] flex size-7 items-center justify-center rounded-lg border shadow-xs',
                    item.kind === 'message' && item.value.role === 'user'
                      ? 'border-primary bg-primary text-primary-foreground'
                      : item.kind === 'message'
                        ? 'border-primary/20 bg-card text-primary'
                        : item.kind === 'tool'
                          ? 'border-border bg-muted text-foreground'
                          : 'border-accent/25 bg-accent/10 text-accent',
                  ].join(' ')}
                >
                  {#if item.kind === 'message' && item.value.role === 'user'}
                    <UserIcon class="size-3.5" aria-hidden="true" />
                  {:else if item.kind === 'message'}
                    <BotIcon class="size-3.5" aria-hidden="true" />
                  {:else if item.kind === 'tool'}
                    <TerminalSquareIcon class="size-3.5" aria-hidden="true" />
                  {:else}
                    <BrainIcon class="size-3.5" aria-hidden="true" />
                  {/if}
                </span>

                {#if item.kind === 'message'}
                  {#if isPrimaryMessage(item.value.role, item.value.content)}
                    <div
                      class={[
                        'rounded-xl border px-4 py-3.5 shadow-xs',
                        item.value.role === 'user'
                          ? 'border-primary/20 bg-primary/[0.045]'
                          : 'border-border/80 bg-card',
                      ].join(' ')}
                    >
                      <div class="mb-2 flex items-center justify-between gap-3">
                        <span
                          class={[
                            'text-xs font-semibold',
                            item.value.role === 'user' ? 'text-primary' : 'text-foreground',
                          ].join(' ')}
                        >
                          {roleLabel(item.value.role)}
                        </span>
                        <time class="text-[11px] text-muted-foreground">
                          {formatTime(item.event.timestampMs)}
                        </time>
                      </div>
                      <div class="selectable-text whitespace-pre-wrap break-words text-sm leading-6">
                        {textFromContent(item.value.content) ||
                          i18nManager.t('sessions.detail.empty_message')}
                      </div>
                    </div>
                  {:else}
                    <Collapsible.Root class="group rounded-xl border border-border/70 bg-muted/15">
                      <Collapsible.Trigger
                        class="flex w-full cursor-pointer items-center gap-3 rounded-xl px-4 py-3 text-left hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                      >
                        <div class="min-w-0 flex-1">
                          <div class="flex items-center justify-between gap-3">
                            <span class="text-xs font-medium">{roleLabel(item.value.role)}</span>
                            <time class="text-[11px] text-muted-foreground">
                              {formatTime(item.event.timestampMs)}
                            </time>
                          </div>
                          <p class="mt-1 truncate text-xs text-muted-foreground">
                            {messagePreview(item.value.content)}
                          </p>
                        </div>
                        <ChevronRightIcon
                          class="size-3.5 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-90"
                          aria-hidden="true"
                        />
                      </Collapsible.Trigger>
                      <Collapsible.Content class="border-t px-4 py-3">
                        <div class="selectable-text whitespace-pre-wrap break-words text-sm leading-6">
                          {textFromContent(item.value.content) ||
                            i18nManager.t('sessions.detail.empty_message')}
                        </div>
                      </Collapsible.Content>
                    </Collapsible.Root>
                  {/if}
                {:else if item.kind === 'tool'}
                  {@const currentStatus = toolStatus(item.resultValue, item.call)}
                  <Card class="gap-0 border-primary/15 py-0 shadow-xs">
                    <div class="flex min-w-0 items-center gap-3 px-4 py-3">
                      <span class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                        <Code2Icon class="size-4" aria-hidden="true" />
                      </span>
                      <div class="min-w-0 flex-1">
                        <div class="flex min-w-0 items-center gap-2">
                          <span class="truncate text-sm font-medium">
                            {item.call.title || item.call.name}
                          </span>
                          <Badge variant={statusVariant(currentStatus)}>
                            {statusLabel(currentStatus)}
                          </Badge>
                        </div>
                        <p class="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                          {item.call.namespace
                            ? `${item.call.namespace} / ${item.call.name}`
                            : item.call.name}
                        </p>
                      </div>
                      {#if item.resultValue?.duration_ms}
                        <span class="inline-flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
                          <Clock3Icon class="size-3" aria-hidden="true" />
                          {formatDuration(item.resultValue.duration_ms)}
                        </span>
                      {/if}
                    </div>

                    <Collapsible.Root class="group border-t">
                      <Collapsible.Trigger
                        class="flex w-full cursor-pointer items-center gap-2 px-4 py-2.5 text-left text-xs font-medium text-muted-foreground hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                      >
                        <ChevronRightIcon
                          class="size-3.5 transition-transform group-data-[state=open]:rotate-90"
                          aria-hidden="true"
                        />
                        {i18nManager.t('sessions.detail.tool_details')}
                      </Collapsible.Trigger>
                      <Collapsible.Content class="flex flex-col gap-4 border-t bg-muted/20 p-4">
                        <div>
                          <p class="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                            {i18nManager.t('sessions.detail.input')}
                          </p>
                          <pre class="max-h-72 overflow-auto rounded-lg border bg-background p-3 text-xs leading-5 whitespace-pre-wrap break-words selectable-text">{formatJson(item.call.input)}</pre>
                        </div>
                        {#if item.resultValue}
                          <div>
                            <p class="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                              {i18nManager.t('sessions.detail.output')}
                            </p>
                            {#if item.resultValue.error}
                              <div class="mb-2 flex items-start gap-2 rounded-lg border border-destructive/25 bg-destructive/5 p-3 text-xs text-destructive">
                                <CircleAlertIcon class="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                                <span class="selectable-text">{item.resultValue.error}</span>
                              </div>
                            {/if}
                            <pre class="max-h-80 overflow-auto rounded-lg border bg-background p-3 text-xs leading-5 whitespace-pre-wrap break-words selectable-text">{formatJson(item.resultValue.output)}</pre>
                          </div>
                        {:else}
                          <p class="text-xs text-muted-foreground">
                            {i18nManager.t('sessions.detail.no_tool_result')}
                          </p>
                        {/if}
                      </Collapsible.Content>
                    </Collapsible.Root>
                  </Card>
                {:else}
                  <Collapsible.Root class="group rounded-xl border border-accent/20 bg-accent/[0.035]">
                    <Collapsible.Trigger
                      class="flex w-full cursor-pointer items-center gap-3 rounded-xl px-4 py-3 text-left hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                    >
                      <BrainIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                      <div class="min-w-0 flex-1">
                        <p class="text-sm font-medium">{i18nManager.t('sessions.detail.reasoning')}</p>
                        {#if item.value.summary?.length}
                          <p class="mt-0.5 truncate text-xs text-muted-foreground">
                            {item.value.summary.join(' · ')}
                          </p>
                        {/if}
                      </div>
                      <ChevronRightIcon
                        class="size-3.5 text-muted-foreground transition-transform group-data-[state=open]:rotate-90"
                        aria-hidden="true"
                      />
                    </Collapsible.Trigger>
                    <Collapsible.Content class="border-t px-4 py-3">
                      <div class="selectable-text whitespace-pre-wrap text-sm leading-6">
                        {textFromContent(item.value.content) ||
                          item.value.summary?.join('\n') ||
                          i18nManager.t('sessions.detail.no_reasoning')}
                      </div>
                    </Collapsible.Content>
                  </Collapsible.Root>
                {/if}
              </article>
            {/each}

            {#if group.technicalEvents.length > 0}
              <Collapsible.Root class="group relative rounded-xl border border-dashed bg-muted/15">
                <Collapsible.Trigger
                  class="flex w-full cursor-pointer items-center gap-3 rounded-xl px-4 py-3 text-left hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                >
                  <span
                    class="absolute top-3.5 -left-[2.9rem] flex size-7 items-center justify-center rounded-lg border bg-card text-muted-foreground shadow-xs"
                  >
                    <CheckCircle2Icon class="size-3.5" aria-hidden="true" />
                  </span>
                  <div class="min-w-0 flex-1">
                    <p class="text-sm font-medium">
                      {i18nManager.t('sessions.detail.other_events', {
                        count: group.technicalEvents.length,
                      })}
                    </p>
                    <p class="mt-0.5 text-xs text-muted-foreground">
                      {i18nManager.t('sessions.detail.other_events_desc')}
                    </p>
                  </div>
                  <ChevronRightIcon
                    class="size-3.5 text-muted-foreground transition-transform group-data-[state=open]:rotate-90"
                    aria-hidden="true"
                  />
                </Collapsible.Trigger>
                <Collapsible.Content class="divide-y border-t">
                  {#each group.technicalEvents as event (event.id)}
                    <Collapsible.Root class="group/event">
                      <Collapsible.Trigger
                        class="flex w-full cursor-pointer items-center gap-3 px-4 py-2.5 text-left hover:bg-muted/25 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                      >
                        <span class="min-w-0 flex-1 truncate text-xs font-medium">
                          {eventTypeLabel(event.eventType)}
                        </span>
                        <time class="text-[11px] text-muted-foreground">
                          {formatTime(event.timestampMs)}
                        </time>
                        <ChevronRightIcon
                          class="size-3 text-muted-foreground transition-transform group-data-[state=open]/event:rotate-90"
                          aria-hidden="true"
                        />
                      </Collapsible.Trigger>
                      <Collapsible.Content class="bg-muted/20 px-4 pb-3">
                        <pre class="max-h-80 overflow-auto rounded-lg border bg-background p-3 text-xs leading-5 whitespace-pre-wrap break-words selectable-text">{formatJson(event.event)}</pre>
                      </Collapsible.Content>
                    </Collapsible.Root>
                  {/each}
                </Collapsible.Content>
              </Collapsible.Root>
            {/if}
          </div>
        </section>
          {/each}
        </div>
      </section>
    {/if}
  </div>
</div>
