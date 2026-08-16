use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};

use super::connection::{
    configure_connection, open_configured_connection, validate_database, Database,
};
use super::error::DatabaseError;
use super::migrations;

const BACKUP_PREFIX: &str = "harness-lens-";
const BACKUP_SUFFIX: &str = ".sqlite";
const BACKUP_PAGES_PER_STEP: i32 = 100;
const BACKUP_STEP_PAUSE: Duration = Duration::from_millis(10);
const MIGRATION_BACKUP_PAGES_PER_STEP: i32 = 1_000;
const MIGRATION_BACKUP_STEP_PAUSE: Duration = Duration::ZERO;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupInfo {
    pub file_name: String,
    pub size_bytes: u64,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreInfo {
    pub restored_from: String,
    pub safety_backup: BackupInfo,
    pub schema_version: u32,
}

enum BackupKind {
    Manual,
    PreMigration { from: u32, to: u32 },
    PreRestore,
}

impl Database {
    pub fn create_backup(&self) -> Result<BackupInfo, DatabaseError> {
        let _guard = self
            .maintenance_lock
            .lock()
            .map_err(|_| DatabaseError::MaintenanceLockPoisoned)?;
        self.create_backup_locked(BackupKind::Manual)
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>, DatabaseError> {
        let mut backups = Vec::new();
        for entry in fs::read_dir(&self.backup_dir)
            .map_err(|source| DatabaseError::io("read the backup directory", source))?
        {
            let entry = entry
                .map_err(|source| DatabaseError::io("read a backup directory entry", source))?;
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if validate_backup_name(file_name).is_err() {
                continue;
            }
            backups.push(backup_info(&path)?);
        }
        backups.sort_by(|left, right| {
            right
                .created_at_ms
                .cmp(&left.created_at_ms)
                .then_with(|| right.file_name.cmp(&left.file_name))
        });
        Ok(backups)
    }

    pub fn restore_backup(&self, file_name: &str) -> Result<RestoreInfo, DatabaseError> {
        validate_backup_name(file_name)?;
        let backup_path = self.backup_dir.join(file_name);
        if !backup_path.is_file() {
            return Err(DatabaseError::BackupNotFound(file_name.to_owned()));
        }

        let _guard = self
            .maintenance_lock
            .lock()
            .map_err(|_| DatabaseError::MaintenanceLockPoisoned)?;
        let staging_path = self.unique_work_path("restore-staging");

        let result = self.restore_backup_locked(file_name, &backup_path, &staging_path);
        remove_working_database(&staging_path);
        result
    }

    pub(super) fn create_pre_migration_backup(
        &self,
        from: u32,
        to: u32,
    ) -> Result<BackupInfo, DatabaseError> {
        let _guard = self
            .maintenance_lock
            .lock()
            .map_err(|_| DatabaseError::MaintenanceLockPoisoned)?;
        self.create_backup_locked(BackupKind::PreMigration { from, to })
    }

    fn restore_backup_locked(
        &self,
        file_name: &str,
        backup_path: &Path,
        staging_path: &Path,
    ) -> Result<RestoreInfo, DatabaseError> {
        copy_database_file(
            backup_path,
            staging_path,
            BACKUP_PAGES_PER_STEP,
            BACKUP_STEP_PAUSE,
        )?;

        let mut staging_connection = open_configured_connection(staging_path)?;
        migrations::apply_pending(&mut staging_connection)?;
        validate_database(&staging_connection)?;
        drop(staging_connection);

        let safety_backup = self.create_backup_locked(BackupKind::PreRestore)?;
        let restore_result = replace_database_from(&self.path, staging_path).and_then(|()| {
            let connection = self.connect()?;
            validate_database(&connection)
        });

        if let Err(restore_error) = restore_result {
            let safety_path = self.backup_dir.join(&safety_backup.file_name);
            if let Err(rollback_error) = replace_database_from(&self.path, &safety_path) {
                return Err(DatabaseError::RestoreRollbackFailed {
                    restore_error: restore_error.to_string(),
                    rollback_error: rollback_error.to_string(),
                });
            }
            return Err(restore_error);
        }

        Ok(RestoreInfo {
            restored_from: file_name.to_owned(),
            safety_backup,
            schema_version: migrations::current_schema_version(),
        })
    }

    fn create_backup_locked(&self, kind: BackupKind) -> Result<BackupInfo, DatabaseError> {
        fs::create_dir_all(&self.backup_dir)
            .map_err(|source| DatabaseError::io("create the backup directory", source))?;
        let final_path = self.unique_backup_path(&kind);
        let partial_path = final_path.with_extension("sqlite.partial");
        let (pages_per_step, step_pause) = match kind {
            // Migration backups run in the background initialization thread.
            BackupKind::PreMigration { .. } => {
                (MIGRATION_BACKUP_PAGES_PER_STEP, MIGRATION_BACKUP_STEP_PAUSE)
            }
            BackupKind::Manual | BackupKind::PreRestore => {
                (BACKUP_PAGES_PER_STEP, BACKUP_STEP_PAUSE)
            }
        };

        let result = (|| {
            copy_database_file(&self.path, &partial_path, pages_per_step, step_pause)?;
            let connection = open_read_only_connection(&partial_path)?;
            migrations::validate_upgrade_source(&connection)?;
            validate_database(&connection)?;
            drop(connection);
            fs::rename(&partial_path, &final_path)
                .map_err(|source| DatabaseError::io("publish the database backup", source))?;
            backup_info(&final_path)
        })();

        remove_working_database(&partial_path);
        result
    }

    fn unique_backup_path(&self, kind: &BackupKind) -> PathBuf {
        let label = match kind {
            BackupKind::Manual => "manual".to_owned(),
            BackupKind::PreMigration { from, to } => {
                format!("pre-migration-v{from}-to-v{to}")
            }
            BackupKind::PreRestore => "pre-restore".to_owned(),
        };
        unique_path(
            &self.backup_dir,
            &format!("{BACKUP_PREFIX}{label}"),
            BACKUP_SUFFIX,
        )
    }

    fn unique_work_path(&self, label: &str) -> PathBuf {
        unique_path(
            self.path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
            &format!(".{BACKUP_PREFIX}{label}"),
            ".sqlite.partial",
        )
    }
}

fn copy_database_file(
    source_path: &Path,
    destination_path: &Path,
    pages_per_step: i32,
    step_pause: Duration,
) -> Result<(), DatabaseError> {
    let source = open_read_only_connection(source_path)?;
    let mut destination = Connection::open(destination_path)
        .map_err(|source| DatabaseError::sqlite("open the database backup target", source))?;
    {
        let backup = Backup::new(&source, &mut destination)
            .map_err(|source| DatabaseError::sqlite("start the database backup", source))?;
        backup
            .run_to_completion(pages_per_step, step_pause, None)
            .map_err(|source| DatabaseError::sqlite("copy the database backup", source))?;
    }
    Ok(())
}

fn replace_database_from(destination_path: &Path, source_path: &Path) -> Result<(), DatabaseError> {
    let source = open_read_only_connection(source_path)?;
    let mut destination = open_configured_connection(destination_path)?;
    {
        let backup = Backup::new(&source, &mut destination)
            .map_err(|source| DatabaseError::sqlite("start the database restore", source))?;
        backup
            .run_to_completion(BACKUP_PAGES_PER_STEP, BACKUP_STEP_PAUSE, None)
            .map_err(|source| DatabaseError::sqlite("restore the database", source))?;
    }
    configure_connection(&destination)?;
    validate_database(&destination)
}

fn open_read_only_connection(path: &Path) -> Result<Connection, DatabaseError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|source| DatabaseError::sqlite("open a database backup", source))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|source| DatabaseError::sqlite("configure a database backup", source))?;
    Ok(connection)
}

