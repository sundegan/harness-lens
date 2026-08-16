import { logWarn } from '$lib/logger';

export type DatabaseRuntimeStatus = 'pending' | 'ready' | 'failed';

export interface SyncStatus {
  provider: string;
  sourceId: string;
  status: string;
  phase: string;
  totalFiles: number;
  processedFiles: number;
  processedLines: number;
  estimatedTotalLines: number | null;
  currentFile: string | null;
  currentLine: number;
  estimatedRemainingMs: number | null;
  lastError: string | null;
  updatedAtMs: number;
}

function isIncrementalSync(status: SyncStatus): boolean {
  return (
    status.status === 'syncing' &&
    (status.phase === 'incremental' || status.phase === 'watching')
  );
}

function isWatching(status: SyncStatus): boolean {
  return status.status === 'ready' && status.phase === 'watching';
}

class SyncStatusManager {
  statuses = $state.raw<SyncStatus[]>([]);
  databaseStatus = $state<DatabaseRuntimeStatus | null>(null);
  #initialized = false;
  #refreshSequence = 0;

  activeStatuses = $derived(this.statuses.filter((status) => status.status === 'syncing'));
  failedStatuses = $derived(
    this.statuses.filter((status) => status.status === 'error' || status.status === 'unavailable')
  );
  watchingStatuses = $derived(
    this.statuses.filter((status) => status.status === 'ready' && status.phase === 'watching')
  );
  lastUpdatedAtMs = $derived.by<number | null>(() => {
    const hasInitialScan = this.statuses.some(
      (status) => status.status === 'syncing' && status.phase === 'initial_scan'
    );
    const hasIncrementalSync = this.statuses.some(isIncrementalSync);
    const hasWatching = this.statuses.some(isWatching);
    const hasFailedStatus = this.statuses.some(
      (status) => status.status === 'error' || status.status === 'unavailable'
    );
    if (
      this.databaseStatus !== 'ready' ||
      hasInitialScan ||
      (!hasIncrementalSync && hasFailedStatus) ||
      (!hasIncrementalSync && !hasWatching)
    ) {
      return null;
    }

    let latest: number | null = null;
    for (const status of this.statuses) {
      if (
        (!isIncrementalSync(status) && !isWatching(status)) ||
        status.updatedAtMs <= 0 ||
        (latest !== null && status.updatedAtMs <= latest)
      ) {
        continue;
      }
      latest = status.updatedAtMs;
    }
    return latest;
  });

  async init() {
    if (this.#initialized || typeof window === 'undefined') return;
    this.#initialized = true;
    await this.refresh();
  }

  async refresh() {
    if (typeof window === 'undefined') return;
    const sequence = ++this.#refreshSequence;
    try {
      const { invoke, isTauri } = await import('@tauri-apps/api/core');
      if (!isTauri()) {
        if (sequence === this.#refreshSequence) {
          this.databaseStatus = 'ready';
          this.statuses = [];
        }
        return;
      }
      const databaseStatus = await invoke<DatabaseRuntimeStatus>('get_database_runtime_status');
      if (sequence === this.#refreshSequence) this.databaseStatus = databaseStatus;
      const statuses = await invoke<SyncStatus[]>('get_sync_status');
      if (sequence === this.#refreshSequence) {
        this.databaseStatus = 'ready';
        this.statuses = statuses;
      }
    } catch (error) {
      if (sequence === this.#refreshSequence) this.databaseStatus = 'failed';
      logWarn('Failed to refresh sync status', error);
    }
  }
}

export const syncStatusManager = new SyncStatusManager();
