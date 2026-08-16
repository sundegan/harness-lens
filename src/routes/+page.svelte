<script lang="ts">
import BlocksIcon from '@lucide/svelte/icons/blocks';
import DownloadIcon from '@lucide/svelte/icons/download';
import MessageSquareIcon from '@lucide/svelte/icons/message-square';
import SettingsIcon from '@lucide/svelte/icons/settings';
import WrenchIcon from '@lucide/svelte/icons/wrench';
import SessionBrowser from '$lib/components/session-history/SessionBrowser.svelte';
import SkillAnalysis from '$lib/components/skill-analysis/SkillAnalysis.svelte';
import ToolCallAnalysis from '$lib/components/tool-call-analysis/ToolCallAnalysis.svelte';
import * as Sidebar from '$lib/components/ui/sidebar';
import { i18nManager } from '$lib/i18n.svelte';
import { getMainSidebar } from '$lib/main-sidebar.svelte';
import { settingsDialogManager } from '$lib/settings-dialog.svelte';
import { appUpdateManager } from '$lib/update.svelte';
import { cn } from '$lib/utils';
import {
  type MainModuleId,
  setWorkspaceNavigation,
  WorkspaceNavigation,
} from '$lib/workspace-navigation.svelte';

type MainModule = {
  id: MainModuleId;
  labelKey: string;
};

const modules = [
  {
    id: 'sessions',
    labelKey: 'main.nav.sessions',
  },
  {
    id: 'skills',
    labelKey: 'main.nav.skills',
  },
  {
    id: 'tool-calls',
    labelKey: 'main.nav.tool_calls',
  },
] as const satisfies ReadonlyArray<MainModule>;

const navigation = setWorkspaceNavigation(new WorkspaceNavigation());
const sidebarState = getMainSidebar();
const DEFAULT_SIDEBAR_WIDTH = 210;
const MIN_SIDEBAR_WIDTH = 208;
const MAX_SIDEBAR_WIDTH = 320;
const SIDEBAR_COLLAPSE_THRESHOLD = 144;
const COLLAPSED_SIDEBAR_WIDTH = 64;

const sidebarOpen = $derived(sidebarState.open);
let sidebarWidth = $state(DEFAULT_SIDEBAR_WIDTH);
const expandedSidebarWidth = $derived(Math.max(sidebarWidth, MIN_SIDEBAR_WIDTH));
let resizingSidebar = $state(false);
let resizePointerId: number | null = null;
let resizeStartX = 0;
let resizeStartWidth = DEFAULT_SIDEBAR_WIDTH;
let skillModuleMounted = $state(false);
let toolCallModuleMounted = $state(false);
let sessionBrowser = $state<{ activate(): void }>();
let skillAnalysis = $state<{ activate(): void }>();
let toolCallAnalysis = $state<{ activate(): void }>();

$effect(() => {
  const module = navigation.activeModule;
  if (module === 'skills') skillModuleMounted = true;
  if (module === 'tool-calls') toolCallModuleMounted = true;
});

function activateModule(module: MainModuleId) {
  navigation.activeModule = module;
  if (module === 'skills') {
    skillModuleMounted = true;
    skillAnalysis?.activate();
  } else if (module === 'tool-calls') {
    toolCallModuleMounted = true;
    toolCallAnalysis?.activate();
  } else {
    sessionBrowser?.activate();
  }
}

function clampSidebarWidth(value: number) {
  return Math.min(Math.max(value, MIN_SIDEBAR_WIDTH), MAX_SIDEBAR_WIDTH);
}

function startSidebarResize(event: PointerEvent) {
  if (event.button !== 0 || !event.isPrimary) return;
  event.preventDefault();
  event.stopPropagation();
  resizingSidebar = true;
  resizePointerId = event.pointerId;
  resizeStartX = event.clientX;
  resizeStartWidth = sidebarOpen ? expandedSidebarWidth : COLLAPSED_SIDEBAR_WIDTH;
  if (event.currentTarget instanceof HTMLElement) {
    event.currentTarget.setPointerCapture(event.pointerId);
  }
}

