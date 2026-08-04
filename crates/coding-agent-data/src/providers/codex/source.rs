use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::providers::shared::source_path::{environment_path, expand_tilde, normalize};
use crate::{Error, Result};

const CODEX_HOME_ENV: &str = "CODEX_HOME";
const SQLITE_HOME_ENV: &str = "CODEX_SQLITE_HOME";

#[derive(Deserialize)]
struct CodexConfig {
    sqlite_home: Option<PathBuf>,
}

/// Resolved paths for one local Codex data source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSource {
    codex_home: PathBuf,
    sqlite_home: PathBuf,
}

impl CodexSource {
    /// Discovers the effective Codex and SQLite homes.
    pub fn discover() -> Result<Self> {
        let codex_home = resolve_codex_home()?;
        if !codex_home.is_dir() {
            return Err(Error::SourceNotFound(codex_home.display().to_string()));
        }

        let sqlite_home = configured_sqlite_home(&codex_home)?
            .or_else(|| environment_path(SQLITE_HOME_ENV))
            .unwrap_or_else(|| codex_home.clone());
        Ok(Self::new(codex_home, sqlite_home))
    }

    /// Creates a source from explicit Codex and SQLite homes.
    pub fn new(codex_home: impl Into<PathBuf>, sqlite_home: impl Into<PathBuf>) -> Self {
        Self {
            codex_home: normalize(codex_home),
            sqlite_home: normalize(sqlite_home),
        }
    }

    /// Returns the Codex home containing session artifacts.
    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    /// Returns the directory containing `state_5.sqlite`.
    pub fn sqlite_home(&self) -> &Path {
        &self.sqlite_home
    }

    pub(super) fn state_database(&self) -> PathBuf {
        self.sqlite_home.join("state_5.sqlite")
    }

    pub(super) fn active_sessions(&self) -> PathBuf {
        self.codex_home.join("sessions")
    }

    pub(super) fn archived_sessions(&self) -> PathBuf {
        self.codex_home.join("archived_sessions")
    }

    pub(super) fn normalize_rollout_path(&self, path: impl Into<PathBuf>) -> Option<PathBuf> {
        let path = normalize(path);
        (path.starts_with(self.active_sessions()) || path.starts_with(self.archived_sessions()))
            .then_some(path)
    }
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
        .map(|value| resolve_from(codex_home, value)))
}

fn resolve_from(base: &Path, path: PathBuf) -> PathBuf {
    let path = expand_tilde(path);
    if path.is_absolute() {
        normalize(path)
    } else {
        normalize(base.join(path))
    }
}
