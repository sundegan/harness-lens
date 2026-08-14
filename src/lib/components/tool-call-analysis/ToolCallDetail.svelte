<script lang="ts">
import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';
import CheckIcon from '@lucide/svelte/icons/check';
import CopyIcon from '@lucide/svelte/icons/copy';
import XIcon from '@lucide/svelte/icons/x';
import { onDestroy } from 'svelte';
import { Button } from '$lib/components/ui/button';
import * as Sheet from '$lib/components/ui/sheet';
import * as Tooltip from '$lib/components/ui/tooltip';
import { i18nManager } from '$lib/i18n.svelte';
import { logWarn } from '$lib/logger';
import type { ToolCallDetail } from '$lib/tool-call-analysis';

let {
  detail,
  onclose,
  onviewsession,
}: {
  detail: ToolCallDetail;
  onclose: () => void;
  onviewsession: (sessionId: string, eventId?: string) => void;
} = $props();

function json(value: unknown): string {
  return JSON.stringify(value ?? null, null, 2);
}

type CopyTarget = 'input' | 'output';
type JsonLine = { id: number; indent: number; text: string };

const DEFAULT_DRAWER_WIDTH = 672;
const MAX_DRAWER_WIDTH = 1152;
const DRAWER_VIEWPORT_GAP = 16;
const KEYBOARD_RESIZE_STEP = 32;

let copied = $state<CopyTarget | null>(null);
let drawerWidth = $state(DEFAULT_DRAWER_WIDTH);
let resizing = $state(false);
let copyResetTimer: ReturnType<typeof setTimeout> | undefined;
let resizeStartX = 0;
let resizeStartWidth = DEFAULT_DRAWER_WIDTH;

function drawerBounds(): { min: number; max: number } {
  const viewportLimit =
    typeof window === 'undefined'
      ? MAX_DRAWER_WIDTH
      : Math.max(0, window.innerWidth - DRAWER_VIEWPORT_GAP);
  return {
    min: Math.min(DEFAULT_DRAWER_WIDTH, viewportLimit),
    max: Math.min(MAX_DRAWER_WIDTH, viewportLimit),
  };
}

function setDrawerWidth(value: number) {
  const { min, max } = drawerBounds();
  drawerWidth = Math.min(Math.max(value, min), max);
}

function startResize(event: MouseEvent) {
  if (event.button !== 0) return;
  if (event.currentTarget instanceof HTMLElement) {
    event.currentTarget.focus({ preventScroll: true });
  }
  event.preventDefault();
  event.stopPropagation();
  resizing = true;
  resizeStartX = event.clientX;
  resizeStartWidth = drawerWidth;
}

function resizeDrawer(event: MouseEvent) {
  if (!resizing) return;
  event.preventDefault();
  setDrawerWidth(resizeStartWidth + resizeStartX - event.clientX);
}

function stopResize() {
  if (!resizing) return;
  resizing = false;
}

function constrainDrawerToViewport() {
  setDrawerWidth(drawerWidth);
}

function resizeFromKeyboard(event: KeyboardEvent) {
  if (event.key === 'ArrowLeft') {
    event.preventDefault();
    setDrawerWidth(drawerWidth + KEYBOARD_RESIZE_STEP);
  } else if (event.key === 'ArrowRight') {
    event.preventDefault();
    setDrawerWidth(drawerWidth - KEYBOARD_RESIZE_STEP);
  } else if (event.key === 'Home') {
    event.preventDefault();
    setDrawerWidth(drawerBounds().min);
  } else if (event.key === 'End') {
    event.preventDefault();
    setDrawerWidth(drawerBounds().max);
  }
}

function selectedCopyableText(): string | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;

  const range = selection.getRangeAt(0);
  const selectableRegion = (node: Node): Element | null => {
    const element = node instanceof Element ? node : node.parentElement;
    return element?.closest('[data-testid="tool-call-detail"] .selectable-text') ?? null;
  };
  const startRegion = selectableRegion(range.startContainer);
  if (!startRegion || selectableRegion(range.endContainer) !== startRegion) return null;

  const text = selection.toString();
  return text.length > 0 ? text : null;
}

