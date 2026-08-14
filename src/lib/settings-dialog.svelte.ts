class SettingsDialogManager {
  open = $state(false);
  activeSection = $state<'general' | 'appearance' | 'updates'>('general');

  show(section: 'general' | 'appearance' | 'updates' = 'general') {
    this.activeSection = section;
    this.open = true;
  }
}

export const settingsDialogManager = new SettingsDialogManager();
