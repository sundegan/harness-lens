<script lang="ts">
import BlocksIcon from '@lucide/svelte/icons/blocks';
import MessageSquareIcon from '@lucide/svelte/icons/message-square';
import SettingsIcon from '@lucide/svelte/icons/settings';
import SessionBrowser from '$lib/components/session-history/SessionBrowser.svelte';
import SkillAnalysis from '$lib/components/skill-analysis/SkillAnalysis.svelte';
import * as Sidebar from '$lib/components/ui/sidebar';
import { i18nManager } from '$lib/i18n.svelte';
import { settingsDialogManager } from '$lib/settings-dialog.svelte';

type MainModuleId = 'sessions' | 'skills';
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
] as const satisfies ReadonlyArray<MainModule>;

let activeModule = $state<MainModuleId>('sessions');

const activeModuleConfig = $derived(
  modules.find((module) => module.id === activeModule) ?? modules[0]
);
const activeModuleLabel = $derived(i18nManager.t(activeModuleConfig.labelKey));
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
          {@const isActive = activeModule === module.id}
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
              onclick={() => (activeModule = module.id)}
            >
              {#if module.id === 'sessions'}
                <MessageSquareIcon strokeWidth={1.8} aria-hidden="true" />
              {:else}
                <BlocksIcon strokeWidth={1.8} aria-hidden="true" />
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
      {#if activeModule === 'sessions'}
        <section class="flex min-h-full w-full bg-card" aria-label={activeModuleLabel}>
          <SessionBrowser />
        </section>
      {:else}
        <section class="flex min-h-full w-full bg-card" aria-label={activeModuleLabel}>
          <SkillAnalysis />
        </section>
      {/if}
    </div>
  </Sidebar.Inset>
</Sidebar.Provider>
