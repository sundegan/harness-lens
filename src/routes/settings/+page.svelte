<script lang="ts">
import { onMount } from 'svelte';
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { i18nManager } from '$lib/i18n.svelte';
import { settingsDialogManager } from '$lib/settings-dialog.svelte';

onMount(() => {
  settingsDialogManager.show();
  void goto(resolve('/', {}), { replaceState: true });
});
</script>

<svelte:head>
  <title>{i18nManager.t('control.settings')} | {i18nManager.t('main.title')}</title>
</svelte:head>

<main class="min-h-full bg-workspace"></main>
