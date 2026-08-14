import { getVersion } from '@tauri-apps/api/app';
import { loadSettings, saveSetting } from '$lib/settings';

type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'ready'
  | 'latest'
  | 'error';

type DownloadEvent =
  | { event: 'Started'; data: { contentLength?: number } }
  | { event: 'Progress'; data: { chunkLength: number } }
  | { event: 'Finished' };

type AppUpdate = {
  currentVersion?: string;
  version?: string;
  download: (onEvent?: (event: DownloadEvent) => void) => Promise<void>;
  install: () => Promise<void>;
};

const AUTO_CHECK_DELAY_MS = 1200;
const AUTO_CHECK_INTERVAL_OPTIONS = [12, 24, 72, 168] as const;
type AutoCheckIntervalHours = (typeof AUTO_CHECK_INTERVAL_OPTIONS)[number];
const DEFAULT_AUTO_CHECK_INTERVAL_HOURS: AutoCheckIntervalHours = 12;
const DEFAULT_AUTO_CHECK_UPDATES = true;

type MockUpdateMode = 'update' | 'latest' | null;

function mockUpdateMode(): MockUpdateMode {
  if (typeof window === 'undefined') return null;
  if (!import.meta.env.DEV && import.meta.env.VITE_WDIO_TAURI !== '1') return null;

  const params = new URLSearchParams(window.location.search);
  if (params.get('mockUpdate') === 'latest') return 'latest';
  if (params.get('mockUpdate') === '1') return 'update';
  if (window.localStorage.getItem('mock-app-update') === '1') return 'update';
  if (import.meta.env.VITE_MOCK_APP_UPDATE === '1') return 'update';
  return null;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function normalizeAutoCheckInterval(value: number | undefined): AutoCheckIntervalHours {
  return AUTO_CHECK_INTERVAL_OPTIONS.includes(value as AutoCheckIntervalHours)
    ? (value as AutoCheckIntervalHours)
    : DEFAULT_AUTO_CHECK_INTERVAL_HOURS;
}

class AppUpdateManager {
  currentVersion = $state('');
  latestVersion = $state('');
  status = $state<UpdateStatus>('idle');
  error = $state('');
  autoCheckUpdates = $state(DEFAULT_AUTO_CHECK_UPDATES);
  autoCheckIntervalHours = $state<AutoCheckIntervalHours>(DEFAULT_AUTO_CHECK_INTERVAL_HOURS);
  downloadProgress = $state(0);
  downloadedBytes = $state(0);
  contentLength = $state<number | undefined>();
  update = $state<AppUpdate | null>(null);
  dialogOpen = $state(false);

  #manualCheckRequested = false;
  #autoCheckTimeout: number | undefined;
  #autoCheckInterval: number | undefined;
  hasUpdate = $derived(
    this.update !== null &&
      (this.status === 'available' ||
        this.status === 'downloading' ||
        this.status === 'installing' ||
        this.status === 'ready')
  );
  isBusy = $derived(
    this.status === 'checking' || this.status === 'downloading' || this.status === 'installing'
  );

  #initialized = false;

  async init() {
    if (this.#initialized || typeof window === 'undefined') {
      return;
    }

    this.#initialized = true;
    const settings = await loadSettings();
    this.autoCheckUpdates = settings.autoCheckUpdates ?? DEFAULT_AUTO_CHECK_UPDATES;
    this.autoCheckIntervalHours = normalizeAutoCheckInterval(settings.autoCheckIntervalHours);

    try {
      this.currentVersion = await getVersion();
    } catch {
      this.currentVersion = '';
    }

    this.scheduleAutomaticChecks();
  }

  setAutoCheckUpdates(enabled: boolean) {
    this.autoCheckUpdates = enabled;
    saveSetting('autoCheckUpdates', enabled);

    if (enabled) this.scheduleAutomaticChecks();
    else this.clearAutomaticChecks();
  }

  setAutoCheckInterval(value: string) {
    const hours = Number(value);
    if (!AUTO_CHECK_INTERVAL_OPTIONS.includes(hours as AutoCheckIntervalHours)) return;

    this.autoCheckIntervalHours = hours as AutoCheckIntervalHours;
    saveSetting('autoCheckIntervalHours', this.autoCheckIntervalHours);
    if (this.autoCheckUpdates) this.scheduleAutomaticChecks();
  }

  private scheduleAutomaticChecks() {
    if (typeof window === 'undefined') return;

    this.clearAutomaticChecks();
    if (!this.autoCheckUpdates) return;

    this.#autoCheckTimeout = window.setTimeout(() => {
      this.#autoCheckTimeout = undefined;
      void this.checkForUpdates({ quiet: true });
    }, AUTO_CHECK_DELAY_MS);
    this.#autoCheckInterval = window.setInterval(
      () => {
        void this.checkForUpdates({ quiet: true });
      },
      this.autoCheckIntervalHours * 60 * 60 * 1000
    );
  }

  private clearAutomaticChecks() {
    if (typeof window === 'undefined') return;

    if (this.#autoCheckTimeout !== undefined) {
      window.clearTimeout(this.#autoCheckTimeout);
      this.#autoCheckTimeout = undefined;
    }
    if (this.#autoCheckInterval !== undefined) {
      window.clearInterval(this.#autoCheckInterval);
      this.#autoCheckInterval = undefined;
    }
  }

  async checkForUpdates(options: { quiet?: boolean } = {}) {
    const quiet = options.quiet ?? false;
    if (this.status === 'checking') {
      if (!quiet) this.#manualCheckRequested = true;
      return;
    }
    if (
      this.status === 'downloading' ||
      this.status === 'installing' ||
      this.status === 'ready' ||
      (quiet && this.status === 'available')
    ) {
      return;
    }

    this.status = 'checking';
    this.error = '';

    try {
      const mockMode = mockUpdateMode();
      if (mockMode === 'latest') {
        await new Promise((resolve) => window.setTimeout(resolve, 250));
        this.latestVersion = '';
        this.update = null;
        this.dialogOpen = false;
        this.status = 'latest';
      } else if (mockMode === 'update') {
        await new Promise((resolve) => window.setTimeout(resolve, 250));
        this.setMockUpdate();
      } else {
        const { check } = await import('@tauri-apps/plugin-updater');
        // Keep the target selection in the existing GitHub Release configuration.
        const update = (await check({ timeout: 30_000 })) as AppUpdate | null;

        if (!update) {
          this.latestVersion = '';
          this.update = null;
          this.dialogOpen = false;
          this.status = 'latest';
          this.#manualCheckRequested = false;
          return;
        }

        this.update = update;
        this.currentVersion = update.currentVersion ?? this.currentVersion;
        this.latestVersion = update.version ?? '';
        this.status = 'available';
      }
      this.#manualCheckRequested = false;
    } catch (error) {
      const reportResult = !quiet || this.#manualCheckRequested;
      this.#manualCheckRequested = false;
      this.status = reportResult ? 'error' : 'idle';
      this.error = reportResult ? errorMessage(error) : '';
    }
  }

  openUpdateDialog() {
    this.dialogOpen = true;
    if (this.status === 'available') void this.downloadUpdate();
  }

  async downloadUpdate() {
    if (!this.update || this.status === 'downloading') return;

    this.status = 'downloading';
    this.error = '';
    this.downloadProgress = 0;
    this.downloadedBytes = 0;
    this.contentLength = undefined;

    try {
      if (mockUpdateMode() === 'update') {
        this.contentLength = 4_000_000;
        for (const progress of [12, 35, 61, 84, 100]) {
          await new Promise((resolve) => window.setTimeout(resolve, 140));
          this.downloadProgress = progress;
          this.downloadedBytes = Math.round((this.contentLength * progress) / 100);
        }
      } else {
        await this.update.download((event) => {
          if (event.event === 'Started') {
            this.contentLength = event.data.contentLength;
            this.downloadProgress = 0;
            this.downloadedBytes = 0;
          } else if (event.event === 'Progress') {
            this.downloadedBytes += event.data.chunkLength;
            this.downloadProgress = this.contentLength
              ? Math.min(100, Math.round((this.downloadedBytes / this.contentLength) * 100))
              : 0;
          } else if (event.event === 'Finished') {
            this.downloadProgress = 100;
          }
        });
      }

      this.status = 'ready';
    } catch (error) {
      this.status = 'error';
      this.error = errorMessage(error);
    }
  }

  async restartApp() {
    if (!this.update || this.status !== 'ready') return;

    try {
      this.status = 'installing';
      if (mockUpdateMode() === 'update') {
        await new Promise((resolve) => window.setTimeout(resolve, 750));
      }
      await this.update.install();
      this.dialogOpen = false;

      if (import.meta.env.DEV || mockUpdateMode() !== null) {
        window.location.reload();
        return;
      }

      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('restart_app');
    } catch (error) {
      this.status = 'error';
      this.error = errorMessage(error);
      this.dialogOpen = true;
    }
  }

  private setMockUpdate() {
    this.update = {
      currentVersion: this.currentVersion || '0.1.0',
      version: '9.9.9-dev',
      download: async () => undefined,
      install: async () => undefined,
    };
    this.currentVersion ||= '0.1.0';
    this.latestVersion = '9.9.9-dev';
    this.status = 'available';
  }
}

export const appUpdateManager = new AppUpdateManager();
