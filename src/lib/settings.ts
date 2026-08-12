import { logWarn } from '$lib/logger';

export type Settings = {
  theme?: string;
  language?: string;
  autoCheckUpdates?: boolean;
};

export type SettingKey = keyof Settings;

let settingsPromise: Promise<Settings> | undefined;

function legacyLocalSettings(): Settings {
  if (typeof window === 'undefined') return {};

  const theme = window.localStorage.getItem('theme') ?? undefined;
  const language = window.localStorage.getItem('language') ?? undefined;
  const autoCheckUpdates =
    window.localStorage.getItem('harness-lens:auto-check-updates') ??
    window.localStorage.getItem('codex-timeline:auto-check-updates');
  return {
    theme,
    language,
    autoCheckUpdates: autoCheckUpdates === null ? undefined : autoCheckUpdates !== 'false',
  };
}

async function loadDesktopSettings(): Promise<Settings> {
  const { invoke, isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) return legacyLocalSettings();

  const saved = await invoke<Settings>('load_settings');
  const legacy = legacyLocalSettings();
  const migrated: Settings = { ...saved };

  for (const key of Object.keys(legacy) as SettingKey[]) {
    const legacyValue = legacy[key];
    if (saved[key] !== undefined || legacyValue === undefined) continue;

    await invoke('save_setting', { key, value: legacyValue });
    if (key === 'theme') migrated.theme = legacyValue as string;
    if (key === 'language') migrated.language = legacyValue as string;
    if (key === 'autoCheckUpdates') migrated.autoCheckUpdates = legacyValue as boolean;
  }

  window.localStorage.removeItem('theme');
  window.localStorage.removeItem('language');
  window.localStorage.removeItem('harness-lens:auto-check-updates');
  window.localStorage.removeItem('codex-timeline:auto-check-updates');

  return migrated;
}

export function loadSettings(): Promise<Settings> {
  settingsPromise ??= loadDesktopSettings().catch((error) => {
    logWarn('Failed to load desktop settings', error);
    return legacyLocalSettings();
  });
  return settingsPromise;
}

export function saveSetting<K extends SettingKey>(key: K, value: Settings[K]): void {
  void (async () => {
    const { invoke, isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) {
      if (typeof window !== 'undefined' && value !== undefined) {
        window.localStorage.setItem(key, String(value));
      }
      return;
    }

    await invoke('save_setting', { key, value });
    const settings = await loadSettings();
    settings[key] = value;
  })().catch((error) => logWarn(`Failed to save ${key} setting`, error));
}
