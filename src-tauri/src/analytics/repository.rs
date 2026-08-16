use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(any(not(feature = "e2e"), test))]
use coding_agent_data::ScanProgress;
use rusqlite::types::Type;
#[cfg(any(not(feature = "e2e"), test))]
use rusqlite::Transaction;
use rusqlite::{params, Connection, OptionalExtension, Row};

#[cfg(test)]
use super::model::{AnalyticsSnapshot, OverallSummary, SessionSummary};
use super::model::{
    SessionDetail, SessionEventItem, SessionListItem, SessionPage, SessionPageRequest,
    SkillAnalysis, SkillSummary, SyncStatus,
};
use crate::database::{Database, DatabaseError};

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(not(feature = "e2e"))]
pub(super) fn load_checkpoint(
    database: &Database,
    source_id: &str,
) -> Result<Option<String>, DatabaseError> {
    database
        .connect()?
        .query_row(
            "
            SELECT checkpoint_json
            FROM provider_sync_state
            WHERE source_id = ?1
            ",
            [source_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|source| DatabaseError::sqlite("load the analytics checkpoint", source))
        .map(Option::flatten)
}

#[cfg(not(feature = "e2e"))]
pub(super) fn update_sync_status(
    database: &Database,
    provider: &str,
    source_id: &str,
    status: &str,
    phase: &str,
    last_error: Option<&str>,
) -> Result<(), DatabaseError> {
    database
        .connect()?
        .execute(
            "
            INSERT INTO provider_sync_state (
                source_id,
                provider,
                status,
                phase,
                last_error,
                updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(source_id) DO UPDATE SET
                provider = excluded.provider,
                status = excluded.status,
                phase = excluded.phase,
                last_error = excluded.last_error,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![source_id, provider, status, phase, last_error, now_ms()],
        )
        .map_err(|source| DatabaseError::sqlite("update the analytics sync status", source))?;
    Ok(())
}

#[cfg(any(not(feature = "e2e"), test))]
pub(super) struct BatchState<'a> {
    pub provider: &'a str,
    pub source_id: &'a str,
    pub checkpoint_json: &'a str,
    pub status: &'a str,
    pub phase: &'a str,
    pub processed_records: usize,
    pub diagnostic_count: usize,
    pub progress: Option<&'a ScanProgress>,
    pub estimated_remaining_ms: Option<i64>,
}

#[cfg(any(not(feature = "e2e"), test))]
pub(super) fn save_batch_state(
    transaction: &Transaction<'_>,
    state: BatchState<'_>,
) -> Result<(), DatabaseError> {
    transaction
        .execute(
            "
            INSERT INTO provider_sync_state (
                source_id,
                provider,
                checkpoint_json,
                status,
                phase,
                processed_records,
                diagnostic_count,
                last_error,
                updated_at_ms,
                total_files,
                processed_files,
                processed_lines,
                estimated_total_lines,
                current_file,
                current_line,
                estimated_remaining_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(source_id) DO UPDATE SET
                provider = excluded.provider,
                checkpoint_json = excluded.checkpoint_json,
                status = excluded.status,
                phase = excluded.phase,
                processed_records = provider_sync_state.processed_records
                    + excluded.processed_records,
                diagnostic_count = provider_sync_state.diagnostic_count
                    + excluded.diagnostic_count,
                last_error = NULL,
                updated_at_ms = excluded.updated_at_ms,
                total_files = CASE WHEN ?16 THEN excluded.total_files ELSE provider_sync_state.total_files END,
                processed_files = CASE WHEN ?16 THEN excluded.processed_files ELSE provider_sync_state.processed_files END,
                processed_lines = CASE WHEN ?16 THEN excluded.processed_lines ELSE provider_sync_state.processed_lines END,
                estimated_total_lines = CASE WHEN ?16 THEN excluded.estimated_total_lines ELSE provider_sync_state.estimated_total_lines END,
                current_file = CASE WHEN ?16 THEN excluded.current_file ELSE provider_sync_state.current_file END,
                current_line = CASE WHEN ?16 THEN excluded.current_line ELSE provider_sync_state.current_line END,
                estimated_remaining_ms = excluded.estimated_remaining_ms
            ",
            params![
                state.source_id,
                state.provider,
                state.checkpoint_json,
                state.status,
                state.phase,
                state.processed_records as i64,
                state.diagnostic_count as i64,
                now_ms(),
                state.progress.map(|progress| progress.total_files as i64).unwrap_or_default(),
                state
                    .progress
                    .map(|progress| progress.processed_files as i64)
                    .unwrap_or_default(),
                state
                    .progress
                    .map(|progress| progress.processed_lines as i64)
                    .unwrap_or_default(),
                state
                    .progress
                    .and_then(|progress| progress.estimated_total_lines)
                    .map(|value| value as i64),
                state
                    .progress
                    .and_then(|progress| progress.current_file.as_deref()),
                state
                    .progress
                    .map(|progress| progress.current_line as i64)
                    .unwrap_or_default(),
                state.estimated_remaining_ms,
                state.progress.is_some(),
            ],
        )
        .map_err(|source| DatabaseError::sqlite("save the analytics checkpoint", source))?;
    Ok(())
}

#[cfg(not(feature = "e2e"))]
pub(super) fn update_sync_progress(
    database: &Database,
    provider: &str,
    source_id: &str,
    progress: &ScanProgress,
) -> Result<(), DatabaseError> {
    database
        .connect()?
        .execute(
            "
            UPDATE provider_sync_state
            SET total_files = ?1,
                processed_files = ?2,
                processed_lines = ?3,
                estimated_total_lines = ?4,
                current_file = ?5,
                current_line = ?6,
                updated_at_ms = ?7
            WHERE provider = ?8 AND source_id = ?9
            ",
            params![
                progress.total_files as i64,
                progress.processed_files as i64,
                progress.processed_lines as i64,
                progress.estimated_total_lines.map(|value| value as i64),
                progress.current_file.as_deref(),
                progress.current_line as i64,
                now_ms(),
                provider,
                source_id,
            ],
        )
        .map_err(|source| DatabaseError::sqlite("update the analytics scan progress", source))?;
    Ok(())
}

pub(super) fn sync_status(database: &Database) -> Result<Vec<SyncStatus>, DatabaseError> {
    let connection = database.connect()?;
    let mut statement = connection
        .prepare(
            "
            SELECT provider, source_id, status, phase, total_files, processed_files,
                   processed_lines, estimated_total_lines, current_file, current_line,
                   estimated_remaining_ms, last_error, updated_at_ms
            FROM provider_sync_state
            ORDER BY provider, source_id
            ",
        )
        .map_err(|source| {
            DatabaseError::sqlite("prepare the analytics sync status query", source)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok(SyncStatus {
                provider: row.get(0)?,
                source_id: row.get(1)?,
                status: row.get(2)?,
                phase: row.get(3)?,
                total_files: row.get(4)?,
                processed_files: row.get(5)?,
                processed_lines: row.get(6)?,
                estimated_total_lines: row.get(7)?,
                current_file: row.get(8)?,
                current_line: row.get(9)?,
                estimated_remaining_ms: row.get(10)?,
                last_error: row.get(11)?,
                updated_at_ms: row.get(12)?,
            })
        })
        .map_err(|source| DatabaseError::sqlite("query the analytics sync status", source))?;
    rows.collect::<Result<_, _>>()
        .map_err(|source| DatabaseError::sqlite("read the analytics sync status", source))
}

#[cfg(test)]
pub(super) fn analytics_snapshot(database: &Database) -> Result<AnalyticsSnapshot, DatabaseError> {
    let connection = database.connect()?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|source| DatabaseError::sqlite("begin the analytics snapshot", source))?;
    let snapshot = AnalyticsSnapshot {
        summary: query_overall_summary(&transaction)?,
        sessions: query_sessions(&transaction)?,
        skills: query_skills(&transaction)?,
    };
    transaction
        .commit()
        .map_err(|source| DatabaseError::sqlite("finish the analytics snapshot", source))?;
    Ok(snapshot)
}

pub(super) fn skill_analysis(database: &Database) -> Result<SkillAnalysis, DatabaseError> {
    let connection = database.connect()?;
    let analysis = SkillAnalysis {
        skills: query_skills(&connection)?,
    };
    Ok(analysis)
}

pub(super) fn session_page(
    database: &Database,
    request: SessionPageRequest,
) -> Result<SessionPage, DatabaseError> {
    let connection = database.connect()?;
    let page_size = if request.page_size == 0 {
        25
    } else {
        request.page_size.clamp(10, 100)
    };
    let requested_page = request.page.max(1);
    let query = request.query.as_deref().map(str::trim).unwrap_or_default();
    let search_pattern = format!("%{}%", escape_like(query));
    let archived = request.archived.map(i64::from);
    let total: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM agent_sessions sessions
            WHERE (
                ?1 = ''
                OR sessions.title LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                OR sessions.project_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                OR sessions.cwd LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                OR sessions.source_session_id LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                OR sessions.model LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                OR sessions.git_branch LIKE ?2 ESCAPE '\\' COLLATE NOCASE
            )
              AND (?3 IS NULL OR sessions.archived = ?3)
            ",
            params![query, search_pattern, archived],
            |row| row.get(0),
        )
        .map_err(|source| DatabaseError::sqlite("count session history rows", source))?;
    let page_count = ((total + i64::from(page_size) - 1) / i64::from(page_size)).max(1);
    let page = i64::from(requested_page).min(page_count) as u32;
    let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);

    let mut statement = connection
        .prepare(
            "
            WITH page_sessions AS MATERIALIZED (
                SELECT *
                FROM agent_sessions sessions
                WHERE (
                    ?1 = ''
                    OR sessions.title LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                    OR sessions.project_name LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                    OR sessions.cwd LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                    OR sessions.source_session_id LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                    OR sessions.model LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                    OR sessions.git_branch LIKE ?2 ESCAPE '\\' COLLATE NOCASE
                )
                  AND (?3 IS NULL OR sessions.archived = ?3)
                ORDER BY
                    COALESCE(sessions.updated_at_ms, sessions.created_at_ms, 0) DESC,
                    sessions.id DESC
                LIMIT ?4 OFFSET ?5
            )
            SELECT
                sessions.id,
                sessions.source_session_id,
                sessions.provider,
                sessions.title,
                sessions.project_name,
                sessions.cwd,
                sessions.created_at_ms,
                sessions.updated_at_ms,
                COALESCE(NULLIF(MAX((
                        SELECT SUM(usage.delta_tokens)
                        FROM token_usage_records usage
                        WHERE usage.session_id = sessions.id
                    ), 0), 0), sessions.tokens_used, 0),
                (
                    SELECT COUNT(*)
                    FROM agent_invocations invocations
                    WHERE invocations.session_id = sessions.id
                ),
                (
                    SELECT COUNT(*)
                    FROM skill_invocations skills
                    WHERE skills.session_id = sessions.id
                ),
                (
                    SELECT COUNT(*)
                    FROM session_events events
                    WHERE events.session_id = sessions.id
                ),
                sessions.archived,
                COALESCE((
                    SELECT latest.status
                    FROM agent_invocations latest
                    WHERE latest.session_id = sessions.id
                    ORDER BY
                        COALESCE(latest.completed_at_ms, latest.started_at_ms, 0) DESC,
                        latest.id DESC
                    LIMIT 1
                ), 'unknown'),
                sessions.model,
                sessions.model_provider,
                sessions.agent_version,
                sessions.git_branch
            FROM page_sessions sessions
            ORDER BY
                COALESCE(sessions.updated_at_ms, sessions.created_at_ms, 0) DESC,
                sessions.id DESC
            ",
        )
        .map_err(|source| DatabaseError::sqlite("prepare the paginated session query", source))?;
    let rows = statement
        .query_map(
            params![
                query,
                search_pattern,
                archived,
                i64::from(page_size),
                offset
            ],
            map_session_list_item,
        )
        .map_err(|source| DatabaseError::sqlite("query paginated sessions", source))?;
    let items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read paginated sessions", source))?;

    Ok(SessionPage {
        items,
        page,
        page_size,
        total,
    })
}

