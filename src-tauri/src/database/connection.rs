use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;

use super::error::DatabaseError;
use super::migrations;
use super::migrations::current_schema_version;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct Database {
    pub(super) path: PathBuf,
    pub(super) backup_dir: PathBuf,
    pub(super) maintenance_lock: Arc<Mutex<()>>,
}

impl Database {
    pub fn initialize(path: impl Into<PathBuf>) -> Result<Self, DatabaseError> {
        let path = path.into();
        let backup_dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("backups");
        let database = Self {
            path,
            backup_dir,
            maintenance_lock: Arc::new(Mutex::new(())),
        };
        database.initialize_storage()?;
        Ok(database)
    }

    pub fn connect(&self) -> Result<Connection, DatabaseError> {
        let connection = open_configured_connection(&self.path)?;
        migrations::ensure_current(&connection)?;
        Ok(connection)
    }

    pub fn schema_version(&self) -> Result<u32, DatabaseError> {
        let connection = self.connect()?;
        migrations::schema_version(&connection)
    }

    pub fn validate_integrity(&self) -> Result<(), DatabaseError> {
        let connection = self.connect()?;
        validate_database(&connection)
    }

    fn initialize_storage(&self) -> Result<(), DatabaseError> {
        create_parent_directory(&self.path)?;
        fs::create_dir_all(&self.backup_dir)
            .map_err(|source| DatabaseError::io("create the backup directory", source))?;

        let existed_before = self.path.exists();
        let mut connection = if existed_before {
            open_configured_connection(&self.path)?
        } else {
            create_empty_database(&self.path)?
        };

        migrations::validate_upgrade_source(&connection)?;
        let from_version = migrations::schema_version(&connection)?;
        let target_version = current_schema_version();
        let needs_migration = from_version < target_version;
        let needs_backup = existed_before
            && needs_migration
            && (from_version > 0 || has_application_objects(&connection)?);
        drop(connection);

        if needs_backup {
            self.create_pre_migration_backup(from_version, target_version)?;
        }

        connection = open_configured_connection(&self.path)?;
        migrations::apply_pending(&mut connection)?;
        Ok(())
    }
}

fn create_parent_directory(path: &Path) -> Result<(), DatabaseError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|source| DatabaseError::io("create the database directory", source))?;
    }
    Ok(())
}

pub(super) fn open_configured_connection(path: &Path) -> Result<Connection, DatabaseError> {
    let connection = Connection::open(path)
        .map_err(|source| DatabaseError::sqlite("open the application database", source))?;
    configure_connection(&connection)?;
    Ok(connection)
}

fn create_empty_database(path: &Path) -> Result<Connection, DatabaseError> {
    let connection = Connection::open(path)
        .map_err(|source| DatabaseError::sqlite("create the application database", source))?;
    connection
        .execute_batch(
            "
            PRAGMA auto_vacuum = INCREMENTAL;
            VACUUM;
            ",
        )
        .map_err(|source| DatabaseError::sqlite("configure incremental auto-vacuum", source))?;
    configure_connection(&connection)?;
    Ok(connection)
}

pub(super) fn configure_connection(connection: &Connection) -> Result<(), DatabaseError> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|source| DatabaseError::sqlite("configure the database busy timeout", source))?;
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            ",
        )
        .map_err(|source| DatabaseError::sqlite("configure the database connection", source))
}

fn has_application_objects(connection: &Connection) -> Result<bool, DatabaseError> {
    let count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_schema
            WHERE name NOT LIKE 'sqlite_%'
            ",
            [],
            |row| row.get(0),
        )
        .map_err(|source| DatabaseError::sqlite("inspect the database schema", source))?;
    Ok(count > 0)
}

pub(super) fn validate_database(connection: &Connection) -> Result<(), DatabaseError> {
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|source| DatabaseError::sqlite("run the database integrity check", source))?;
    if quick_check != "ok" {
        return Err(DatabaseError::IntegrityCheckFailed(quick_check));
    }

    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|source| DatabaseError::sqlite("prepare the foreign-key check", source))?;
    let mut rows = statement
        .query([])
        .map_err(|source| DatabaseError::sqlite("run the foreign-key check", source))?;
    if rows
        .next()
        .map_err(|source| DatabaseError::sqlite("read the foreign-key check", source))?
        .is_some()
    {
        return Err(DatabaseError::ForeignKeyCheckFailed);
    }
    Ok(())
}
