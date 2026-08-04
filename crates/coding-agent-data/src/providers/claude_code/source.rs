use std::path::{Path, PathBuf};

use crate::providers::shared::source_path::{environment_path, normalize};
use crate::{Error, Result};

const CLAUDE_CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// Resolved path for one local Claude Code data source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaudeCodeSource {
    config_dir: PathBuf,
}

impl ClaudeCodeSource {
    /// Discovers `CLAUDE_CONFIG_DIR` or the default `~/.claude`.
    pub fn discover() -> Result<Self> {
        let config_dir = environment_path(CLAUDE_CONFIG_DIR_ENV)
            .or_else(|| dirs::home_dir().map(|home| home.join(".claude")))
            .ok_or_else(|| {
                Error::SourceNotFound("the user home directory is unavailable".to_owned())
            })?;
        if !config_dir.is_dir() {
            return Err(Error::SourceNotFound(config_dir.display().to_string()));
        }
        Ok(Self::new(config_dir))
    }

    /// Creates a source from an explicit Claude Code configuration directory.
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: normalize(config_dir),
        }
    }

    /// Returns the Claude Code configuration directory.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Returns the directory containing project transcripts.
    pub fn projects_dir(&self) -> PathBuf {
        self.config_dir.join("projects")
    }
}
