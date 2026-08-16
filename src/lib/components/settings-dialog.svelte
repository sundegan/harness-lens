<script lang="ts">
import MonitorCogIcon from '@lucide/svelte/icons/monitor-cog';
import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
import SettingsIcon from '@lucide/svelte/icons/settings';
import XIcon from '@lucide/svelte/icons/x';
import { autoStartManager } from '$lib/autostart.svelte';
import * as Breadcrumb from '$lib/components/ui/breadcrumb/index.js';
import { Button } from '$lib/components/ui/button/index.js';
import { Checkbox } from '$lib/components/ui/checkbox/index.js';
import * as Dialog from '$lib/components/ui/dialog/index.js';
import * as Field from '$lib/components/ui/field/index.js';
import * as Select from '$lib/components/ui/select/index.js';
import * as Sidebar from '$lib/components/ui/sidebar/index.js';
import { i18nManager } from '$lib/i18n.svelte';
import { settingsDialogManager } from '$lib/settings-dialog.svelte';
import { themeManager } from '$lib/theme.svelte';
import { appUpdateManager } from '$lib/update.svelte';
import { windowBehaviorManager } from '$lib/window-behavior.svelte';

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

const updateIntervalOptions = $derived([
  { value: '12', label: i18nManager.t('settings.updates.interval.12_hours') },
  { value: '24', label: i18nManager.t('settings.updates.interval.24_hours') },
  { value: '72', label: i18nManager.t('settings.updates.interval.3_days') },
  { value: '168', label: i18nManager.t('settings.updates.interval.7_days') },
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
  {
    value: 'updates' as const,
    label: i18nManager.t('nav.updates'),
    icon: RefreshCwIcon,
  },
]);