fn backup_info(path: &Path) -> Result<BackupInfo, DatabaseError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(DatabaseError::InvalidBackupName)?;
    validate_backup_name(file_name)?;
    let metadata = fs::metadata(path)
        .map_err(|source| DatabaseError::io("read database backup metadata", source))?;
    if !metadata.is_file() {
        return Err(DatabaseError::InvalidBackupName);
    }
    let created_at_ms = metadata
        .modified()
        .map_err(|source| DatabaseError::io("read database backup modification time", source))?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    Ok(BackupInfo {
        file_name: file_name.to_owned(),
        size_bytes: metadata.len(),
        created_at_ms,
    })
}

fn validate_backup_name(file_name: &str) -> Result<(), DatabaseError> {
    let path = Path::new(file_name);
    let is_single_component = matches!(
        (path.components().next(), path.components().nth(1)),
        (Some(Component::Normal(_)), None)
    );
    if !is_single_component
        || !file_name.starts_with(BACKUP_PREFIX)
        || !file_name.ends_with(BACKUP_SUFFIX)
    {
        return Err(DatabaseError::InvalidBackupName);
    }
    Ok(())
}

fn unique_path(directory: &Path, prefix: &str, suffix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for counter in 0u32.. {
        let candidate = directory.join(format!("{prefix}-{timestamp}-{counter}{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 backup filename space exhausted")
}

fn remove_working_database(path: &Path) {
    let _ = fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = fs::remove_file(sidecar);
    }
}
