use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to {action} at {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "codex")]
    #[error("failed to {action} in SQLite database {path}: {source}")]
    Sqlite {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },

    #[error("invalid checkpoint: {0}")]
    InvalidCheckpoint(String),

    #[error("invalid coding-agent configuration: {0}")]
    InvalidConfiguration(String),

    #[error("coding-agent data source was not found: {0}")]
    SourceNotFound(String),

    #[cfg(feature = "watch")]
    #[error("filesystem watcher failed: {0}")]
    Watch(#[from] notify::Error),

    #[cfg(feature = "watch")]
    #[error("filesystem watcher stopped")]
    WatcherStopped,
}

impl Error {
    #[cfg(feature = "codex")]
    pub(crate) fn io(
        action: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }

    #[cfg(feature = "codex")]
    pub(crate) fn sqlite(
        action: &'static str,
        path: impl Into<PathBuf>,
        source: rusqlite::Error,
    ) -> Self {
        Self::Sqlite {
            action,
            path: path.into(),
            source,
        }
    }
}
