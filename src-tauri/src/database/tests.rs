use std::fs;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;
use rusqlite_migration::{Migrations, SchemaVersion, M};

use super::migrations::{validate_embedded_migrations, APPLICATION_ID};
use super::{current_schema_version, Database, DatabaseError};

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

fn test_database_path(test_name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join(format!(
            "harness-lens-database-test-{}-{}-{test_name}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ))
        .join("nested")
        .join("harness-lens.sqlite")
}

fn cleanup(path: &std::path::Path) {
    if let Some(root) = path.parent().and_then(|nested| nested.parent()) {
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn initialize_creates_versioned_analytics_database() {
    let path = test_database_path("empty");
    let database = Database::initialize(&path).unwrap();
    let connection = database.connect().unwrap();

    let analytics_table_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_schema
            WHERE type = 'table'
              AND name IN (
                'provider_sync_state',
                'agent_sessions',
                'rollout_sources',
                'agent_invocations',
                'skill_invocations',
                'token_usage_records',
                'session_events'
              )
            ",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    let journal_mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    let application_id: u32 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .unwrap();
    let sync_state_columns: Vec<String> = connection
        .prepare("SELECT name FROM pragma_table_info('provider_sync_state') ORDER BY cid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let auto_vacuum: u32 = connection
        .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))
        .unwrap();

    assert!(path.is_file());
    assert_eq!(database.schema_version().unwrap(), current_schema_version());
    assert_eq!(analytics_table_count, 7);
    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode, "wal");
    assert_eq!(application_id, APPLICATION_ID);
    assert_eq!(
        sync_state_columns,
        [
            "provider",
            "checkpoint_json",
            "status",
            "phase",
            "processed_records",
            "diagnostic_count",
            "last_error",
            "updated_at_ms"
        ]
    );
    assert_eq!(auto_vacuum, 2);

    drop(connection);
    cleanup(&path);
}

#[test]
fn embedded_migration_directory_is_valid() {
    validate_embedded_migrations().unwrap();
    assert_eq!(current_schema_version(), 7);
}

#[test]
fn migrations_are_ordered_and_transactional() {
    let mut connection = Connection::open_in_memory().unwrap();
    let migrations = Migrations::new(vec![
        M::up("CREATE TABLE example (id INTEGER PRIMARY KEY, value TEXT NOT NULL);"),
        M::up("ALTER TABLE example ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;"),
    ]);
    migrations.validate().unwrap();
    migrations.to_latest(&mut connection).unwrap();

    let version = migrations.current_version(&connection).unwrap();
    let enabled_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('example') WHERE name = 'enabled'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(
        version,
        SchemaVersion::Inside(NonZeroUsize::new(2).unwrap())
    );
    assert_eq!(enabled_exists, 1);
}

#[test]
fn failed_migration_rolls_back_schema_and_version() {
    let mut connection = Connection::open_in_memory().unwrap();
    let migrations = Migrations::new(vec![
        M::up("CREATE TABLE example (id INTEGER PRIMARY KEY);"),
        M::up("THIS IS NOT SQL;"),
    ]);
    let result = migrations.to_latest(&mut connection);

    let version = migrations.current_version(&connection).unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'example'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(result.is_err());
    assert_eq!(version, SchemaVersion::NoneSet);
    assert_eq!(table_count, 0);
}

#[test]
fn existing_unversioned_data_is_backed_up_before_baseline_migration() {
    let path = test_database_path("pre-migration");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("CREATE TABLE legacy (value TEXT NOT NULL)", [])
        .unwrap();
    connection
        .execute("INSERT INTO legacy VALUES ('preserved')", [])
        .unwrap();
    drop(connection);

    let database = Database::initialize(&path).unwrap();
    let backups = database.list_backups().unwrap();
    let value: String = database
        .connect()
        .unwrap()
        .query_row("SELECT value FROM legacy", [], |row| row.get(0))
        .unwrap();

    assert_eq!(value, "preserved");
    assert_eq!(backups.len(), 1);
    assert!(backups[0].file_name.contains(&format!(
        "pre-migration-v0-to-v{}",
        current_schema_version()
    )));

    database
        .connect()
        .unwrap()
        .execute("UPDATE legacy SET value = 'changed'", [])
        .unwrap();
    database.restore_backup(&backups[0].file_name).unwrap();
    let restored_value: String = database
        .connect()
        .unwrap()
        .query_row("SELECT value FROM legacy", [], |row| row.get(0))
        .unwrap();
    assert_eq!(restored_value, "preserved");
    assert_eq!(database.schema_version().unwrap(), current_schema_version());

    cleanup(&path);
}

#[test]
fn backup_and_restore_preserve_data_and_create_safety_backup() {
    let path = test_database_path("restore");
    let database = Database::initialize(&path).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE TABLE example (value TEXT NOT NULL)", [])
        .unwrap();
    connection
        .execute("INSERT INTO example VALUES ('before')", [])
        .unwrap();
    drop(connection);

    let backup = database.create_backup().unwrap();
    database
        .connect()
        .unwrap()
        .execute("UPDATE example SET value = 'after'", [])
        .unwrap();

    let restore = database.restore_backup(&backup.file_name).unwrap();
    let restored_value: String = database
        .connect()
        .unwrap()
        .query_row("SELECT value FROM example", [], |row| row.get(0))
        .unwrap();

    assert_eq!(restored_value, "before");
    assert_eq!(restore.restored_from, backup.file_name);
    assert!(restore.safety_backup.file_name.contains("pre-restore"));
    assert_eq!(restore.schema_version, current_schema_version());
    assert_eq!(database.list_backups().unwrap().len(), 2);

    cleanup(&path);
}

#[test]
fn newer_database_version_is_rejected() {
    let path = test_database_path("newer");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(&path).unwrap();
    let supported_version = current_schema_version();
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    connection
        .pragma_update(None, "user_version", supported_version + 1)
        .unwrap();
    drop(connection);

    let error = Database::initialize(&path).unwrap_err();
    assert!(matches!(
        error,
        DatabaseError::UnsupportedSchemaVersion {
            found,
            supported
        } if found == supported_version + 1 && supported == supported_version
    ));

    cleanup(&path);
}

#[test]
fn database_with_another_application_id_is_rejected() {
    let path = test_database_path("application-id");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .pragma_update(None, "application_id", APPLICATION_ID + 1)
        .unwrap();
    drop(connection);

    let error = Database::initialize(&path).unwrap_err();
    assert!(matches!(
        error,
        DatabaseError::InvalidApplicationId { found } if found == APPLICATION_ID + 1
    ));

    cleanup(&path);
}

#[test]
fn restore_rejects_path_traversal() {
    let path = test_database_path("traversal");
    let database = Database::initialize(&path).unwrap();

    let error = database.restore_backup("../other.sqlite").unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidBackupName));

    cleanup(&path);
}
