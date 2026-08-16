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

fn create_legacy_v8_database(path: &std::path::Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE provider_sync_state (
                source_id TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL
            );
            CREATE TABLE agent_sessions (
                id TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                source_session_id TEXT NOT NULL,
                source_id TEXT,
                project_key TEXT
            );
            CREATE TABLE rollout_sources (
                path TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL
            );
            CREATE TABLE agent_invocations (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
                source_path TEXT NOT NULL
            );
            CREATE TABLE skill_invocations (
                id TEXT PRIMARY KEY NOT NULL,
                invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE
            );
            CREATE TABLE pending_skill_reads (
                source_path TEXT NOT NULL,
                call_id TEXT NOT NULL,
                invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
                PRIMARY KEY (source_path, call_id)
            );
            CREATE TABLE token_usage_records (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
                invocation_id TEXT REFERENCES agent_invocations(id) ON DELETE SET NULL,
                source_path TEXT NOT NULL
            );
            CREATE TABLE session_events (
                id TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
                source_path TEXT NOT NULL
            );
            CREATE TABLE tool_call (
                id TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                source_id TEXT NOT NULL,
                session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
                source_path TEXT NOT NULL,
                call_id TEXT NOT NULL,
                retry_class TEXT NOT NULL DEFAULT 'none'
                    CHECK (retry_class IN ('none', 'explicit', 'inferred'))
            );

            INSERT INTO provider_sync_state VALUES ('codex:legacy', 'codex');
            INSERT INTO agent_sessions
                VALUES ('session:legacy', 'codex', 'legacy-session', 'codex:legacy', 'legacy-project');
            INSERT INTO rollout_sources VALUES ('legacy.jsonl', 'codex', 'session:legacy');
            INSERT INTO agent_invocations VALUES ('invocation:legacy', 'session:legacy', 'legacy.jsonl');
            INSERT INTO skill_invocations
                VALUES ('skill:legacy', 'invocation:legacy', 'session:legacy');
            INSERT INTO pending_skill_reads
                VALUES ('legacy.jsonl', 'call:legacy', 'invocation:legacy');
            INSERT INTO token_usage_records
                VALUES ('usage:legacy', 'session:legacy', 'invocation:legacy', 'legacy.jsonl');
            INSERT INTO session_events
                VALUES ('event:legacy', 'codex', 'session:legacy', 'legacy.jsonl');
            INSERT INTO tool_call
                VALUES (
                    'tool:legacy',
                    'codex',
                    'codex:legacy',
                    'session:legacy',
                    'legacy.jsonl',
                    'call:legacy',
                    'explicit'
                );
            ",
        )
        .unwrap();
    connection
        .pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    connection.pragma_update(None, "user_version", 8).unwrap();
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
                'session_events',
                'mcp_tool_call',
                'mcp_tool_call_retry'
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
    let tool_call_indexes: Vec<String> = connection
        .prepare("SELECT name FROM pragma_index_list('mcp_tool_call') ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(path.is_file());
    assert_eq!(database.schema_version().unwrap(), current_schema_version());
    assert_eq!(analytics_table_count, 9);
    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode, "wal");
    assert_eq!(application_id, APPLICATION_ID);
    assert_eq!(
        sync_state_columns,
        [
            "source_id",
            "provider",
            "checkpoint_json",
            "status",
            "phase",
            "processed_records",
            "diagnostic_count",
            "last_error",
            "updated_at_ms",
            "total_files",
            "processed_files",
            "processed_lines",
            "estimated_total_lines",
            "current_file",
            "current_line",
            "estimated_remaining_ms"
        ]
    );
    assert_eq!(auto_vacuum, 2);
    assert!(tool_call_indexes.contains(&"mcp_tool_call_call_event_idx".to_owned()));
    assert!(tool_call_indexes.contains(&"mcp_tool_call_result_event_idx".to_owned()));

    drop(connection);
    cleanup(&path);
}

#[test]
fn embedded_migration_directory_is_valid() {
    validate_embedded_migrations().unwrap();
    assert_eq!(current_schema_version(), 12);
}

