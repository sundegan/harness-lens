use std::sync::LazyLock;

use include_dir::{include_dir, Dir};
use rusqlite::Connection;
use rusqlite_migration::Migrations;

use super::error::DatabaseError;

pub(super) const APPLICATION_ID: u32 = 0x484C_4E53;

static MIGRATION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/src/database/migrations");
static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::from_directory(&MIGRATION_DIR)
        .expect("embedded database migration directory must be valid")
});

pub fn current_schema_version() -> u32 {
    MIGRATION_DIR.dirs().count() as u32
}

pub(super) fn apply_pending(connection: &mut Connection) -> Result<(), DatabaseError> {
    validate_upgrade_source(connection)?;
    MIGRATIONS
        .to_latest(connection)
        .map_err(DatabaseError::migration)?;
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(|source| DatabaseError::sqlite("write the database application id", source))
}

pub(super) fn ensure_current(connection: &Connection) -> Result<(), DatabaseError> {
    validate_application_id(connection, false)?;
    let version = schema_version(connection)?;
    let supported = current_schema_version();
    if version != supported {
        return Err(DatabaseError::UnsupportedSchemaVersion {
            found: version,
            supported,
        });
    }
    Ok(())
}

pub(super) fn validate_upgrade_source(connection: &Connection) -> Result<(), DatabaseError> {
    validate_application_id(connection, true)?;
    let version = schema_version(connection)?;
    let supported = current_schema_version();
    if version > supported {
        return Err(DatabaseError::UnsupportedSchemaVersion {
            found: version,
            supported,
        });
    }
    Ok(())
}

pub(super) fn schema_version(connection: &Connection) -> Result<u32, DatabaseError> {
    let version = MIGRATIONS
        .current_version(connection)
        .map_err(DatabaseError::migration)?;
    Ok(usize::from(version) as u32)
}

fn read_application_id(connection: &Connection) -> Result<u32, DatabaseError> {
    connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|source| DatabaseError::sqlite("read the database application id", source))
}

fn validate_application_id(
    connection: &Connection,
    allow_uninitialized: bool,
) -> Result<(), DatabaseError> {
    let application_id = read_application_id(connection)?;
    if application_id == APPLICATION_ID || (allow_uninitialized && application_id == 0) {
        return Ok(());
    }
    Err(DatabaseError::InvalidApplicationId {
        found: application_id,
    })
}

#[cfg(test)]
pub(super) fn validate_embedded_migrations() -> Result<(), DatabaseError> {
    MIGRATIONS.validate().map_err(DatabaseError::migration)
}
