import { invoke } from '@tauri-apps/api/core';

export type Theme = 'system' | 'light' | 'dark';

class ThemeManager {
  #theme = $state<Theme>('system');
  isDarkMode = $state(false);

  get theme() {
    return this.#theme;
  }

  set theme(value: Theme) {
    this.#theme = value;
    if (typeof window !== 'undefined') {
      localStorage.setItem('theme', value);
    }
    void this.updateTheme();
  }

  constructor() {
    if (typeof window === 'undefined') {
      return;
    }

    const savedTheme = localStorage.getItem('theme') as Theme;
    if (savedTheme === 'light' || savedTheme === 'dark' || savedTheme === 'system') {
      this.#theme = savedTheme;
    }

    void this.updateTheme();

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', () => {
      if (this.theme === 'system') {
        void this.updateTheme();
      }
    });
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
      await invoke('set_window_theme', { isDark: dark });
    } catch (err) {
      console.error('Failed to sync native window theme:', err);
    }
  }
}

export const themeManager = new ThemeManager();