let { open = $bindable(false) }: { open?: boolean } = $props();
const activeSection = $derived(settingsDialogManager.activeSection);

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
const selectedUpdateIntervalLabel = $derived(
  updateIntervalOptions.find(
    (option) => option.value === String(appUpdateManager.autoCheckIntervalHours)
  )?.label ?? ''
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
                      data-testid={`settings-nav-${item.value}`}
                      onclick={() => (settingsDialogManager.activeSection = item.value)}
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

                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="launch-at-login" class="text-sm">
                      {i18nManager.t('settings.general.launch_at_login')}
                    </Field.Label>
                    <Field.Description class="text-xs">
                      {i18nManager.t('settings.general.launch_at_login_desc')}
                    </Field.Description>
                  </Field.Content>
                  <Checkbox
                    id="launch-at-login"
                    data-testid="launch-at-login"
                    checked={autoStartManager.enabled}
                    disabled={autoStartManager.available !== true || autoStartManager.busy}
                    onCheckedChange={(checked) => void autoStartManager.setEnabled(checked === true)}
                  />
                </Field.Field>

                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="minimize-to-tray-on-close" class="text-sm">
                      {i18nManager.t('settings.general.minimize_to_tray_on_close')}
                    </Field.Label>
                    <Field.Description class="text-xs">
                      {i18nManager.t('settings.general.minimize_to_tray_on_close_desc')}
                    </Field.Description>
                  </Field.Content>
                  <Checkbox
                    id="minimize-to-tray-on-close"
                    data-testid="minimize-to-tray-on-close"
                    checked={windowBehaviorManager.minimizeToTrayOnClose}
                    disabled={!windowBehaviorManager.ready || windowBehaviorManager.busy}
                    onCheckedChange={(checked) =>
                      void windowBehaviorManager.setMinimizeToTrayOnClose(checked === true)}
                  />
                </Field.Field>
              </Field.Group>
            </section>
          {:else if activeSection === 'appearance'}
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
          {:else}
            <section class="mx-auto max-w-xl" aria-label={i18nManager.t('settings.updates.title')}>
              <Field.Group>
                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="current-app-version" class="text-sm">
                      {i18nManager.t('about.version')}
                    </Field.Label>
                  </Field.Content>
                  <output
                    id="current-app-version"
                    data-testid="current-app-version"
                    class="w-full text-xs font-medium text-muted-foreground sm:w-44 sm:text-right"
                  >
                    {appUpdateManager.currentVersion || i18nManager.t('about.unknown')}
                  </output>
                </Field.Field>

                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="auto-check-updates" class="text-sm">
                      {i18nManager.t('settings.updates.auto_check')}
                    </Field.Label>
                    <Field.Description class="text-xs">
                      {i18nManager.t('settings.updates.auto_check_desc')}
                    </Field.Description>
                  </Field.Content>
                  <Checkbox
                    id="auto-check-updates"
                    data-testid="auto-check-updates"
                    checked={appUpdateManager.autoCheckUpdates}
                    onCheckedChange={(checked) => appUpdateManager.setAutoCheckUpdates(checked)}
                  />
                </Field.Field>

                <Field.Field orientation="responsive" class="border-b py-3">
                  <Field.Content>
                    <Field.Label for="auto-check-interval" class="text-sm">
                      {i18nManager.t('settings.updates.interval')}
                    </Field.Label>
                    <Field.Description class="text-xs">
                      {i18nManager.t('settings.updates.interval_desc')}
                    </Field.Description>
                  </Field.Content>
                  <Select.Root
                    type="single"
                    value={String(appUpdateManager.autoCheckIntervalHours)}
                    onValueChange={(value) => appUpdateManager.setAutoCheckInterval(value)}
                    disabled={!appUpdateManager.autoCheckUpdates}
                  >
                    <Select.Trigger
                      id="auto-check-interval"
                      data-testid="auto-check-interval"
                      class="w-full text-xs sm:w-44"
                    >
                      <span class="truncate">{selectedUpdateIntervalLabel}</span>
                    </Select.Trigger>
                    <Select.Content>
                      <Select.Group>
                        {#each updateIntervalOptions as option (option.value)}
                          <Select.Item value={option.value} label={option.label} />
                        {/each}
                      </Select.Group>
                    </Select.Content>
                  </Select.Root>
                </Field.Field>

                <Field.Field
                  orientation="responsive"
                  class={appUpdateManager.hasUpdate ? 'py-3 settings-update-row' : 'py-3'}
                >
                  <Field.Content>
                    <Field.Label for="check-for-updates" class="text-sm">
                      {i18nManager.t('settings.updates.title')}
                    </Field.Label>
                    <Field.Description
                      class="text-xs"
                      role="status"
                      aria-live="polite"
                      data-testid="update-status"
                    >
                      {#if appUpdateManager.status === 'checking'}
                        {i18nManager.t('update.status.checking')}
                      {:else if appUpdateManager.status === 'downloading'}
                        {i18nManager.t('update.status.downloading')}
                      {:else if appUpdateManager.status === 'installing'}
                        {i18nManager.t('update.status.installing')}
                      {:else if appUpdateManager.status === 'ready'}
                        {i18nManager.t('update.status.ready')}
                      {:else if appUpdateManager.status === 'error'}
                        {i18nManager.t('update.status.error', { error: appUpdateManager.error })}
                      {:else if appUpdateManager.hasUpdate}
                        {i18nManager.t('update.status.available', { version: appUpdateManager.latestVersion })}
                      {:else if appUpdateManager.status === 'latest'}
                        {i18nManager.t('update.status.latest', { currentVersion: appUpdateManager.currentVersion })}
                      {:else}
                        {i18nManager.t('update.status.idle')}
                      {/if}
                    </Field.Description>
                  </Field.Content>
                  {#if appUpdateManager.hasUpdate}
                    <Button
                      size="sm"
                      class="settings-update-action"
                      onclick={() => appUpdateManager.openUpdateDialog()}
                    >
                      {appUpdateManager.status === 'downloading'
                        ? i18nManager.t('update.status.downloading')
                        : appUpdateManager.status === 'installing'
                          ? i18nManager.t('update.status.installing')
                        : appUpdateManager.status === 'ready'
                          ? i18nManager.t('update.action.restart')
                          : i18nManager.t('update.action.download')}
                    </Button>
                  {:else}
                    <Button
                      id="check-for-updates"
                      data-testid="check-for-updates"
                      variant="outline"
                      size="sm"
                      disabled={appUpdateManager.isBusy}
                      onclick={() => void appUpdateManager.checkForUpdates()}
                    >
                      {appUpdateManager.status === 'checking'
                        ? i18nManager.t('update.action.checking')
                        : i18nManager.t('update.action.check_now')}
                    </Button>
                  {/if}
                </Field.Field>
              </Field.Group>
            </section>
          {/if}
        </div>
      </main>
    </Sidebar.Provider>
  </Dialog.Content>
</Dialog.Root>

<style>
  :global(.settings-update-row) {
    border: 1px solid rgb(22 163 74 / 18%);
    border-radius: 8px;
    margin: 8px -14px;
    padding: 12px 14px;
    background: rgb(22 163 74 / 5%);
  }

  :global(.dark .settings-update-row) {
    border-color: rgb(74 222 128 / 22%);
    background: rgb(74 222 128 / 6%);
  }

  :global(.settings-update-action) {
    color: #16a34a;
    border-color: rgb(22 163 74 / 30%);
    background: rgb(22 163 74 / 8%);
    animation: settings-update-action-pulse 2.4s infinite ease-in-out;
  }

  @keyframes settings-update-action-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgb(22 163 74 / 35%);
    }
    50% {
      box-shadow: 0 0 0 5px rgb(22 163 74 / 0%);
      transform: scale(1.02);
    }
  }
</style>
