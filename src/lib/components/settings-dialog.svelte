<script lang="ts">
import MonitorCogIcon from '@lucide/svelte/icons/monitor-cog';
import SettingsIcon from '@lucide/svelte/icons/settings';
import XIcon from '@lucide/svelte/icons/x';
import * as Breadcrumb from '$lib/components/ui/breadcrumb/index.js';
import { Button } from '$lib/components/ui/button/index.js';
import * as Dialog from '$lib/components/ui/dialog/index.js';
import * as Field from '$lib/components/ui/field/index.js';
import * as Select from '$lib/components/ui/select/index.js';
import * as Sidebar from '$lib/components/ui/sidebar/index.js';
import { i18nManager } from '$lib/i18n.svelte';
import { themeManager } from '$lib/theme.svelte';

type SettingsSection = 'general' | 'appearance';

const languageOptions = $derived([
  { value: 'system', label: i18nManager.t('settings.language.lang_system') },
  { value: 'en', label: i18nManager.t('settings.language.lang_en') },
  { value: 'zh', label: i18nManager.t('settings.language.lang_zh') },
  { value: 'zh_tw', label: i18nManager.t('settings.language.lang_zh_tw') },
  { value: 'ja', label: i18nManager.t('settings.language.lang_ja') },
  { value: 'ko', label: i18nManager.t('settings.language.lang_ko') },
  { value: 'es', label: i18nManager.t('settings.language.lang_es') },
  { value: 'fr', label: i18nManager.t('settings.language.lang_fr') },
  { value: 'de', label: i18nManager.t('settings.language.lang_de') },
]);

const themeOptions = $derived([
  { value: 'system', label: i18nManager.t('settings.appearance.theme_system') },
  { value: 'light', label: i18nManager.t('settings.appearance.theme_light') },
  { value: 'dark', label: i18nManager.t('settings.appearance.theme_dark') },
]);

const navigation = $derived([
  {
    value: 'general' as const,
    label: i18nManager.t('nav.general'),
    icon: SettingsIcon,
  },
  {
    value: 'appearance' as const,
    label: i18nManager.t('nav.appearance'),
    icon: MonitorCogIcon,
  },
]);

let { open = $bindable(false) }: { open?: boolean } = $props();
let activeSection = $state<SettingsSection>('general');

const activeLabel = $derived(
  navigation.find((item) => item.value === activeSection)?.label ??
    i18nManager.t('control.settings')
);
const selectedLanguageLabel = $derived(
  languageOptions.find((option) => option.value === i18nManager.language)?.label ?? ''
);
const selectedThemeLabel = $derived(
  themeOptions.find((option) => option.value === themeManager.theme)?.label ?? ''
);
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="h-[min(34rem,calc(100%-2rem))] max-w-[calc(100%-2rem)] gap-0 overflow-hidden p-0 md:max-w-[700px] lg:max-w-[800px]"
    showCloseButton={false}
    data-testid="settings-dialog"
  >
    <Dialog.Title class="sr-only">{i18nManager.t('control.settings')}</Dialog.Title>
    <Dialog.Description class="sr-only">{activeLabel}</Dialog.Description>

    <Sidebar.Provider class="h-full min-h-0 items-start" style="--sidebar-width: 13rem;">
      <Sidebar.Root
        collapsible="none"
        class="hidden w-52 shrink-0 border-r bg-sidebar md:flex"
        aria-label={i18nManager.t('control.settings')}
      >
        <Sidebar.Content>
          <Sidebar.Group class="px-2 py-3">
            <Sidebar.GroupContent>
              <Sidebar.Menu class="gap-1">
                {#each navigation as item (item.value)}
                  <Sidebar.MenuItem>
                    <Sidebar.MenuButton
                      isActive={activeSection === item.value}
                      class="h-9"
                      onclick={() => (activeSection = item.value)}
                    >
                      <item.icon aria-hidden="true" />
                      <span>{item.label}</span>
                    </Sidebar.MenuButton>
                  </Sidebar.MenuItem>
                {/each}
              </Sidebar.Menu>
            </Sidebar.GroupContent>
          </Sidebar.Group>
        </Sidebar.Content>

      </Sidebar.Root>

      <main class="flex h-full min-w-0 flex-1 flex-col overflow-hidden bg-background">
        <header class="flex h-14 shrink-0 items-center justify-between gap-3 border-b px-4">
          <Breadcrumb.Root>
            <Breadcrumb.List>
              <Breadcrumb.Item class="hidden md:block">
                <span>{i18nManager.t('control.settings')}</span>
              </Breadcrumb.Item>
              <Breadcrumb.Separator class="hidden md:block" />
              <Breadcrumb.Item>
                <Breadcrumb.Page>{activeLabel}</Breadcrumb.Page>
              </Breadcrumb.Item>
            </Breadcrumb.List>
          </Breadcrumb.Root>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={i18nManager.t('control.close')}
            data-testid="settings-close-button"
            onclick={() => (open = false)}
          >
            <XIcon aria-hidden="true" />
          </Button>
        </header>

        <div class="min-h-0 flex-1 overflow-y-auto p-4 sm:p-6">
          {#if activeSection === 'general'}
            <section class="mx-auto max-w-xl" aria-label={i18nManager.t('settings.general.title')}>
              <Field.Group>
                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="language-select" class="text-sm">
                      {i18nManager.t('settings.language.title')}
                    </Field.Label>
                    <Field.Description class="text-xs">{i18nManager.t('settings.language.desc')}</Field.Description>
                  </Field.Content>
                  <Select.Root type="single" bind:value={i18nManager.language}>
                    <Select.Trigger id="language-select" class="w-full text-xs sm:w-44">
                      <span class="truncate">{selectedLanguageLabel}</span>
                    </Select.Trigger>
                    <Select.Content>
                      <Select.Group>
                        {#each languageOptions as option (option.value)}
                          <Select.Item value={option.value} label={option.label} />
                        {/each}
                      </Select.Group>
                    </Select.Content>
                  </Select.Root>
                </Field.Field>
              </Field.Group>
            </section>
          {:else}
            <section class="mx-auto max-w-xl" aria-label={i18nManager.t('settings.appearance.title')}>
              <Field.Group>
                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="theme-select" class="text-sm">
                      {i18nManager.t('settings.appearance.select')}
                    </Field.Label>
                    <Field.Description class="text-xs">{i18nManager.t('settings.appearance.desc')}</Field.Description>
                  </Field.Content>
                  <Select.Root type="single" bind:value={themeManager.theme}>
                    <Select.Trigger id="theme-select" class="w-full text-xs sm:w-44">
                      <span class="truncate">{selectedThemeLabel}</span>
                    </Select.Trigger>
                    <Select.Content>
                      <Select.Group>
                        {#each themeOptions as option (option.value)}
                          <Select.Item value={option.value} label={option.label} />
                        {/each}
                      </Select.Group>
                    </Select.Content>
                  </Select.Root>
                </Field.Field>
              </Field.Group>
            </section>
          {/if}
        </div>
      </main>
    </Sidebar.Provider>
  </Dialog.Content>
</Dialog.Root>
