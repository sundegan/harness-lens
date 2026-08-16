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

<Sidebar.Provider class="h-full min-h-0" style="--sidebar-width: 4rem; --sidebar-width-icon: 4rem;">
  <Sidebar.Root
    collapsible="none"
    class="w-16 border-r border-sidebar-border bg-sidebar"
    aria-label={i18nManager.t('main.nav.label')}
  >
    <Sidebar.Header class="items-center px-2 pt-3 pb-2">
      <img
        src="/app-icon.png"
        alt={i18nManager.t('main.title')}
        class="size-10 shrink-0"
      />
    </Sidebar.Header>

    <Sidebar.Content>
      <Sidebar.Group class="px-2 py-2">
        <Sidebar.GroupContent>
          <Sidebar.Menu class="gap-1">
        {#each modules as module (module.id)}
          {@const isActive = navigation.activeModule === module.id}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={isActive}
              tooltipContent={i18nManager.t(module.labelKey)}
              tooltipContentProps={{ hidden: false, sideOffset: 8 }}
              class="mx-auto size-8 justify-center p-0 data-active:bg-muted data-active:text-foreground"
              aria-label={i18nManager.t(module.labelKey)}
              aria-current={isActive ? 'page' : undefined}
              aria-pressed={isActive}
              data-testid={`main-nav-${module.id}`}
              onclick={() => activateModule(module.id)}
            >
              {#if module.id === 'sessions'}
                <MessageSquareIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
              {:else if module.id === 'skills'}
                <BlocksIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
              {:else if module.id === 'tool-calls'}
                <WrenchIcon class="size-4" strokeWidth={1.8} aria-hidden="true" />
              {/if}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    </Sidebar.Content>

    <Sidebar.Footer class="items-center px-2 py-3">
      <Sidebar.Menu>
        {#if appUpdateManager.hasUpdate}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              tooltipContent={i18nManager.t('update.action.download')}
              tooltipContentProps={{ hidden: false, sideOffset: 8 }}
              class="update-entry mx-auto size-7! justify-center rounded-full p-0"
              aria-label={i18nManager.t('update.action.download')}
              data-testid="main-nav-update"
              onclick={() => appUpdateManager.openUpdateDialog()}
            >
              <DownloadIcon strokeWidth={1.8} aria-hidden="true" />
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        {/if}
        <Sidebar.MenuItem>
          <Sidebar.MenuButton
            tooltipContent={i18nManager.t('main.nav.settings')}
            tooltipContentProps={{ hidden: false, sideOffset: 8 }}
            class="mx-auto size-8 justify-center p-0"
            aria-label={i18nManager.t('main.nav.settings')}
            data-testid="main-nav-settings"
            onclick={() => settingsDialogManager.show()}
          >
            <SettingsIcon strokeWidth={1.8} aria-hidden="true" />
          </Sidebar.MenuButton>
        </Sidebar.MenuItem>
      </Sidebar.Menu>
    </Sidebar.Footer>
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
