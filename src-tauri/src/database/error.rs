use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum DatabaseError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Sqlite {
        operation: &'static str,
        source: rusqlite::Error,
    },
    Migration {
        source: rusqlite_migration::Error,
    },
    UnsupportedSchemaVersion {
        found: u32,
        supported: u32,
    },
    InvalidApplicationId {
        found: u32,
    },
    InvalidBackupName,
    BackupNotFound(String),
    IntegrityCheckFailed(String),
    ForeignKeyCheckFailed,
    MaintenanceLockPoisoned,
    RestoreRollbackFailed {
        restore_error: String,
        rollback_error: String,
    },
}

impl DatabaseError {
    pub(super) fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }

    pub(super) fn sqlite(operation: &'static str, source: rusqlite::Error) -> Self {
        Self::Sqlite { operation, source }
    }

    pub(super) fn migration(source: rusqlite_migration::Error) -> Self {
        Self::Migration { source }
    }
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, .. } | Self::Sqlite { operation, .. } => {
                write!(formatter, "failed to {operation}")
            }
            Self::Migration { .. } => {
                formatter.write_str("failed to migrate the application database")
            }
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                formatter,
                "database schema version {found} is newer than supported version {supported}"
            ),
            Self::InvalidApplicationId { found } => {
                write!(
                    formatter,
                    "database application id {found} is not HarnessLens"
                )
            }
            Self::InvalidBackupName => formatter.write_str("invalid database backup name"),
            Self::BackupNotFound(name) => write!(formatter, "database backup not found: {name}"),
            Self::IntegrityCheckFailed(result) => {
                write!(formatter, "database integrity check failed: {result}")
            }
            Self::ForeignKeyCheckFailed => formatter.write_str("database foreign-key check failed"),
            Self::MaintenanceLockPoisoned => {
                formatter.write_str("database maintenance lock is unavailable")
            }
            Self::RestoreRollbackFailed {
                restore_error,
                rollback_error,
            } => write!(
                formatter,
                "database restore failed ({restore_error}) and rollback failed ({rollback_error})"
            ),
        }
    }
}

impl Error for DatabaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Sqlite { source, .. } => Some(source),
            Self::Migration { source } => Some(source),
            _ => None,
        }
    }
}
