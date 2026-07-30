<script lang="ts">
import { i18nManager } from '$lib/i18n.svelte';
import AboutDialog from '$lib/menu/AboutDialog.svelte';
</script>

<main>
  <h1>{i18nManager.t('main.title')}</h1>
  <p>{i18nManager.t('main.desc')}</p>
  <AboutDialog />
</main>

<style>
  main {
    display: flex;
    box-sizing: border-box;
    min-height: 100%;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px 32px 96px;
    text-align: center;
  }

  h1 {
    margin: 0 0 4px;
    font-size: 28px;
    line-height: 1.2;
  }

  p {
    margin: 0;
    color: var(--text-muted);
  }
</style>