pub(super) fn session_detail(
    database: &Database,
    session_id: &str,
) -> Result<Option<SessionDetail>, DatabaseError> {
    let connection = database.connect()?;
    let Some(session) = query_session_list_item(&connection, session_id)? else {
        return Ok(None);
    };
    let metadata: (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    ) = connection
        .query_row(
            "
            SELECT
                agent_name,
                agent_role,
                git_commit,
                git_remote_url,
                data_quality
            FROM agent_sessions
            WHERE id = ?1
            ",
            [session_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|source| DatabaseError::sqlite("query session detail metadata", source))?;
    let mut statement = connection
        .prepare(
            "
            SELECT
                id,
                invocation_id,
                timestamp_ms,
                event_type,
                event_json
            FROM session_events
            WHERE session_id = ?1
            ORDER BY
                COALESCE(logical_ordinal, sequence_position),
                sequence_position,
                sequence_part,
                id
            ",
        )
        .map_err(|source| DatabaseError::sqlite("prepare the session event query", source))?;
    let rows = statement
        .query_map([session_id], |row| {
            let event_json: String = row.get(4)?;
            let event = serde_json::from_str(&event_json).map_err(|source| {
                rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(source))
            })?;
            Ok(SessionEventItem {
                id: row.get(0)?,
                invocation_id: row.get(1)?,
                timestamp_ms: row.get(2)?,
                event_type: row.get(3)?,
                event,
            })
        })
        .map_err(|source| DatabaseError::sqlite("query session events", source))?;
    let events = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read session events", source))?;

    Ok(Some(SessionDetail {
        session,
        agent_name: metadata.0,
        agent_role: metadata.1,
        git_commit: metadata.2,
        git_remote_url: metadata.3,
        data_quality: metadata.4,
        events,
    }))
}

