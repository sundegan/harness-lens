use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(any(not(feature = "e2e"), test))]
use rusqlite::{params, Transaction};
use rusqlite::{Connection, OptionalExtension, Row};

use super::model::{AnalyticsSnapshot, OverallSummary, SessionSummary, SkillSummary, SyncStatus};
use crate::database::{Database, DatabaseError};

pub(super) const PROVIDER: &str = "codex";

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(not(feature = "e2e"))]
pub(super) fn load_checkpoint(database: &Database) -> Result<Option<String>, DatabaseError> {
    database
        .connect()?
        .query_row(
            "
            SELECT checkpoint_json
            FROM analytics_sync_state
            WHERE provider = ?1
            ",
            [PROVIDER],
            |row| row.get(0),
        )
        .optional()
        .map_err(|source| DatabaseError::sqlite("load the analytics checkpoint", source))
        .map(Option::flatten)
}

#[cfg(not(feature = "e2e"))]
pub(super) fn update_sync_status(
    database: &Database,
    status: &str,
    phase: &str,
    last_error: Option<&str>,
) -> Result<(), DatabaseError> {
    database
        .connect()?
        .execute(
            "
            INSERT INTO analytics_sync_state (
                provider,
                status,
                phase,
                last_error,
                updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(provider) DO UPDATE SET
                status = excluded.status,
                phase = excluded.phase,
                last_error = excluded.last_error,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![PROVIDER, status, phase, last_error, now_ms()],
        )
        .map_err(|source| DatabaseError::sqlite("update the analytics sync status", source))?;
    Ok(())
}

#[cfg(any(not(feature = "e2e"), test))]
pub(super) fn save_batch_state(
    transaction: &Transaction<'_>,
    checkpoint_json: &str,
    status: &str,
    phase: &str,
    processed_records: usize,
    diagnostic_count: usize,
) -> Result<(), DatabaseError> {
    transaction
        .execute(
            "
            INSERT INTO analytics_sync_state (
                provider,
                checkpoint_json,
                status,
                phase,
                processed_records,
                diagnostic_count,
                last_error,
                updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)
            ON CONFLICT(provider) DO UPDATE SET
                checkpoint_json = excluded.checkpoint_json,
                status = excluded.status,
                phase = excluded.phase,
                processed_records = analytics_sync_state.processed_records
                    + excluded.processed_records,
                diagnostic_count = analytics_sync_state.diagnostic_count
                    + excluded.diagnostic_count,
                last_error = NULL,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![
                PROVIDER,
                checkpoint_json,
                status,
                phase,
                processed_records as i64,
                diagnostic_count as i64,
                now_ms()
            ],
        )
        .map_err(|source| DatabaseError::sqlite("save the analytics checkpoint", source))?;
    Ok(())
}

pub(super) fn analytics_snapshot(database: &Database) -> Result<AnalyticsSnapshot, DatabaseError> {
    let connection = database.connect()?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|source| DatabaseError::sqlite("begin the analytics snapshot", source))?;
    let snapshot = AnalyticsSnapshot {
        sync: query_sync_status(&transaction)?,
        summary: query_overall_summary(&transaction)?,
        sessions: query_sessions(&transaction)?,
        skills: query_skills(&transaction)?,
        generated_at_ms: now_ms(),
    };
    transaction
        .commit()
        .map_err(|source| DatabaseError::sqlite("finish the analytics snapshot", source))?;
    Ok(snapshot)
}

fn query_sync_status(connection: &Connection) -> Result<SyncStatus, DatabaseError> {
    connection
        .query_row(
            "
            SELECT
                status,
                phase,
                processed_records,
                diagnostic_count,
                last_error,
                updated_at_ms
            FROM analytics_sync_state
            WHERE provider = ?1
            ",
            [PROVIDER],
            |row| {
                Ok(SyncStatus {
                    status: row.get(0)?,
                    phase: row.get(1)?,
                    processed_records: row.get(2)?,
                    diagnostic_count: row.get(3)?,
                    last_error: row.get(4)?,
                    updated_at_ms: row.get(5)?,
                })
            },
        )
        .optional()
        .map(|status| status.unwrap_or_default())
        .map_err(|source| DatabaseError::sqlite("query the analytics sync status", source))
}

fn query_overall_summary(connection: &Connection) -> Result<OverallSummary, DatabaseError> {
    connection
        .query_row(
            "
            SELECT
                (SELECT COUNT(*) FROM agent_sessions),
                (SELECT COALESCE(SUM(delta_tokens), 0) FROM token_usage_records),
                (SELECT COUNT(*) FROM skill_invocations),
                (SELECT COUNT(*) FROM session_turns WHERE status = 'succeeded'),
                (SELECT COUNT(*) FROM session_turns WHERE status = 'failed'),
                (SELECT COUNT(*) FROM session_turns WHERE status = 'cancelled'),
                (SELECT COUNT(*) FROM session_turns WHERE status = 'in_progress'),
                (SELECT COUNT(*) FROM session_turns WHERE status = 'unknown')
            ",
            [],
            |row| {
                Ok(OverallSummary {
                    session_count: row.get(0)?,
                    total_tokens: row.get(1)?,
                    skill_invocation_count: row.get(2)?,
                    succeeded_turn_count: row.get(3)?,
                    failed_turn_count: row.get(4)?,
                    cancelled_turn_count: row.get(5)?,
                    active_turn_count: row.get(6)?,
                    unknown_turn_count: row.get(7)?,
                })
            },
        )
        .map_err(|source| DatabaseError::sqlite("query the analytics summary", source))
}

