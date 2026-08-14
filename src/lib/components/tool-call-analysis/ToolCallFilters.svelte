<script lang="ts">
import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
import XIcon from '@lucide/svelte/icons/x';
import { Badge } from '$lib/components/ui/badge';
import { Button, buttonVariants } from '$lib/components/ui/button';
import { Checkbox } from '$lib/components/ui/checkbox';
import * as Popover from '$lib/components/ui/popover';
import { i18nManager } from '$lib/i18n.svelte';
import type {
  ToolCallFilterOption,
  ToolCallFilterOptions,
  ToolCallFilters,
} from '$lib/tool-call-analysis';
import { cn } from '$lib/utils';

type ArrayField = 'providers' | 'projectKeys' | 'toolNames' | 'mcpServers';

interface FilterGroup {
  field: ArrayField;
  label: string;
  values: ToolCallFilterOption[];
}

let {
  filters,
  options,
  onchange,
}: {
  filters: ToolCallFilters;
  options: ToolCallFilterOptions | null;
  onchange: (filters: ToolCallFilters) => void;
} = $props();

const groups = $derived<FilterGroup[]>([
  {
    field: 'providers',
    label: i18nManager.t('tool_calls.filter.provider'),
    values: options?.providers ?? [],
  },
  {
    field: 'projectKeys',
    label: i18nManager.t('tool_calls.filter.project'),
    values: options?.projects ?? [],
  },
  {
    field: 'toolNames',
    label: i18nManager.t('tool_calls.filter.tool'),
    values: options?.toolNames ?? [],
  },
  {
    field: 'mcpServers',
    label: i18nManager.t('tool_calls.filter.mcp'),
    values: options?.mcpServers ?? [],
  },
]);

const selected = $derived(
  groups.flatMap((group) =>
    filters[group.field].map((value) => ({
      field: group.field,
      group: group.label,
      value,
      label: group.values.find((option) => option.value === value)?.label ?? value,
    }))
  )
);

function toggle(field: ArrayField, value: string) {
  const values = filters[field];
  onchange({
    ...filters,
    [field]: values.includes(value) ? values.filter((item) => item !== value) : [...values, value],
  });
}

function clear() {
  onchange({
    ...filters,
    providers: [],
    projectKeys: [],
    toolNames: [],
    mcpServers: [],
  });
}
</script>

<div class="flex flex-col gap-2 border-b bg-card px-5 py-2 sm:px-6" aria-label={i18nManager.t('tool_calls.filter.label')}>
  <div class="flex flex-wrap items-center gap-2">
    {#each groups as group (group.field)}
      <Popover.Root>
        <Popover.Trigger
          class={cn(buttonVariants({ variant: 'outline', size: 'sm' }), 'min-w-28 justify-between bg-background text-xs')}
          aria-label={i18nManager.t('tool_calls.filter.selected', { label: group.label, count: filters[group.field].length })}
        >
          <span>{group.label}</span>
          {#if filters[group.field].length > 0}
            <Badge variant="secondary">{filters[group.field].length}</Badge>
          {:else}
            <ChevronDownIcon data-icon="inline-end" />
          {/if}
        </Popover.Trigger>
        <Popover.Content align="start" class="w-72 gap-2 p-2">
          <Popover.Header class="px-2 py-1">
            <Popover.Title class="text-sm">{group.label}</Popover.Title>
            <Popover.Description class="text-xs">{i18nManager.t('tool_calls.filter.multiple')}</Popover.Description>
          </Popover.Header>
          <div class="max-h-72 overflow-auto">
            {#if group.values.length === 0}
              <p class="px-2 py-4 text-center text-sm text-muted-foreground">{i18nManager.t('tool_calls.filter.no_options')}</p>
            {:else}
              {#each group.values as option (option.value)}
                <label class="flex cursor-pointer items-center gap-3 rounded-md px-2 py-1.5 text-xs hover:bg-muted">
                  <Checkbox
                    checked={filters[group.field].includes(option.value)}
                    onCheckedChange={() => toggle(group.field, option.value)}
                  />
                  <span class="min-w-0 flex-1 truncate">{option.label}</span>
                  {#if option.count > 0}
                    <span class="text-xs tabular-nums text-muted-foreground">{option.count}</span>
                  {/if}
                </label>
              {/each}
            {/if}
          </div>
        </Popover.Content>
      </Popover.Root>
    {/each}
    {#if selected.length > 0}
      <Button variant="ghost" size="sm" onclick={clear}>{i18nManager.t('tool_calls.filter.clear')}</Button>
    {/if}
  </div>

  {#if selected.length > 0}
    <div class="flex flex-wrap gap-1.5" aria-label={i18nManager.t('tool_calls.filter.selected_label')}>
      {#each selected as item (`${item.field}:${item.value}`)}
        <Badge variant="outline" class="gap-1">
          <span class="text-muted-foreground">{item.group}</span>
          <span>{item.label}</span>
          <button
            type="button"
            class="rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            aria-label={i18nManager.t('tool_calls.filter.remove', { group: item.group, label: item.label })}
            onclick={() => toggle(item.field, item.value)}
          >
            <XIcon aria-hidden="true" />
          </button>
        </Badge>
      {/each}
    </div>
  {/if}
</div>