function resizeSidebar(event: PointerEvent) {
  if (!resizingSidebar || event.pointerId !== resizePointerId) return;
  event.preventDefault();
  const nextWidth = resizeStartWidth + event.clientX - resizeStartX;
  const startedCollapsed = resizeStartWidth === COLLAPSED_SIDEBAR_WIDTH;

  if (startedCollapsed && nextWidth <= COLLAPSED_SIDEBAR_WIDTH) {
    sidebarState.open = false;
    return;
  }

  if (!startedCollapsed && nextWidth < SIDEBAR_COLLAPSE_THRESHOLD) {
    sidebarState.open = false;
    return;
  }

  sidebarState.open = true;
  sidebarWidth = clampSidebarWidth(nextWidth);
}

function stopSidebarResize(event: PointerEvent) {
  if (!resizingSidebar || event.pointerId !== resizePointerId) return;
  if (
    event.currentTarget instanceof HTMLElement &&
    event.currentTarget.hasPointerCapture(event.pointerId)
  ) {
    event.currentTarget.releasePointerCapture(event.pointerId);
  }
  resizingSidebar = false;
  resizePointerId = null;
}
</script>

<svelte:head>
  <title>{i18nManager.t('main.title')}</title>
</svelte:head>

<a
  href="#main-workspace"
  class="sr-only z-50 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground focus:not-sr-only focus:absolute focus:top-3 focus:left-3"
>
  {i18nManager.t('main.skip_to_content')}
</a>

<svelte:window onpointercancel={stopSidebarResize} />

