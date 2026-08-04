use std::path::PathBuf;

use thiserror::Error;

/// Result type returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced while discovering, reading, or monitoring local agent data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A local artifact could not be accessed.
    #[error("failed to {action} at {path}: {source}")]
    Io {
        /// Operation that failed.
        action: &'static str,
        /// Artifact involved.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    #[cfg(feature = "sqlite")]
    /// A SQLite-backed local data source could not be queried read-only.
    #[error("failed to {action} in SQLite database {path}: {source}")]
    Sqlite {
        /// Operation that failed.
        action: &'static str,
        /// Database path.
        path: PathBuf,
        /// Underlying SQLite error.
        #[source]
        source: rusqlite::Error,
    },

    /// A checkpoint is malformed, outdated, or belongs to another source.
    #[error("invalid checkpoint: {0}")]
    InvalidCheckpoint(String),

    /// A provider produced a batch that violates the public data-model contract.
    #[error("invalid provider batch: {0}")]
    InvalidBatch(String),

    /// Provider configuration could not be interpreted.
    #[error("invalid coding-agent configuration: {0}")]
    InvalidConfiguration(String),

    /// The requested local data source does not exist.
    #[error("coding-agent data source was not found: {0}")]
    SourceNotFound(String),

    /// A provider could not start or maintain live monitoring.
    #[error("provider subscription failed: {0}")]
    Subscription(String),

    /// A live subscription ended before the caller cancelled it.
    #[error("provider subscription closed")]
    SubscriptionClosed,
}

impl Error {
    /// Wraps an I/O failure with the attempted operation and artifact path.
    pub fn io(action: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }

    #[cfg(feature = "sqlite")]
    /// Wraps a SQLite failure with the attempted operation and database path.
    pub fn sqlite(action: &'static str, path: impl Into<PathBuf>, source: rusqlite::Error) -> Self {
        Self::Sqlite {
            action,
            path: path.into(),
            source,
        }
    }
}
