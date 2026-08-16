import { logWarn } from '$lib/logger';

class AutoStartManager {
  enabled = $state(false);
  available = $state<boolean | null>(null);
  busy = $state(false);
  #initialized = false;

  async init() {
    if (this.#initialized || typeof window === 'undefined') return;
    this.#initialized = true;

    try {
      const { isTauri } = await import('@tauri-apps/api/core');
      if (!isTauri()) {
        this.available = false;
        return;
      }

      const { isEnabled } = await import('@tauri-apps/plugin-autostart');
      this.enabled = await isEnabled();
      this.available = true;
    } catch (error) {
      this.available = false;
      logWarn('Failed to load auto-start status', error);
    }
  }

  async setEnabled(enabled: boolean) {
    if (this.available !== true || this.busy) return;

    const previous = this.enabled;
    this.busy = true;
    try {
      const { enable, disable } = await import('@tauri-apps/plugin-autostart');
      if (enabled) await enable();
      else await disable();
      this.enabled = enabled;
    } catch (error) {
      this.enabled = previous;
      logWarn('Failed to update auto-start setting', error);
    } finally {
      this.busy = false;
    }
  }
}

export const autoStartManager = new AutoStartManager();
