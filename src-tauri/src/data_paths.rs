use std::path::PathBuf;

const APP_DATA_DIR_NAME: &str = ".harness-lens";

pub fn root_dir() -> PathBuf {
    #[cfg(feature = "e2e")]
    if let Some(path) = std::env::var_os("HARNESS_LENS_E2E_DATA_DIR") {
        return PathBuf::from(path);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DATA_DIR_NAME)
}

pub fn logs_dir() -> PathBuf {
    root_dir().join("logs")
}

pub fn database_path() -> PathBuf {
    root_dir().join("harness-lens.sqlite")
}

pub fn crash_log_path() -> PathBuf {
    root_dir().join("crash.log")
}

pub fn settings_path() -> PathBuf {
    root_dir().join("settings.json")
}

pub fn window_state_path() -> PathBuf {
    root_dir().join("window-state.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_files_share_the_same_root_directory() {
        let root = root_dir();

        assert_eq!(logs_dir(), root.join("logs"));
        assert_eq!(database_path(), root.join("harness-lens.sqlite"));
        assert_eq!(crash_log_path(), root.join("crash.log"));
        assert_eq!(settings_path(), root.join("settings.json"));
        assert_eq!(window_state_path(), root.join("window-state.json"));
    }
}
