import { logError } from '$lib/logger';
import { loadSettings, saveSetting } from '$lib/settings';

export type Theme = 'system' | 'light' | 'dark';

class ThemeManager {
  #theme = $state<Theme>('system');
  isDarkMode = $state(false);

  get theme() {
    return this.#theme;
  }

  set theme(value: Theme) {
    this.#theme = value;
    saveSetting('theme', value);
    void this.updateTheme();
  }

  constructor() {
    if (typeof window === 'undefined') {
      return;
    }

    void this.updateTheme();

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', () => {
      if (this.theme === 'system') {
        void this.updateTheme();
      }
    });
  }

  async init() {
    const savedTheme = (await loadSettings()).theme as Theme | undefined;
    if (savedTheme === 'light' || savedTheme === 'dark' || savedTheme === 'system') {
      this.#theme = savedTheme;
    }

    void this.updateTheme();
  }

  toggleTheme() {
    if (this.theme === 'system') {
      this.setTheme('light');
    } else if (this.theme === 'light') {
      this.setTheme('dark');
    } else {
      this.setTheme('system');
    }
  }

  setTheme(theme: Theme) {
    this.theme = theme;
  }

  async updateTheme() {
    if (typeof window === 'undefined') {
      return;
    }
    const dark =
      this.theme === 'system'
        ? window.matchMedia('(prefers-color-scheme: dark)').matches
        : this.theme === 'dark';

    this.isDarkMode = dark;
    document.documentElement.classList.toggle('dark', dark);
    document.documentElement.classList.toggle('light', !dark);

    try {
      const { invoke, isTauri } = await import('@tauri-apps/api/core');
      if (!isTauri()) return;
      await invoke('set_window_theme', { isDark: dark });
    } catch (err) {
      logError('Failed to sync native window theme', err);
    }
  }
}

export const themeManager = new ThemeManager();