fn query_session_list_item(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<SessionListItem>, DatabaseError> {
    connection
        .query_row(
            "
            SELECT
                sessions.id,
                sessions.source_session_id,
                sessions.provider,
                sessions.title,
                sessions.project_name,
                sessions.cwd,
                sessions.created_at_ms,
                sessions.updated_at_ms,
                COALESCE(NULLIF(MAX((
                        SELECT SUM(usage.delta_tokens)
                        FROM token_usage_records usage
                        WHERE usage.session_id = sessions.id
                    ), 0), 0), sessions.tokens_used, 0),
                COALESCE((
                    SELECT COUNT(*)
                    FROM agent_invocations
                    WHERE session_id = sessions.id
                ), 0),
                COALESCE((
                    SELECT COUNT(*)
                    FROM skill_invocations
                    WHERE session_id = sessions.id
                ), 0),
                COALESCE((
                    SELECT COUNT(*)
                    FROM session_events
                    WHERE session_id = sessions.id
                ), 0),
                sessions.archived,
                COALESCE((
                    SELECT latest.status
                    FROM agent_invocations latest
                    WHERE latest.session_id = sessions.id
                    ORDER BY
                        COALESCE(latest.completed_at_ms, latest.started_at_ms, 0) DESC,
                        latest.id DESC
                    LIMIT 1
                ), 'unknown'),
                sessions.model,
                sessions.model_provider,
                sessions.agent_version,
                sessions.git_branch
            FROM agent_sessions sessions
            WHERE sessions.id = ?1
            ",
            [session_id],
            map_session_list_item,
        )
        .optional()
        .map_err(|source| DatabaseError::sqlite("query session history metadata", source))
}

fn map_session_list_item(row: &Row<'_>) -> rusqlite::Result<SessionListItem> {
    Ok(SessionListItem {
        id: row.get(0)?,
        source_session_id: row.get(1)?,
        provider: row.get(2)?,
        title: row.get(3)?,
        project_name: row.get(4)?,
        cwd: row.get(5)?,
        created_at_ms: row.get(6)?,
        updated_at_ms: row.get(7)?,
        tokens_used: row.get(8)?,
        invocation_count: row.get(9)?,
        skill_invocation_count: row.get(10)?,
        event_count: row.get(11)?,
        archived: row.get::<_, i64>(12)? != 0,
        status: row.get(13)?,
        model: row.get(14)?,
        model_provider: row.get(15)?,
        agent_version: row.get(16)?,
        git_branch: row.get(17)?,
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
fn query_overall_summary(connection: &Connection) -> Result<OverallSummary, DatabaseError> {
    connection
        .query_row(
            "
            SELECT
                (SELECT COUNT(*) FROM agent_sessions),
                (SELECT COALESCE(SUM(delta_tokens), 0) FROM token_usage_records),
                (SELECT COUNT(*) FROM skill_invocations),
                (SELECT COUNT(*) FROM agent_invocations WHERE status = 'succeeded'),
                (SELECT COUNT(*) FROM agent_invocations WHERE status = 'failed'),
                (SELECT COUNT(*) FROM agent_invocations WHERE status = 'cancelled'),
                (SELECT COUNT(*) FROM agent_invocations WHERE status = 'in_progress'),
                (SELECT COUNT(*) FROM agent_invocations WHERE status = 'unknown')
            ",
            [],
            |row| {
                Ok(OverallSummary {
                    session_count: row.get(0)?,
                    total_tokens: row.get(1)?,
                    skill_invocation_count: row.get(2)?,
                    succeeded_invocation_count: row.get(3)?,
                    failed_invocation_count: row.get(4)?,
                    cancelled_invocation_count: row.get(5)?,
                    active_invocation_count: row.get(6)?,
                    unknown_invocation_count: row.get(7)?,
                })
            },
        )
        .map_err(|source| DatabaseError::sqlite("query the analytics summary", source))
}

#[cfg(test)]
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
                COALESCE(invocations.observed_duration_ms, 0),
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
                COALESCE(invocations.invocation_count, 0),
                COALESCE(skills.skill_count, 0),
                sessions.archived,
                COALESCE((
                    SELECT latest.status
                    FROM agent_invocations latest
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
                    COUNT(*) AS invocation_count,
                    COALESCE(SUM(duration_ms), 0) AS observed_duration_ms
                FROM agent_invocations
                GROUP BY session_id
            ) invocations ON invocations.session_id = sessions.id
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

#[cfg(test)]
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
        invocation_count: row.get(8)?,
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
