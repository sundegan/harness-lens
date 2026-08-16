import { loadSettings, saveSetting } from '$lib/settings';

const DEFAULT_MINIMIZE_TO_TRAY_ON_CLOSE = true;

class WindowBehaviorManager {
  minimizeToTrayOnClose = $state(DEFAULT_MINIMIZE_TO_TRAY_ON_CLOSE);
  ready = $state(false);
  busy = $state(false);
  #initialized = false;

  async init() {
    if (this.#initialized) return;
    this.#initialized = true;

    try {
      const savedValue = (await loadSettings()).minimizeToTrayOnClose;
      if (typeof savedValue === 'boolean') {
        this.minimizeToTrayOnClose = savedValue;
      }
    } finally {
      this.ready = true;
    }
  }

  async setMinimizeToTrayOnClose(enabled: boolean) {
    if (!this.ready || this.busy) return;

    const previous = this.minimizeToTrayOnClose;
    this.busy = true;
    this.minimizeToTrayOnClose = enabled;
    try {
      const saved = await saveSetting('minimizeToTrayOnClose', enabled);
      if (!saved) this.minimizeToTrayOnClose = previous;
    } finally {
      this.busy = false;
    }
  }
}

export const windowBehaviorManager = new WindowBehaviorManager();
