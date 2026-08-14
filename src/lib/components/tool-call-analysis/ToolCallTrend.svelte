<script lang="ts">
import { i18nManager } from '$lib/i18n.svelte';
import type { ToolCallTrendPoint } from '$lib/tool-call-analysis';

let { points }: { points: ToolCallTrendPoint[] } = $props();

let hoveredPoint = $state<ToolCallTrendPoint | null>(null);
let tooltipX = $state(0);
let tooltipY = $state(0);
let tooltipBelow = $state(false);

const width = 860;
const height = 128;
const padding = 16;
const maxCalls = $derived(Math.max(1, ...points.map((point) => point.callCount)));
const coordinates = $derived(
  points.map((point, index) => ({
    ...point,
    x:
      points.length <= 1
        ? width / 2
        : padding + (index / (points.length - 1)) * (width - padding * 2),
    y: height - padding - (point.callCount / maxCalls) * (height - padding * 2),
  }))
);
const polyline = $derived(coordinates.map((point) => `${point.x},${point.y}`).join(' '));
const totalCalls = $derived(points.reduce((sum, point) => sum + point.callCount, 0));
const totalFailures = $derived(points.reduce((sum, point) => sum + point.failedCount, 0));

function showNearestPoint(event: PointerEvent) {
  const svg = event.currentTarget as SVGSVGElement;
  const bounds = svg.getBoundingClientRect();
  const chartX = ((event.clientX - bounds.left) / bounds.width) * width;
  const point = coordinates.reduce((nearest, candidate) =>
    Math.abs(candidate.x - chartX) < Math.abs(nearest.x - chartX) ? candidate : nearest
  );
  const inset = Math.min(100, bounds.width / 2);
  tooltipX = Math.min(Math.max(event.clientX - bounds.left, inset), bounds.width - inset);
  const pointerY = event.clientY - bounds.top;
  tooltipBelow = pointerY < 52;
  tooltipY = pointerY + (tooltipBelow ? 10 : -8);
  hoveredPoint = point;
}
</script>

<div class="min-w-0" data-testid="tool-call-trend">
  {#if points.length === 0}
    <p class="py-10 text-center text-xs text-muted-foreground">{i18nManager.t('tool_calls.trend.empty')}</p>
  {:else}
    <div class="relative h-28">
      <svg
        viewBox={`0 0 ${width} ${height}`}
        class="h-full w-full"
        role="img"
        aria-label={i18nManager.t('tool_calls.trend.aria', { buckets: points.length, max: maxCalls })}
        onpointerenter={showNearestPoint}
        onpointermove={showNearestPoint}
        onpointerleave={() => (hoveredPoint = null)}
      >
        <line
          x1={padding}
          y1={height - padding}
          x2={width - padding}
          y2={height - padding}
          class="stroke-border"
        />
        <polyline
          points={polyline}
          fill="none"
          class="stroke-primary"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        {#each coordinates as point (point.bucketStartMs)}
          <circle
            cx={point.x}
            cy={point.y}
            r={hoveredPoint?.bucketStartMs === point.bucketStartMs ? 4.5 : 3}
            class="fill-background stroke-primary"
            stroke-width="1.75"
          />
        {/each}
      </svg>
      {#if hoveredPoint}
        <div
          class="pointer-events-none absolute z-10 min-w-36 rounded-lg border bg-popover px-2.5 py-2 text-xs text-popover-foreground shadow-md"
          style:left={`${tooltipX}px`}
          style:top={`${tooltipY}px`}
          style:transform={`translate(-50%, ${tooltipBelow ? '0' : '-100%'})`}
        >
          <p class="font-medium">{hoveredPoint.label}</p>
          <div class="mt-1 flex items-center justify-between gap-4 text-muted-foreground">
            <span>{i18nManager.t('tool_calls.trend.calls')}</span>
            <span class="font-medium tabular-nums text-foreground">{hoveredPoint.callCount}</span>
          </div>
          <div class="mt-0.5 flex items-center justify-between gap-4 text-muted-foreground">
            <span>{i18nManager.t('tool_calls.trend.failures')}</span>
            <span class="font-medium tabular-nums text-foreground">{hoveredPoint.failedCount}</span>
          </div>
        </div>
      {/if}
    </div>
    <p class="sr-only">
      {i18nManager.t('tool_calls.trend.summary', { calls: totalCalls, failures: totalFailures })}
    </p>
  {/if}
</div>