{#if resizingSidebar}
  <div class="fixed inset-0 z-[60] cursor-col-resize" data-testid="main-sidebar-resize-overlay"></div>
{/if}

<Sidebar.Provider
  bind:open={sidebarState.open}
  class={cn(
    'h-full min-h-0 grid grid-cols-[var(--sidebar-width)_minmax(0,1fr)]',
    !resizingSidebar && !sidebarState.isToggling && 'transition-[grid-template-columns] duration-200 ease-out'
  )}
  style={`--sidebar-width: ${sidebarOpen ? expandedSidebarWidth : COLLAPSED_SIDEBAR_WIDTH}px;`}
>
  <Sidebar.Root
    collapsible="none"
    class="relative w-full min-w-0 border-r border-sidebar-border/70 bg-sidebar/95"
    aria-label={i18nManager.t('main.nav.label')}
  >
    <Sidebar.Header class="px-3 pt-0 pb-2">
      <div
        class={cn(
          'flex w-full items-center border-b border-sidebar-border/70 pb-3',
          sidebarOpen ? 'gap-3' : 'justify-center'
        )}
      >
        <img
          src="/app-icon.png"
          alt={i18nManager.t('main.title')}
          class="size-10 shrink-0 translate-x-0.5"
        />
        {#if sidebarOpen}
          <div class="min-w-0">
            <p class="truncate text-sm font-semibold tracking-tight text-foreground">{i18nManager.t('main.title')}</p>
          </div>
        {/if}
      </div>
    </Sidebar.Header>

    <Sidebar.Content>
      <Sidebar.Group class={cn(sidebarOpen && 'px-3 pt-2 pb-3')}>
        <Sidebar.GroupContent>
          <Sidebar.Menu class="gap-1">
            {#each modules as module (module.id)}
              {@const isActive = navigation.activeModule === module.id}
              <Sidebar.MenuItem>
                {#if sidebarOpen && isActive}
                  <span class="pointer-events-none absolute inset-y-2 start-0 z-10 w-0.5 rounded-full bg-primary" aria-hidden="true"></span>
                {/if}
                <Sidebar.MenuButton
                  isActive={isActive}
                  tooltipContent={i18nManager.t(module.labelKey)}
                  tooltipContentProps={{ hidden: sidebarOpen, sideOffset: 8 }}
                  class={cn(
                    'transition-colors duration-150 data-active:bg-primary/10 data-active:text-primary data-active:shadow-sm data-active:ring-1 data-active:ring-primary/15',
                    sidebarOpen
                      ? 'h-10 rounded-md px-2.5 text-[13px] hover:bg-sidebar-accent/80'
                      : 'mx-auto size-8 justify-center p-0'
                  )}
                  aria-label={i18nManager.t(module.labelKey)}
                  aria-current={isActive ? 'page' : undefined}
                  aria-pressed={isActive}
                  data-testid={`main-nav-${module.id}`}
                  onclick={() => activateModule(module.id)}
                >
                  <span
                    class={cn(
                      'flex shrink-0 items-center justify-center',
                      sidebarOpen
                        ? isActive
                          ? 'size-8 rounded-md bg-primary text-primary-foreground shadow-sm'
                          : 'size-8 rounded-md bg-muted/70 text-muted-foreground group-hover/menu-button:bg-sidebar-accent group-hover/menu-button:text-foreground'
                        : 'size-4'
                    )}
                  >
                    {#if module.id === 'sessions'}
                      <MessageSquareIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
                    {:else if module.id === 'skills'}
                      <BlocksIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
                    {:else}
                      <WrenchIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
                    {/if}
                  </span>
                  {#if sidebarOpen}
                    <span class="min-w-0 truncate">{i18nManager.t(module.labelKey)}</span>
                  {/if}
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
            {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    </Sidebar.Content>

    <Sidebar.Separator class="mx-3" />

    <Sidebar.Footer class={cn('py-3', sidebarOpen ? 'px-3' : 'px-2')}>
      <Sidebar.Menu class="gap-1">
        {#if appUpdateManager.hasUpdate}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              tooltipContent={i18nManager.t('update.action.download')}
              tooltipContentProps={{ hidden: sidebarOpen, sideOffset: 8 }}
              class={cn(
                'update-sidebar-entry transition-[background-color,border-color,color] duration-200',
                sidebarOpen
                  ? 'h-10 w-full rounded-lg border border-green-500/25 bg-gradient-to-r from-green-500/[0.16] via-green-500/[0.07] to-transparent px-2.5 text-[13px] text-green-700 shadow-[0_0_18px_rgba(22,163,74,0.16)] ring-1 ring-green-500/10 hover:border-green-500/40 hover:from-green-500/[0.22] hover:via-green-500/[0.1] hover:text-green-800 dark:border-green-400/25 dark:from-green-400/[0.16] dark:via-green-400/[0.07] dark:text-green-400 dark:hover:border-green-400/40 dark:hover:from-green-400/[0.22]'
                  : 'mx-auto size-7! justify-center rounded-full border border-green-500/30 bg-green-500/[0.14] p-0 text-green-600 shadow-[0_0_10px_rgba(22,163,74,0.24)] ring-1 ring-green-500/20 hover:bg-green-500/[0.22] hover:ring-green-500/35 dark:border-green-400/30 dark:bg-green-400/[0.14] dark:text-green-400 dark:ring-green-400/20'
              )}
              aria-label={i18nManager.t('update.action.download')}
              data-testid="main-nav-update"
              onclick={() => appUpdateManager.openUpdateDialog()}
            >
              <span
                class={cn(
                  'flex shrink-0 items-center justify-center',
                  sidebarOpen
                    ? 'size-7 rounded-full bg-green-500/15 text-green-600 shadow-sm ring-1 ring-green-500/20 dark:bg-green-400/15 dark:text-green-400 dark:ring-green-400/20'
                    : 'size-4'
                )}
              >
                <DownloadIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
              </span>
              {#if sidebarOpen}
                <span class="min-w-0 flex-1 truncate">{i18nManager.t('update.action.download')}</span>
                <span
                  class="size-1.5 shrink-0 rounded-full bg-green-500 ring-2 ring-green-500/20 shadow-[0_0_10px_rgba(22,163,74,0.45)] dark:bg-green-400 dark:ring-green-400/20"
                  aria-hidden="true"
                ></span>
              {/if}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        {/if}
        <Sidebar.MenuItem>
          <Sidebar.MenuButton
            tooltipContent={i18nManager.t('main.nav.settings')}
            tooltipContentProps={{ hidden: sidebarOpen, sideOffset: 8 }}
            class={cn(
              'transition-colors duration-150',
              sidebarOpen
                ? 'h-10 w-full rounded-md px-2.5 text-[13px] hover:bg-sidebar-accent/80'
                : 'mx-auto size-8 justify-center p-0'
            )}
            aria-label={i18nManager.t('main.nav.settings')}
            data-testid="main-nav-settings"
            onclick={() => settingsDialogManager.show()}
          >
            <span
              class={cn(
                'flex shrink-0 items-center justify-center',
                sidebarOpen ? 'size-8 rounded-md bg-muted/70 text-muted-foreground' : 'size-4'
              )}
            >
              <SettingsIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
            </span>
            {#if sidebarOpen}
              <span class="min-w-0 truncate">{i18nManager.t('main.nav.settings')}</span>
            {/if}
          </Sidebar.MenuButton>
        </Sidebar.MenuItem>
      </Sidebar.Menu>
    </Sidebar.Footer>

    <div
      class={cn(
        'group absolute inset-y-0 -right-1 z-20 hidden w-2 cursor-col-resize touch-none bg-transparent sm:block',
        resizingSidebar && 'bg-primary/5'
      )}
      role="separator"
      aria-orientation="vertical"
      aria-label={i18nManager.t('main.nav.resize')}
      data-testid="main-sidebar-resize"
      onpointerdown={startSidebarResize}
      onpointermove={resizeSidebar}
      onpointerup={stopSidebarResize}
      onpointercancel={stopSidebarResize}
    >
      <span
        class="absolute inset-y-0 right-0 w-px bg-border transition-colors group-hover:w-0.5 group-hover:bg-primary/70"
      ></span>
    </div>
  </Sidebar.Root>

  <Sidebar.Inset id="main-workspace" class="min-h-0 min-w-0 overflow-hidden">
    <div class="min-h-0 flex-1 overflow-auto bg-background">
      <section
        class={cn('min-h-full w-full bg-card', navigation.activeModule === 'sessions' ? 'flex' : 'hidden')}
        aria-label={i18nManager.t('main.nav.sessions')}
        aria-hidden={navigation.activeModule !== 'sessions'}
      >
        <SessionBrowser bind:this={sessionBrowser} active={navigation.activeModule === 'sessions'} />
      </section>
      {#if skillModuleMounted}
        <section
          class={cn('min-h-full w-full bg-card', navigation.activeModule === 'skills' ? 'flex' : 'hidden')}
          aria-label={i18nManager.t('main.nav.skills')}
          aria-hidden={navigation.activeModule !== 'skills'}
        >
          <SkillAnalysis bind:this={skillAnalysis} active={navigation.activeModule === 'skills'} />
        </section>
      {/if}
      {#if toolCallModuleMounted}
        <section
          class={cn('min-h-full w-full bg-card', navigation.activeModule === 'tool-calls' ? 'flex' : 'hidden')}
          aria-label={i18nManager.t('main.nav.tool_calls')}
          aria-hidden={navigation.activeModule !== 'tool-calls'}
        >
          <ToolCallAnalysis bind:this={toolCallAnalysis} active={navigation.activeModule === 'tool-calls'} />
        </section>
      {/if}
    </div>
  </Sidebar.Inset>
</Sidebar.Provider>

<style>
  :global(.update-sidebar-entry) {
    --update-glow: rgb(22 163 74 / 18%);
    --update-glow-soft: rgb(22 163 74 / 10%);
    --update-glow-strong: rgb(22 163 74 / 24%);
    animation: update-sidebar-pulse 2.6s infinite ease-in-out;
  }

  :global(.dark .update-sidebar-entry) {
    --update-glow: rgb(74 222 128 / 20%);
    --update-glow-soft: rgb(74 222 128 / 12%);
    --update-glow-strong: rgb(74 222 128 / 28%);
  }

  @keyframes update-sidebar-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 var(--update-glow), 0 0 12px var(--update-glow-soft);
    }

    50% {
      box-shadow: 0 0 0 4px rgb(22 163 74 / 0%), 0 0 24px var(--update-glow-strong);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.update-sidebar-entry) {
      animation: none;
    }
  }
</style>
