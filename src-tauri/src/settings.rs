use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data_paths;

static SETTINGS_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Option<String>,
    pub language: Option<String>,
    pub auto_check_updates: Option<bool>,
}

fn read_settings() -> Result<Settings, String> {
    read_settings_from_path(&data_paths::settings_path())
}

fn read_settings_from_path(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }

    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    match serde_json::from_str(&contents) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            recover_corrupted_settings(path, &error);
            Ok(Settings::default())
        }
    }
}

fn write_settings_to_path(path: &Path, settings: &Settings) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "settings path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

fn write_settings(settings: &Settings) -> Result<(), String> {
    write_settings_to_path(&data_paths::settings_path(), settings)
}

fn corrupt_backup_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("settings.corrupt-{timestamp}.json"))
}

fn recover_corrupted_settings(path: &Path, parse_error: &serde_json::Error) {
    let backup_path = corrupt_backup_path(path);
    match fs::rename(path, &backup_path) {
        Ok(()) => log::warn!(
            "settings file was invalid JSON ({parse_error}); moved it to {}",
            backup_path.display()
        ),
        Err(error) => log::warn!(
            "settings file was invalid JSON ({parse_error}); failed to back it up: {error}"
        ),
    }

    if let Err(error) = write_settings_to_path(path, &Settings::default()) {
        log::warn!(
            "failed to recreate default settings at {}: {error}",
            path.display()
        );
    }
}

#[tauri::command]
pub fn load_settings() -> Result<Settings, String> {
    let _lock = SETTINGS_LOCK.lock().map_err(|error| error.to_string())?;
    read_settings()
}

#[tauri::command]
pub fn save_setting(key: String, value: Value) -> Result<(), String> {
    let _lock = SETTINGS_LOCK.lock().map_err(|error| error.to_string())?;
    let mut settings = read_settings()?;

    match key.as_str() {
        "theme" => {
            settings.theme = Some(
                value
                    .as_str()
                    .ok_or_else(|| "theme setting must be a string".to_owned())?
                    .to_owned(),
            );
        }
        "language" => {
            settings.language = Some(
                value
                    .as_str()
                    .ok_or_else(|| "language setting must be a string".to_owned())?
                    .to_owned(),
            );
        }
        "autoCheckUpdates" => {
            settings.auto_check_updates = Some(
                value
                    .as_bool()
                    .ok_or_else(|| "autoCheckUpdates setting must be a boolean".to_owned())?,
            );
        }
        _ => return Err(format!("unsupported setting key: {key}")),
    }

    write_settings(&settings)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{read_settings_from_path, Settings};

    #[test]
    fn settings_use_camel_case_json_keys() {
        let json = serde_json::to_string(&Settings {
            auto_check_updates: Some(true),
            ..Default::default()
        })
        .unwrap();

        assert!(json.contains("autoCheckUpdates"));
    }

    #[test]
    fn invalid_settings_are_backed_up_and_recreated() {
        let directory =
            std::env::temp_dir().join(format!("harness-lens-settings-test-{}", std::process::id()));
        let path = directory.join("settings.json");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "{invalid json").unwrap();

        let settings = read_settings_from_path(&path).unwrap();
        assert!(settings.theme.is_none());
        let recreated: Settings =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(recreated.language.is_none());
        assert!(fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.corrupt-")));

        fs::remove_dir_all(directory).unwrap();
    }
}