fn query_sessions(connection: &Connection) -> Result<Vec<SessionSummary>, DatabaseError> {
    let mut statement = connection
        .prepare(
            "
            SELECT
                sessions.id,
                sessions.title,
                sessions.project_name,
                sessions.created_at_ms,
                sessions.updated_at_ms,
                COALESCE(turns.observed_duration_ms, 0),
                CASE
                    WHEN sessions.created_at_ms IS NOT NULL
                        AND sessions.updated_at_ms IS NOT NULL
                    THEN MAX(sessions.updated_at_ms - sessions.created_at_ms, 0)
                END,
                CASE
                    WHEN COALESCE(usage.tokens_used, 0) > 0
                    THEN usage.tokens_used
                    ELSE COALESCE(sessions.tokens_used, 0)
                END,
                COALESCE(turns.turn_count, 0),
                COALESCE(skills.skill_count, 0),
                sessions.archived,
                COALESCE((
                    SELECT latest.status
                    FROM session_turns latest
                    WHERE latest.session_id = sessions.id
                    ORDER BY
                        COALESCE(latest.completed_at_ms, latest.started_at_ms, 0) DESC,
                        latest.id DESC
                    LIMIT 1
                ), 'unknown')
            FROM agent_sessions sessions
            LEFT JOIN (
                SELECT
                    session_id,
                    COUNT(*) AS turn_count,
                    COALESCE(SUM(duration_ms), 0) AS observed_duration_ms
                FROM session_turns
                GROUP BY session_id
            ) turns ON turns.session_id = sessions.id
            LEFT JOIN (
                SELECT session_id, COUNT(*) AS skill_count
                FROM skill_invocations
                GROUP BY session_id
            ) skills ON skills.session_id = sessions.id
            LEFT JOIN (
                SELECT session_id, COALESCE(SUM(delta_tokens), 0) AS tokens_used
                FROM token_usage_records
                GROUP BY session_id
            ) usage ON usage.session_id = sessions.id
            ORDER BY COALESCE(sessions.updated_at_ms, sessions.created_at_ms, 0) DESC
            ",
        )
        .map_err(|source| DatabaseError::sqlite("prepare the session summary query", source))?;
    let rows = statement
        .query_map([], map_session)
        .map_err(|source| DatabaseError::sqlite("query session summaries", source))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read session summaries", source))
}

fn map_session(row: &Row<'_>) -> rusqlite::Result<SessionSummary> {
    Ok(SessionSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        project_name: row.get(2)?,
        created_at_ms: row.get(3)?,
        updated_at_ms: row.get(4)?,
        observed_duration_ms: row.get(5)?,
        wall_duration_ms: row.get(6)?,
        tokens_used: row.get(7)?,
        turn_count: row.get(8)?,
        skill_invocation_count: row.get(9)?,
        archived: row.get::<_, i64>(10)? != 0,
        status: row.get(11)?,
    })
}

fn query_skills(connection: &Connection) -> Result<Vec<SkillSummary>, DatabaseError> {
    let mut statement = connection
        .prepare(
            "
            SELECT
                skill_name,
                COUNT(*) AS invocation_count,
                SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status IN ('unknown', 'in_progress') THEN 1 ELSE 0 END),
                AVG(duration_ms),
                MAX(duration_ms),
                AVG(total_tokens),
                MAX(total_tokens)
            FROM skill_invocations
            GROUP BY skill_name
            ORDER BY invocation_count DESC, skill_name ASC
            ",
        )
        .map_err(|source| DatabaseError::sqlite("prepare the skill summary query", source))?;
    let rows = statement
        .query_map([], |row| {
            let succeeded_count: i64 = row.get(2)?;
            let failed_count: i64 = row.get(3)?;
            let decided_count = succeeded_count + failed_count;
            Ok(SkillSummary {
                name: row.get(0)?,
                invocation_count: row.get(1)?,
                succeeded_count,
                failed_count,
                cancelled_count: row.get(4)?,
                unknown_count: row.get(5)?,
                success_rate: (decided_count > 0)
                    .then_some(succeeded_count as f64 / decided_count as f64),
                average_duration_ms: row.get(6)?,
                max_duration_ms: row.get(7)?,
                average_tokens: row.get(8)?,
                max_tokens: row.get(9)?,
            })
        })
        .map_err(|source| DatabaseError::sqlite("query skill summaries", source))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read skill summaries", source))
}
