<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { i18nManager } from '$lib/i18n.svelte';

  // Keep the in-app link aligned with the canonical GitHub repository URL.
  const GITHUB_URL = 'https://github.com/sundegan/harness-lens';

  let appVersion = $state('');
  let isOpen = $state(false);

  const close = () => {
    isOpen = false;
  };

  const openGitHub = async () => {
    await openUrl(GITHUB_URL);
  };

  onMount(() => {
    const handleKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        close();
      }
    };

    window.addEventListener('keydown', handleKeydown);

    void getVersion()
      .then((version) => {
        appVersion = version;
      })
      .catch(() => {
        appVersion = '';
      });

    const unlisten = listen('show-about', () => {
      isOpen = true;
    });

    return () => {
      window.removeEventListener('keydown', handleKeydown);
      void unlisten.then((dispose) => dispose());
    };
  });
</script>

{#if isOpen}
  <div class="about-backdrop" role="presentation" onclick={close}>
    <dialog
      class="about-dialog"
      open
      aria-modal="true"
      aria-labelledby="about-title"
      onclick={(event) => event.stopPropagation()}
    >
      <button class="about-close" type="button" aria-label={i18nManager.t('about.close')} onclick={close}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M18 6 6 18M6 6l12 12" />
        </svg>
      </button>

      <div class="about-hero">
        <img class="about-icon" src="/app-icon.png" alt="" />
        <div class="about-copy">
          <h2 id="about-title">{i18nManager.t('about.title')}</h2>
          <p>{i18nManager.t('about.desc')}</p>
        </div>
      </div>

      <div class="about-meta">
        <div class="about-row">
          <span>{i18nManager.t('about.version')}</span>
          <strong>{appVersion || i18nManager.t('about.unknown')}</strong>
        </div>
      </div>

      <button class="about-row about-github" type="button" onclick={openGitHub}>
        <span>GitHub</span>
        <strong>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M12 .5A11.5 11.5 0 0 0 .5 12.28c0 5.2 3.36 9.6 8.02 11.16.58.11.79-.26.79-.57v-2.02c-3.26.72-3.95-1.61-3.95-1.61-.53-1.39-1.3-1.76-1.3-1.76-1.07-.75.08-.74.08-.74 1.18.09 1.8 1.25 1.8 1.25 1.05 1.85 2.76 1.32 3.43 1 .11-.78.41-1.32.75-1.62-2.6-.3-5.33-1.33-5.33-5.94 0-1.31.46-2.38 1.21-3.22-.12-.31-.53-1.54.12-3.2 0 0 .99-.33 3.24 1.23a10.95 10.95 0 0 1 5.9 0c2.25-1.56 3.24-1.23 3.24-1.23.65 1.66.24 2.89.12 3.2.75.84 1.21 1.91 1.21 3.22 0 4.62-2.73 5.64-5.34 5.94.42.37.8 1.1.8 2.22v3.28c0 .31.21.68.8.57a11.77 11.77 0 0 0 8.01-11.16A11.5 11.5 0 0 0 12 .5Z"
            />
          </svg>
          {i18nManager.t('about.star')}
        </strong>
      </button>

      <p class="about-footer">{i18nManager.t('about.copyright')}</p>
    </dialog>
  </div>
{/if}

<style>
  .about-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10000;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--backdrop-bg);
    backdrop-filter: blur(10px);
  }

  .about-dialog {
    position: relative;
    box-sizing: border-box;
    width: min(420px, 100%);
    padding: 24px;
    border: 1px solid var(--dialog-border);
    border-radius: 14px;
    color: var(--text-color);
    background: var(--dialog-bg);
    box-shadow: var(--dialog-shadow);
    text-align: left;
  }

  .about-close {
    position: absolute;
    top: 14px;
    right: 14px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 1px solid transparent;
    border-radius: 8px;
    color: var(--close-btn-color);
    background: transparent;
    cursor: pointer;
  }

  .about-close:hover {
    border-color: var(--close-btn-hover-border);
    color: var(--text-color);
    background: var(--close-btn-hover-bg);
  }

  .about-close svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-width: 2.2;
  }

  .about-hero {
    display: flex;
    gap: 16px;
    align-items: flex-start;
    padding-right: 32px;
  }

  .about-icon {
    width: 68px;
    height: 68px;
    flex: 0 0 auto;
  }

  .about-copy h2 {
    margin: 2px 0 0;
    font-size: 24px;
    font-weight: 800;
    line-height: 1.18;
  }

  .about-copy p {
    margin: 6px 0 0;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.45;
    white-space: nowrap;
  }

  .about-meta {
    display: grid;
    gap: 10px;
    margin-top: 20px;
  }

  .about-row {
    display: flex;
    box-sizing: border-box;
    height: 38px;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 0 12px;
    border: 1px solid var(--row-border);
    border-radius: 8px;
    background: var(--row-bg);
  }

  .about-row span {
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 700;
  }

  .about-row strong {
    min-width: 0;
    color: var(--text-color);
    font-size: 13px;
    font-weight: 700;
    line-height: 1.35;
  }

  .about-github {
    width: 100%;
    margin-top: 12px;
    cursor: pointer;
    font: inherit;
  }

  .about-github:hover {
    border-color: var(--github-hover-border);
    background: var(--github-hover-bg);
  }

  .about-github:hover strong {
    color: var(--github-hover-text);
  }

  .about-github svg {
    width: 17px;
    height: 17px;
    flex: 0 0 auto;
    fill: currentColor;
  }

  .about-github strong {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  .about-footer {
    margin: 18px 0 0;
    color: var(--footer-color);
    font-size: 12px;
    text-align: center;
  }

  @media (max-width: 520px) {
    .about-dialog {
      padding: 24px;
    }

    .about-hero {
      flex-direction: column;
      align-items: flex-start;
      padding-right: 30px;
    }

    .about-copy p {
      white-space: normal;
    }
  }
</style>
