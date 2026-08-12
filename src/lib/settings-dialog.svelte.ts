class SettingsDialogManager {
  open = $state(false);

  show() {
    this.open = true;
  }
}

export const settingsDialogManager = new SettingsDialogManager();