function copySelectedText(event: ClipboardEvent) {
  const text = selectedCopyableText();
  if (!text || !event.clipboardData) return;

  event.preventDefault();
  event.clipboardData.setData('text/plain', text);
}

function jsonLines(value: unknown): JsonLine[] {
  return json(value)
    .split('\n')
    .map((line, id) => {
      const indent = line.length - line.trimStart().length;
      return { id, indent, text: line.slice(indent) };
    });
}

async function copyJson(target: CopyTarget, value: unknown) {
  try {
    await navigator.clipboard.writeText(json(value));
    copied = target;
    if (copyResetTimer !== undefined) clearTimeout(copyResetTimer);
    copyResetTimer = setTimeout(() => {
      copied = null;
      copyResetTimer = undefined;
    }, 1500);
  } catch (error) {
    logWarn(`Failed to copy MCP tool call ${target}`, error);
  }
}

onDestroy(() => {
  if (copyResetTimer !== undefined) clearTimeout(copyResetTimer);
});
</script>

<svelte:window
  onmousemove={resizeDrawer}
  onmouseup={stopResize}
  onblur={stopResize}
  onresize={constrainDrawerToViewport}
  oncopy={copySelectedText}
/>

{#if resizing}
  <div class="fixed inset-0 z-[60] cursor-col-resize" data-testid="tool-call-detail-resize-overlay"></div>
{/if}

<Tooltip.Provider delayDuration={0} skipDelayDuration={0} disableHoverableContent>
  <Sheet.Root open={true} onOpenChange={(open) => !open && onclose()}>
    <Sheet.Content
      class="flex min-w-0 flex-col gap-0 overflow-hidden"
      style={`width: ${drawerWidth}px; min-width: min(calc(100vw - 1rem), ${DEFAULT_DRAWER_WIDTH}px); max-width: min(calc(100vw - 1rem), ${MAX_DRAWER_WIDTH}px);`}
      showCloseButton={false}
      data-testid="tool-call-detail"
    >
      <div
        class={[
          'group absolute inset-y-0 left-0 z-20 w-3 cursor-col-resize touch-none border-0 bg-transparent p-0 outline-none focus-visible:bg-primary/5 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60',
          resizing && 'bg-primary/5',
        ]}
        role="slider"
        aria-orientation="horizontal"
        aria-label={i18nManager.t('tool_calls.detail.resize')}
        aria-valuemin={drawerBounds().min}
        aria-valuemax={drawerBounds().max}
        aria-valuenow={drawerWidth}
        tabindex={0}
        data-testid="tool-call-detail-resize"
        onmousedown={startResize}
        onkeydown={resizeFromKeyboard}
      >
        <span class="absolute inset-y-0 left-0 w-px bg-border transition-colors group-hover:w-0.5 group-hover:bg-primary/70 group-focus-visible:w-0.5 group-focus-visible:bg-primary"></span>
      </div>
      <Sheet.Header class="border-b p-5 pb-4">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0 flex-1">
            <Sheet.Title class="wrap-break-word pr-2 leading-6" data-testid="tool-call-detail-session-title">{detail.call.sessionTitle || i18nManager.t('tool_calls.untitled_session')}</Sheet.Title>
            <Sheet.Description class="selectable-text mt-1 break-all font-mono text-xs leading-5" data-testid="tool-call-detail-session-id">{i18nManager.t('tool_calls.detail.session_id')}: {detail.call.sourceSessionId}</Sheet.Description>
          </div>
          <Button class="shrink-0" variant="ghost" size="icon-sm" aria-label={i18nManager.t('tool_calls.detail.close')} onclick={onclose}>
            <XIcon />
          </Button>
        </div>
      </Sheet.Header>

      <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-auto px-5 py-4">
        <dl class="grid grid-cols-2 gap-x-5 gap-y-3 text-xs">
          <div><dt class="text-muted-foreground">{i18nManager.t('tool_calls.detail.agent_version')}</dt><dd>{detail.call.agentVersion || i18nManager.t('tool_calls.unknown')}</dd></div>
          <div><dt class="text-muted-foreground">{i18nManager.t('tool_calls.detail.tool_kind')}</dt><dd>{detail.call.toolKind}</dd></div>
          <div><dt class="text-muted-foreground">{i18nManager.t('tool_calls.detail.duration')}</dt><dd>{detail.call.durationMs === null ? i18nManager.t('tool_calls.unknown') : `${detail.call.durationMs} ms`}</dd></div>
          <div><dt class="text-muted-foreground">{i18nManager.t('tool_calls.detail.result_evidence')}</dt><dd>{detail.call.hasResult ? i18nManager.t('tool_calls.detail.recorded') : i18nManager.t('tool_calls.detail.missing')}</dd></div>
        </dl>

        <section>
          <h3 class="mb-2 text-sm font-semibold">{i18nManager.t('tool_calls.detail.input')}</h3>
          {#if detail.callEvent}
            <div class="relative">
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <Button {...props} class="absolute top-2 right-4 z-10 bg-muted/90" variant="ghost" size="icon-xs" data-testid="tool-call-copy-input" data-copied={copied === 'input'} aria-label={i18nManager.t('tool_calls.detail.copy_input')} onclick={() => void copyJson('input', detail.callEvent?.event.data)}>
                      {#if copied === 'input'}<CheckIcon aria-hidden="true" />{:else}<CopyIcon aria-hidden="true" />{/if}
                    </Button>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content>{copied === 'input' ? i18nManager.t('tool_calls.detail.copied') : i18nManager.t('tool_calls.detail.copy_input')}</Tooltip.Content>
              </Tooltip.Root>
              <pre class="selectable-text max-h-72 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs leading-5" data-testid="tool-call-input-json">{#each jsonLines(detail.callEvent.event.data) as line (line.id)}<code class="block whitespace-pre-wrap break-words" style:padding-left={`${line.indent}ch`}>{line.text}</code>{/each}</pre>
            </div>
          {:else}
            <p class="text-sm text-muted-foreground">{i18nManager.t('tool_calls.detail.input_missing')}</p>
          {/if}
        </section>

        <section>
          <h3 class="mb-2 text-sm font-semibold">{i18nManager.t('tool_calls.detail.output')}</h3>
          {#if detail.resultEvent}
            <div class="relative">
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <Button {...props} class="absolute top-2 right-4 z-10 bg-muted/90" variant="ghost" size="icon-xs" data-testid="tool-call-copy-output" data-copied={copied === 'output'} aria-label={i18nManager.t('tool_calls.detail.copy_output')} onclick={() => void copyJson('output', detail.resultEvent?.event.data)}>
                      {#if copied === 'output'}<CheckIcon aria-hidden="true" />{:else}<CopyIcon aria-hidden="true" />{/if}
                    </Button>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content>{copied === 'output' ? i18nManager.t('tool_calls.detail.copied') : i18nManager.t('tool_calls.detail.copy_output')}</Tooltip.Content>
              </Tooltip.Root>
              <pre class="selectable-text max-h-72 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs leading-5" data-testid="tool-call-output-json">{#each jsonLines(detail.resultEvent.event.data) as line (line.id)}<code class="block whitespace-pre-wrap break-words" style:padding-left={`${line.indent}ch`}>{line.text}</code>{/each}</pre>
            </div>
          {:else}
            <p class="text-sm text-muted-foreground">{i18nManager.t('tool_calls.detail.output_missing')}</p>
          {/if}
        </section>
      </div>

      <Sheet.Footer class="border-t px-5 pt-4 pb-5">
        <Button size="sm" data-testid="tool-call-view-session" onclick={() => onviewsession(detail.call.sessionId, detail.sessionEventId ?? undefined)}>
          {i18nManager.t('tool_calls.detail.view_session')}
          <ArrowRightIcon data-icon="inline-end" />
        </Button>
      </Sheet.Footer>
    </Sheet.Content>
  </Sheet.Root>
</Tooltip.Provider>

<style>
  :global([data-testid='tool-call-detail']),
  :global([data-testid='tool-call-detail'] *) {
    -webkit-user-select: none;
    user-select: none;
  }

  :global([data-testid='tool-call-detail'] .selectable-text),
  :global([data-testid='tool-call-detail'] .selectable-text *) {
    -webkit-user-select: text;
    user-select: text;
  }
</style>
