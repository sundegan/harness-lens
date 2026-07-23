import { getVersion } from '@tauri-apps/api/app';

type UpdateStatus = 'idle' | 'checking' | 'available' | 'installing' | 'ready' | 'latest' | 'error';

type AppUpdate = {
  version?: string;
  currentVersion?: string;
  date?: string;
  body?: string;
  downloadAndInstall: () => Promise<void>;
};

const AUTO_CHECK_DELAY_MS = 1200;
const AUTO_CHECK_UPDATES_KEY = 'harness-lens:auto-check-updates';
const LEGACY_AUTO_CHECK_UPDATES_KEY = 'codex-timeline:auto-check-updates';
const DEFAULT_AUTO_CHECK_UPDATES = true;

function isMockAppUpdateEnabled() {
  // @ts-ignore
  if (typeof window === 'undefined') return false;
  // @ts-ignore
  if (!import.meta.env.DEV && import.meta.env.VITE_WDIO_TAURI !== '1') return false;
  // @ts-ignore
  if (import.meta.env.VITE_MOCK_APP_UPDATE === '1') return true;

  const params = new URLSearchParams(window.location.search);
  return params.get('mockUpdate') === '1' || window.localStorage.getItem('mock-app-update') === '1';
}

class AppUpdateManager {
  currentVersion = $state('');
  latestVersion = $state('');
  releaseNotes = $state('');
  publishedAt = $state('');
  status = $state<UpdateStatus>('idle');
  error = $state('');
  autoCheckUpdates = $state(true);
  hasUpdate = $derived(this.status === 'available');
  isChecking = $derived(this.status === 'checking' || this.status === 'installing');
  update = $state<AppUpdate | null>(null);

  #initialized = false;

  async init() {
    if (this.#initialized || typeof window === 'undefined') {
      return;
    }

    this.#initialized = true;
    const storedPreference = localStorage.getItem(AUTO_CHECK_UPDATES_KEY) ?? localStorage.getItem(LEGACY_AUTO_CHECK_UPDATES_KEY);
    this.autoCheckUpdates = (storedPreference ?? String(DEFAULT_AUTO_CHECK_UPDATES)) !== 'false';

    if (storedPreference !== null && localStorage.getItem(AUTO_CHECK_UPDATES_KEY) === null) {
      localStorage.setItem(AUTO_CHECK_UPDATES_KEY, storedPreference);
    }

    try {
      this.currentVersion = await getVersion();
    } catch {
      this.currentVersion = '';
    }

    if (this.autoCheckUpdates) {
      window.setTimeout(() => {
        void this.checkForUpdates({ quiet: true });
      }, AUTO_CHECK_DELAY_MS);
    }
  }

  setAutoCheckUpdates(enabled: boolean) {
    this.autoCheckUpdates = enabled;
    localStorage.setItem(AUTO_CHECK_UPDATES_KEY, String(enabled));
  }

  async checkForUpdates(options: { quiet?: boolean } = {}) {
    if (this.status === 'checking') {
      return;
    }

    this.status = 'checking';
    this.error = '';

    try {
      if (isMockAppUpdateEnabled()) {
        await new Promise((resolve) => setTimeout(resolve, AUTO_CHECK_DELAY_MS));
        this.update = {
          version: '9.9.9-dev',
          body: 'Local mock update for testing HarnessLens updates.',
          date: new Date().toISOString(),
          downloadAndInstall: async () => {
            await new Promise((resolve) => setTimeout(resolve, 1500));
          },
        };
        this.currentVersion = this.currentVersion || '0.1.0';
        this.latestVersion = '9.9.9-dev';
        this.releaseNotes = 'Local mock update for testing HarnessLens updates.';
        this.publishedAt = new Date().toISOString();
        this.status = 'available';
        return;
      }

      const { check } = await import('@tauri-apps/plugin-updater');
      const update = (await check({ timeout: 30_000 })) as AppUpdate | null;

      if (!update) {
        this.latestVersion = '';
        this.releaseNotes = '';
        this.publishedAt = '';
        this.update = null;
        this.status = 'latest';
        return;
      }

      this.update = update;
      this.currentVersion = update.currentVersion ?? this.currentVersion;
      this.latestVersion = update.version ?? '';
      this.releaseNotes = update.body ?? '';
      this.publishedAt = update.date ?? '';
      this.status = 'available';
    } catch (error) {
      this.status = options.quiet ? 'idle' : 'error';
      this.error = options.quiet ? '' : error instanceof Error ? error.message : String(error);
    }
  }

  async installUpdate() {
    if (!this.update) {
      return;
    }

    this.status = 'installing';
    this.error = '';

    try {
      await this.update.downloadAndInstall();
      this.status = 'ready';
    } catch (error) {
      this.status = 'error';
      this.error = error instanceof Error ? error.message : String(error);
    }
  }
}

export const appUpdateManager = new AppUpdateManager();
