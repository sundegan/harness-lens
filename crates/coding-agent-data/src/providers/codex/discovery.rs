use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::CodexSource;
use crate::{Error, Result};

const CODEX_HOME_ENV: &str = "CODEX_HOME";
const SQLITE_HOME_ENV: &str = "CODEX_SQLITE_HOME";

#[derive(Deserialize)]
struct CodexConfig {
    sqlite_home: Option<PathBuf>,
}

pub(super) fn discover() -> Result<CodexSource> {
    let codex_home = resolve_codex_home()?;
    if !codex_home.is_dir() {
        return Err(Error::SourceNotFound(codex_home.display().to_string()));
    }

    let sqlite_home = configured_sqlite_home(&codex_home)?
        .or_else(|| environment_path(SQLITE_HOME_ENV))
        .unwrap_or_else(|| codex_home.clone());

    Ok(CodexSource::from_paths(codex_home, sqlite_home))
}

fn resolve_codex_home() -> Result<PathBuf> {
    if let Some(path) = environment_path(CODEX_HOME_ENV) {
        return Ok(path);
    }

    dirs::home_dir()
        .map(|home| home.join(".codex"))
        .ok_or_else(|| Error::SourceNotFound("the user home directory is unavailable".to_owned()))
}

fn configured_sqlite_home(codex_home: &Path) -> Result<Option<PathBuf>> {
    let path = codex_home.join("config.toml");
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io("read Codex configuration", path, error)),
    };
    let config: CodexConfig = toml::from_str(&contents).map_err(|error| {
        Error::InvalidConfiguration(format!("failed to parse {}: {error}", path.display()))
    })?;
    Ok(config
        .sqlite_home
        .map(|value| resolve_config_path(codex_home, value)))
}

fn environment_path(name: &str) -> Option<PathBuf> {
    let value = env::var_os(name)?;
    let value = value.to_string_lossy();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(resolve_process_path(PathBuf::from(trimmed)))
}

fn resolve_process_path(path: PathBuf) -> PathBuf {
    let path = expand_tilde(path);
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|current| current.join(&path))
            .unwrap_or(path)
    }
}

fn resolve_config_path(codex_home: &Path, path: PathBuf) -> PathBuf {
    let path = expand_tilde(path);
    if path.is_absolute() {
        path
    } else {
        codex_home.join(path)
    }
}

fn expand_tilde(path: PathBuf) -> PathBuf {
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