#[test]
fn first_user_message_migration_backfills_role_or_actor_user_events() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE agent_sessions (id TEXT PRIMARY KEY NOT NULL);
            CREATE TABLE session_events (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL,
                timestamp_ms INTEGER,
                sequence_position INTEGER NOT NULL,
                sequence_part INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                event_json TEXT NOT NULL
            );
            INSERT INTO agent_sessions (id) VALUES ('session-1'), ('session-2');
            INSERT INTO session_events (
                id, session_id, timestamp_ms, sequence_position, sequence_part, event_type, event_json
            ) VALUES
                (
                    'event-role-user', 'session-1', 20, 2, 0, 'message',
                    '{"actor":"agent","data":{"type":"message","value":{"role":"user","content":[{"type":"text","text":"role user"}]}}}'
                ),
                (
                    'event-actor-user', 'session-1', 10, 1, 0, 'message',
                    '{"actor":"user","data":{"type":"message","value":{"role":"assistant","content":[{"type":"text","text":"actor user"}]}}}'
                ),
                (
                    'event-not-user', 'session-2', 1, 1, 0, 'message',
                    '{"actor":"agent","data":{"type":"message","value":{"role":"assistant","content":[{"type":"text","text":"assistant"}]}}}'
                );
            "#,
        )
        .unwrap();

    connection
        .execute_batch(include_str!("migrations/11-first-user-message/up.sql"))
        .unwrap();

    let first: (Option<String>, Option<String>, Option<i64>) = connection
        .query_row(
            "
            SELECT first_user_message_text,
                   first_user_message_event_id,
                   first_user_message_timestamp_ms
            FROM agent_sessions
            WHERE id = 'session-1'
            ",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(first.0.as_deref(), Some("actor user"));
    assert_eq!(first.1.as_deref(), Some("event-actor-user"));
    assert_eq!(first.2, Some(10));

    let empty: (Option<String>, Option<String>, Option<i64>) = connection
        .query_row(
            "
            SELECT first_user_message_text,
                   first_user_message_event_id,
                   first_user_message_timestamp_ms
            FROM agent_sessions
            WHERE id = 'session-2'
            ",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(empty, (None, None, None));
}

#[test]
fn version_8_projection_is_backed_up_and_rebuilt_as_mcp_only() {
    let path = test_database_path("version-8-mcp-only");
    create_legacy_v8_database(&path);

    let database = Database::initialize(&path).unwrap();
    let connection = database.connect().unwrap();
    let backups = database.list_backups().unwrap();
    let rebuilt_projection_rows: i64 = connection
        .query_row(
            "
            SELECT
                (SELECT COUNT(*) FROM provider_sync_state) +
                (SELECT COUNT(*) FROM agent_sessions) +
                (SELECT COUNT(*) FROM rollout_sources) +
                (SELECT COUNT(*) FROM agent_invocations) +
                (SELECT COUNT(*) FROM skill_invocations) +
                (SELECT COUNT(*) FROM pending_skill_reads) +
                (SELECT COUNT(*) FROM token_usage_records) +
                (SELECT COUNT(*) FROM session_events) +
                (SELECT COUNT(*) FROM mcp_tool_call) +
                (SELECT COUNT(*) FROM mcp_tool_call_retry)
            ",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let required_source_columns: i64 = connection
        .query_row(
            "
            SELECT
                (SELECT COUNT(*) FROM pragma_table_info('rollout_sources') WHERE name = 'source_id') +
                (SELECT COUNT(*) FROM pragma_table_info('agent_invocations') WHERE name = 'source_id') +
                (SELECT COUNT(*) FROM pragma_table_info('session_events') WHERE name = 'source_id')
            ",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let first_user_message_columns: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM pragma_table_info('agent_sessions')
            WHERE name IN (
                'first_user_message_text',
                'first_user_message_event_id',
                'first_user_message_timestamp_ms'
            )
            ",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let retry_table_exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'mcp_tool_call_retry'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let legacy_tool_table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name IN ('tool_call', 'tool_call_retry')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "
            INSERT INTO agent_sessions (id, provider, source_id, source_session_id)
            VALUES ('session:new', 'codex', 'codex:new', 'new-session')
            ",
            [],
        )
        .unwrap();
    let explicit_retry_insert = connection.execute(
        "
        INSERT INTO mcp_tool_call (
            id, provider, source_id, session_id, source_path, call_id, retry_class
        ) VALUES (
            'tool:new', 'codex', 'codex:new', 'session:new', 'new.jsonl', 'call:new', 'explicit'
        )
        ",
        [],
    );

    assert_eq!(database.schema_version().unwrap(), 12);
    assert_eq!(rebuilt_projection_rows, 0);
    assert_eq!(required_source_columns, 3);
    assert_eq!(first_user_message_columns, 3);
    assert_eq!(retry_table_exists, 1);
    assert_eq!(legacy_tool_table_count, 0);
    assert!(explicit_retry_insert.is_err());
    assert_eq!(backups.len(), 1);
    assert!(backups[0].file_name.contains("pre-migration-v8-to-v12"));

    let backup_path = path
        .parent()
        .unwrap()
        .join("backups")
        .join(&backups[0].file_name);
    let backup_connection = Connection::open(backup_path).unwrap();
    let backed_up_tool_call: String = backup_connection
        .query_row("SELECT id FROM tool_call", [], |row| row.get(0))
        .unwrap();
    assert_eq!(backed_up_tool_call, "tool:legacy");

    drop(backup_connection);
    drop(connection);
    cleanup(&path);
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
