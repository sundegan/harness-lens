use std::{env, path::PathBuf};

pub(crate) fn environment_path(name: &str) -> Option<PathBuf> {
    let value = env::var_os(name)?;
    let value = value.to_string_lossy();
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| normalize(PathBuf::from(trimmed)))
}

pub(crate) fn normalize(path: impl Into<PathBuf>) -> PathBuf {
    let path = expand_tilde(path.into());
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|current| current.join(&path))
            .unwrap_or(path)
    };
    absolute.canonicalize().unwrap_or(absolute)
}

pub(crate) fn expand_tilde(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if value == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(suffix) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(suffix);
        }
    }
    path
}
