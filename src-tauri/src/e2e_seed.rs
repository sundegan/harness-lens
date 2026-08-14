use std::fs;
use std::path::Path;

use rusqlite::params;

use crate::database::{Database, DatabaseError};

pub fn reset_database_files(path: &Path) -> Result<(), DatabaseError> {
    for candidate in [
        path.to_path_buf(),
        path.with_extension("sqlite-wal"),
        path.with_extension("sqlite-shm"),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(DatabaseError::io("reset the isolated E2E database", source))
            }
        }
    }
    Ok(())
}

pub fn seed(database: &Database) -> Result<(), DatabaseError> {
    let connection = database.connect()?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let base = now_ms - 3_600_000;

    connection
        .execute_batch(
            r#"
            INSERT INTO agent_sessions (
                id, provider, source_id, source_session_id, title, project_name,
                project_key, agent_version, created_at_ms, updated_at_ms,
                metadata_present, data_quality
            ) VALUES
                ('e2e-session-codex', 'codex', 'codex:e2e', 'codex-e2e',
                 'E2E Codex session', 'Fixture Alpha', 'fixture-alpha', '1.2.3',
                 0, 0, 1, 'complete'),
                ('e2e-session-claude', 'claude-code', 'claude:e2e', 'claude-e2e',
                 'E2E Claude session', 'Fixture Beta', 'fixture-beta', '4.5.6',
                 0, 0, 1, 'complete');

            INSERT INTO agent_invocations (
                id, source_id, session_id, source_path, started_at_ms, status
            ) VALUES
                ('e2e-invocation-codex', 'codex:e2e', 'e2e-session-codex',
                 'fixture://codex', 0, 'failed'),
                ('e2e-invocation-claude', 'claude:e2e', 'e2e-session-claude',
                 'fixture://claude', 0, 'cancelled');

            INSERT INTO session_events (
                id, provider, source_id, session_id, invocation_id, source_path,
                timestamp_ms, sequence_position, sequence_part, event_type, event_json
            ) VALUES
                ('e2e-event-call-3', 'codex', 'codex:e2e', 'e2e-session-codex',
                 'e2e-invocation-codex', 'fixture://codex', 0, 3, 0, 'tool_call',
                 '{"external_id":"native-3","sequence":{"position":3,"part":0},"parent":null,"actor":"assistant","agent_id":null,"data":{"type":"tool_call","value":{"call_id":"native-3","name":"terminal","namespace":"terminal","source_kind":"mcp","server_name":"terminal","kind":"execute","status":"completed","input":{"fixture":"safe"},"locations":[]}}}'),
                ('e2e-event-result-3', 'codex', 'codex:e2e', 'e2e-session-codex',
                 'e2e-invocation-codex', 'fixture://codex', 0, 4, 0, 'tool_result',
                 '{"external_id":"native-3-result","sequence":{"position":4,"part":0},"parent":"native-3","actor":"tool","agent_id":null,"data":{"type":"tool_result","value":{"call_id":"native-3","name":"terminal","output":{"fixture":"ok"},"content":[],"status":"completed","duration_ms":200}}}'),
                ('e2e-event-retry-3', 'codex', 'codex:e2e', 'e2e-session-codex',
                 'e2e-invocation-codex', 'fixture://codex', 0, 2, 0, 'retry',
                 '{"external_id":"retry-3","sequence":{"position":2,"part":0},"parent":"native-3","actor":"system","agent_id":null,"data":{"type":"retry","value":{"attempt":2,"delay_ms":25}}}');

            INSERT INTO mcp_tool_call (
                id, provider, source_id, session_id, invocation_id, source_path, call_id,
                tool_name, mcp_server, tool_kind, started_at_ms, completed_at_ms,
                duration_ms, duration_source, status, has_result, has_error,
                call_event_id, result_event_id, input_fingerprint, repeat_group_id,
                repeat_of_id, repeat_index, retry_class, retry_of_id, evidence_quality
            ) VALUES
                ('e2e-call-1', 'codex', 'codex:e2e', 'e2e-session-codex', 'e2e-invocation-codex',
                 'fixture://codex', 'native-1', 'terminal', 'terminal', 'execute',
                 0, 0, 100, 'normalized', 'completed', 1, 0,
                 NULL, NULL, 'e2e-fingerprint-a', 'e2e-repeat-a', NULL, 0, 'none', NULL, 'observed'),
                ('e2e-call-2', 'codex', 'codex:e2e', 'e2e-session-codex', 'e2e-invocation-codex',
                 'fixture://codex', 'native-2', 'terminal', 'terminal', 'execute',
                 0, 0, 120, 'normalized', 'failed', 1, 1,
                 NULL, NULL, 'e2e-fingerprint-a', 'e2e-repeat-a', 'e2e-call-1', 1, 'none', NULL, 'observed'),
                ('e2e-call-3', 'codex', 'codex:e2e', 'e2e-session-codex', 'e2e-invocation-codex',
                 'fixture://codex', 'native-3', 'terminal', 'terminal', 'execute',
                 0, 0, 200, 'event_delta', 'completed', 1, 0,
                 'e2e-event-call-3', 'e2e-event-result-3', 'e2e-fingerprint-a', 'e2e-repeat-a',
                 'e2e-call-2', 2, 'inferred', 'e2e-call-2', 'observed'),
                ('e2e-call-4', 'claude-code', 'claude:e2e', 'e2e-session-claude', 'e2e-invocation-claude',
                 'fixture://claude', 'native-4', 'read_file', 'filesystem', 'read',
                 0, 0, 50, 'normalized', 'declined', 1, 0,
                 NULL, NULL, 'e2e-fingerprint-b', 'e2e-repeat-b', NULL, 0, 'none', NULL, 'observed'),
                ('e2e-call-5', 'claude-code', 'claude:e2e', 'e2e-session-claude', 'e2e-invocation-claude',
                 'fixture://claude', 'native-5', 'provider_search', 'search', 'search',
                 0, NULL, NULL, 'unknown', 'cancelled', 0, 0,
                 NULL, NULL, 'e2e-fingerprint-c', 'e2e-repeat-c', NULL, 0, 'none', NULL, 'observed'),
                ('e2e-call-6', 'claude-code', 'claude:e2e', 'e2e-session-claude', 'e2e-invocation-claude',
                 'fixture://claude', 'native-6', 'query', 'database', 'fetch',
                 NULL, NULL, NULL, 'unknown', 'unknown', 0, 0,
                 NULL, NULL, 'e2e-fingerprint-d', 'e2e-repeat-d', NULL, 0, 'none', NULL, 'unknown');

            INSERT INTO mcp_tool_call_retry (
                id, provider, source_id, session_id, invocation_id, source_path,
                timestamp_ms, mcp_tool_call_id, attempt, delay_ms, evidence_quality
            ) VALUES
                ('e2e-event-retry-3', 'codex', 'codex:e2e', 'e2e-session-codex',
                 'e2e-invocation-codex', 'fixture://codex', 0, 'e2e-call-3', 2, 25, 'observed');
            "#,
        )
        .map_err(|source| DatabaseError::sqlite("seed the isolated E2E database", source))?;

    connection
        .execute(
            "UPDATE agent_sessions SET created_at_ms = ?1, updated_at_ms = ?1",
            [base],
        )
        .map_err(|source| DatabaseError::sqlite("timestamp E2E sessions", source))?;
    connection
        .execute("UPDATE agent_invocations SET started_at_ms = ?1", [base])
        .map_err(|source| DatabaseError::sqlite("timestamp E2E invocations", source))?;
    connection
        .execute(
            "UPDATE session_events SET timestamp_ms = ?1 + sequence_position * 100",
            [base],
        )
        .map_err(|source| DatabaseError::sqlite("timestamp E2E events", source))?;
    connection
        .execute(
            "UPDATE mcp_tool_call SET started_at_ms = CASE WHEN id = 'e2e-call-6' THEN NULL ELSE ?1 + repeat_index * 1000 END, completed_at_ms = CASE WHEN completed_at_ms IS NULL THEN NULL ELSE ?1 + repeat_index * 1000 + COALESCE(duration_ms, 0) END",
            params![base],
        )
        .map_err(|source| DatabaseError::sqlite("timestamp E2E tool calls", source))?;
    Ok(())
}
