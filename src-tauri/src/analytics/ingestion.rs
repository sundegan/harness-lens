use std::collections::BTreeSet;
use std::path::Path;
#[cfg(not(feature = "e2e"))]
use std::path::PathBuf;
#[cfg(not(feature = "e2e"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(feature = "e2e"))]
use std::sync::Arc;
#[cfg(not(feature = "e2e"))]
use std::thread::{self, JoinHandle};
#[cfg(not(feature = "e2e"))]
use std::time::Duration;

#[cfg(not(feature = "e2e"))]
use coding_agent_data::providers::codex::CodexProvider;
use coding_agent_data::{
    Batch, Change, DataQuality, ItemData, Record, RecordData, Session, ToolCall, ToolResult,
    ToolStatus, Turn, TurnStatus, Usage,
};
#[cfg(not(feature = "e2e"))]
use coding_agent_data::{Checkpoint, Provider, WatchProvider};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;
#[cfg(not(feature = "e2e"))]
use tauri::{AppHandle, Emitter};

use super::repository;
use crate::database::Database;

#[cfg(not(feature = "e2e"))]
const ANALYTICS_UPDATED_EVENT: &str = "analytics-updated";

#[cfg(not(feature = "e2e"))]
pub struct AgentDataMonitor {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(not(feature = "e2e"))]
impl AgentDataMonitor {
    pub fn start(database_path: PathBuf, app: AppHandle) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("harness-lens-agent-data".to_owned())
            .spawn(move || run(database_path, app, worker_stop))?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

#[cfg(not(feature = "e2e"))]
impl Drop for AgentDataMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(not(feature = "e2e"))]
fn run(database_path: PathBuf, app: AppHandle, stop: Arc<AtomicBool>) {
    let database = match Database::initialize(database_path) {
        Ok(database) => database,
        Err(error) => {
            log::error!("failed to open the analytics database: {error}");
            return;
        }
    };
    let provider = match CodexProvider::discover() {
        Ok(provider) => provider,
        Err(error) => {
            log::warn!("Codex data monitoring is unavailable: {error}");
            set_error_status(&database, "unavailable", "discovery", &error.to_string());
            emit_updated(&app);
            return;
        }
    };

    log::info!("starting Codex analytics synchronization");
    let stored_checkpoint = match repository::load_checkpoint(&database) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            log::warn!("failed to load the analytics checkpoint: {error}");
            None
        }
    };
    let checkpoint =
        stored_checkpoint
            .as_deref()
            .and_then(|value| match Checkpoint::from_json(value) {
                Ok(checkpoint) => Some(checkpoint),
                Err(error) => {
                    log::warn!("the analytics checkpoint is invalid and will be rebuilt: {error}");
                    None
                }
            });

    if checkpoint.is_none() {
        if let Err(error) = reset_provider_data(&database) {
            set_error_status(&database, "error", "initial_scan", &error);
            emit_updated(&app);
            return;
        }
    }
    if let Err(error) = repository::update_sync_status(&database, "syncing", "initial_scan", None) {
        log::warn!("failed to publish the analytics sync status: {error}");
    }
    emit_updated(&app);

    let mut batch = match provider.scan(checkpoint.as_ref()) {
        Ok(batch) => batch,
        Err(error) if checkpoint.is_some() => {
            log::warn!(
                "the saved Codex checkpoint could not be used ({error}); rebuilding analytics"
            );
            if let Err(reset_error) = reset_provider_data(&database) {
                set_error_status(&database, "error", "initial_scan", &reset_error);
                emit_updated(&app);
                return;
            }
            match provider.scan(None) {
                Ok(batch) => batch,
                Err(error) => {
                    set_error_status(&database, "error", "initial_scan", &error.to_string());
                    emit_updated(&app);
                    return;
                }
            }
        }
        Err(error) => {
            set_error_status(&database, "error", "initial_scan", &error.to_string());
            emit_updated(&app);
            return;
        }
    };

    loop {
        let has_more = batch.has_more;
        if let Err(error) = apply_batch(
            &database,
            &batch,
            if has_more { "syncing" } else { "ready" },
            if has_more { "initial_scan" } else { "watching" },
        ) {
            log::error!("failed to import Codex analytics: {error}");
            set_error_status(&database, "error", "initial_scan", &error);
            emit_updated(&app);
            return;
        }
        log_batch("initial", &batch);
        emit_updated(&app);

        if !has_more || stop.load(Ordering::Acquire) {
            break;
        }
        batch = match provider.scan(Some(&batch.checkpoint)) {
            Ok(batch) => batch,
            Err(error) => {
                set_error_status(&database, "error", "initial_scan", &error.to_string());
                emit_updated(&app);
                return;
            }
        };
    }
    if stop.load(Ordering::Acquire) {
        return;
    }

    let watcher = match provider.watch(batch.checkpoint) {
        Ok(watcher) => watcher,
        Err(error) => {
            set_error_status(&database, "error", "watching", &error.to_string());
            emit_updated(&app);
            return;
        }
    };
    while !stop.load(Ordering::Acquire) {
        match watcher.recv_timeout(Duration::from_millis(250)) {
            Ok(Some(batch)) => {
                let status = if batch.has_more { "syncing" } else { "ready" };
                if let Err(error) = apply_batch(&database, &batch, status, "watching") {
                    log::error!("failed to import incremental Codex analytics: {error}");
                    set_error_status(&database, "error", "watching", &error);
                    emit_updated(&app);
                    return;
                }
                log_batch("incremental", &batch);
                emit_updated(&app);
            }
            Ok(None) => {}
            Err(coding_agent_data::Error::SubscriptionClosed) => {
                if !stop.load(Ordering::Acquire) {
                    let message = "Codex data monitoring stopped unexpectedly";
                    log::error!("{message}");
                    set_error_status(&database, "error", "watching", message);
                    emit_updated(&app);
                }
                return;
            }
            Err(error) => {
                log::warn!("Codex data monitoring transient error: {error}");
            }
        }
    }
}

#[cfg(not(feature = "e2e"))]
fn set_error_status(database: &Database, status: &str, phase: &str, error: &str) {
    if let Err(status_error) = repository::update_sync_status(database, status, phase, Some(error))
    {
        log::error!("failed to record analytics error status: {status_error}");
    }
}

#[cfg(not(feature = "e2e"))]
fn emit_updated(app: &AppHandle) {
    if let Err(error) = app.emit(ANALYTICS_UPDATED_EVENT, ()) {
        log::warn!("failed to emit analytics update: {error}");
    }
}

#[cfg(not(feature = "e2e"))]
fn reset_provider_data(database: &Database) -> Result<(), String> {
    let mut connection = database.connect().map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin the analytics reset: {error}"))?;
    transaction
        .execute(
            "DELETE FROM agent_sessions WHERE provider = ?1",
            [repository::PROVIDER],
        )
        .map_err(|error| format!("failed to reset sessions: {error}"))?;
    transaction
        .execute(
            "DELETE FROM rollout_sources WHERE provider = ?1",
            [repository::PROVIDER],
        )
        .map_err(|error| format!("failed to reset rollout state: {error}"))?;
    transaction
        .execute(
            "DELETE FROM provider_sync_state WHERE provider = ?1 OR provider LIKE ?2",
            params![repository::PROVIDER, format!("{}:%", repository::PROVIDER)],
        )
        .map_err(|error| format!("failed to reset sync state: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit the analytics reset: {error}"))
}

fn apply_batch(
    database: &Database,
    batch: &Batch,
    status: &str,
    phase: &str,
) -> Result<(), String> {
    let mut connection = database.connect().map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin an analytics import: {error}"))?;

    for change in &batch.changes {
        match change {
            Change::Upsert(record) => import_record(&transaction, record)?,
            Change::Delete(id) => {
                let usage_turn_id = transaction
                    .query_row(
                        "SELECT turn_id FROM token_usage_records WHERE id = ?1",
                        [id.as_str()],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()
                    .map_err(|error| {
                        format!("failed to inspect the deleted usage record: {error}")
                    })?
                    .flatten();
                transaction
                    .execute(
                        "DELETE FROM token_usage_records WHERE id = ?1",
                        [id.as_str()],
                    )
                    .map_err(|error| format!("failed to delete a usage record: {error}"))?;
                transaction
                    .execute("DELETE FROM session_turns WHERE id = ?1", [id.as_str()])
                    .map_err(|error| format!("failed to delete a turn record: {error}"))?;
                transaction
                    .execute(
                        "DELETE FROM agent_sessions WHERE provider = ?1 AND id = ?2",
                        params![repository::PROVIDER, id.as_str()],
                    )
                    .map_err(|error| format!("failed to delete a session record: {error}"))?;
                if let Some(turn_id) = usage_turn_id {
                    refresh_turn_tokens(&transaction, &turn_id)?;
                }
            }
            Change::Reset(source) => {
                reset_rollout_source(&transaction, &source.path)?;
            }
            Change::Remove(source) => {
                remove_rollout_source(&transaction, &source.path)?;
            }
            _ => {}
        }
    }

    let checkpoint_json = batch
        .checkpoint
        .to_json()
        .map_err(|error| format!("failed to serialize the analytics checkpoint: {error}"))?;
    repository::save_batch_state(
        &transaction,
        &checkpoint_json,
        status,
        phase,
        batch.changes.len(),
        batch.diagnostics.len(),
    )
    .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit the analytics import: {error}"))
}

fn import_record(transaction: &Transaction<'_>, record: &Record) -> Result<(), String> {
    match &record.data {
        RecordData::Session(session) => import_session(transaction, record, session),
        RecordData::Turn(turn) => import_turn(transaction, record, turn),
        RecordData::Usage(usage) => import_usage(transaction, record, usage),
        RecordData::Item(item) => match &item.data {
            ItemData::ToolCall(call) => remember_skill_read_call(transaction, record, call),
            ItemData::ToolResult(result) => complete_skill_read_call(transaction, record, result),
            _ => Ok(()),
        },
        RecordData::Unknown(_) => Ok(()),
        _ => Ok(()),
    }
}

fn import_session(
    transaction: &Transaction<'_>,
    record: &Record,
    session: &Session,
) -> Result<(), String> {
    let id = record.id.as_str();
    let source_session_id = &session.external_id;
    let title = session.title.as_deref().unwrap_or_default();
    let cwd = session
        .cwd
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let project_name = session
        .cwd
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let rollout_path = session
        .transcript
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let created_at_ms = session.created_at.map(|timestamp| timestamp.as_millis());
    let updated_at_ms = session.updated_at.map(|timestamp| timestamp.as_millis());
    let metadata_present = i64::from(session.quality == DataQuality::Complete);

    if let Some(path) = rollout_path.as_deref() {
        transaction
            .execute(
                "
                UPDATE agent_sessions
                SET rollout_path = NULL
                WHERE rollout_path = ?1
                  AND id <> ?2
                  AND metadata_present = 0
                ",
                params![path, id],
            )
            .map_err(|error| format!("failed to replace fallback session metadata: {error}"))?;
    }
    transaction
        .execute(
            "
            INSERT INTO agent_sessions (
                id,
                provider,
                source_session_id,
                title,
                project_name,
                cwd,
                rollout_path,
                created_at_ms,
                updated_at_ms,
                tokens_used,
                archived,
                metadata_present
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                project_name = excluded.project_name,
                cwd = excluded.cwd,
                rollout_path = excluded.rollout_path,
                created_at_ms = excluded.created_at_ms,
                updated_at_ms = excluded.updated_at_ms,
                tokens_used = excluded.tokens_used,
                archived = excluded.archived,
                metadata_present = excluded.metadata_present
            ",
            params![
                id,
                repository::PROVIDER,
                source_session_id,
                title,
                project_name,
                cwd,
                rollout_path,
                created_at_ms,
                updated_at_ms,
                session.total_tokens,
                session.archived as i64,
                metadata_present
            ],
        )
        .map_err(|error| format!("failed to import session metadata: {error}"))?;

    if let Some(path) = rollout_path.as_deref() {
        upsert_rollout_source(
            transaction,
            path,
            Some(id),
            session.quality == DataQuality::Complete,
        )?;
    }
    Ok(())
}

fn import_turn(transaction: &Transaction<'_>, record: &Record, turn: &Turn) -> Result<(), String> {
    match turn.status {
        TurnStatus::InProgress => import_running_turn(transaction, record, turn),
        TurnStatus::Completed
        | TurnStatus::Failed
        | TurnStatus::Cancelled
        | TurnStatus::Interrupted => import_terminal_turn(transaction, record, turn),
        _ => Ok(()),
    }
}

fn import_running_turn(
    transaction: &Transaction<'_>,
    record: &Record,
    turn: &Turn,
) -> Result<(), String> {
    let Some(session_id) = record.session.as_ref().map(|id| id.as_str()) else {
        return Ok(());
    };
    let path = record.origin.path.to_string_lossy();
    let turn_id = record.id.as_str();
    let started_at_ms = turn.started_at.map(|timestamp| timestamp.as_millis());
    upsert_rollout_source(transaction, &path, Some(session_id), false)?;

    transaction
        .execute(
            "
            UPDATE session_turns
            SET status = 'unknown'
            WHERE source_path = ?1
              AND status = 'in_progress'
              AND id <> ?2
            ",
            params![path, turn_id],
        )
        .map_err(|error| format!("failed to close the previous incomplete turn: {error}"))?;
    transaction
        .execute(
            "
            UPDATE skill_invocations
            SET status = 'unknown'
            WHERE turn_id IN (
                SELECT id
                FROM session_turns
                WHERE source_path = ?1 AND status = 'unknown'
            ) AND status = 'in_progress'
            ",
            [&path],
        )
        .map_err(|error| format!("failed to close incomplete skill invocations: {error}"))?;
    transaction
        .execute(
            "
            INSERT INTO session_turns (
                id,
                session_id,
                source_path,
                started_at_ms,
                status
            ) VALUES (?1, ?2, ?3, ?4, 'in_progress')
            ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                source_path = excluded.source_path,
                started_at_ms = COALESCE(session_turns.started_at_ms, excluded.started_at_ms)
            ",
            params![turn_id, session_id, path, started_at_ms],
        )
        .map_err(|error| format!("failed to import a task start: {error}"))?;
    transaction
        .execute(
            "
            UPDATE rollout_sources
            SET current_turn_id = ?2, updated_at_ms = ?3
            WHERE path = ?1
            ",
            params![path, turn_id, repository::now_ms()],
        )
        .map_err(|error| format!("failed to track the current turn: {error}"))?;
    Ok(())
}

fn remember_skill_read_call(
    transaction: &Transaction<'_>,
    record: &Record,
    call: &ToolCall,
) -> Result<(), String> {
    if !value_contains_text(&call.input, "SKILL.md") {
        return Ok(());
    }
    let path = record.origin.path.to_string_lossy();
    let turn_id = if let Some(turn_id) = &record.turn {
        turn_id.as_str().to_owned()
    } else {
        let Some((turn_id, _, _)) = current_turn(transaction, &path)? else {
            return Ok(());
        };
        turn_id
    };
    if !turn_exists(transaction, &turn_id)? {
        return Ok(());
    }
    transaction
        .execute(
            "
            INSERT INTO pending_skill_reads (source_path, call_id, turn_id)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(source_path, call_id) DO UPDATE SET
                turn_id = excluded.turn_id
            ",
            params![path, call.call_id, turn_id],
        )
        .map_err(|error| format!("failed to remember a possible Skill file read: {error}"))?;
    Ok(())
}

fn complete_skill_read_call(
    transaction: &Transaction<'_>,
    record: &Record,
    result: &ToolResult,
) -> Result<(), String> {
    let path = record.origin.path.to_string_lossy();
    let turn_id: Option<String> = transaction
        .query_row(
            "
            SELECT turn_id
            FROM pending_skill_reads
            WHERE source_path = ?1 AND call_id = ?2
            ",
            params![path, result.call_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("failed to find a pending Skill file read: {error}"))?;
    let Some(turn_id) = turn_id else {
        return Ok(());
    };
    transaction
        .execute(
            "DELETE FROM pending_skill_reads WHERE source_path = ?1 AND call_id = ?2",
            params![path, result.call_id],
        )
        .map_err(|error| format!("failed to finish a pending Skill file read: {error}"))?;

    if matches!(
        result.status,
        ToolStatus::Failed | ToolStatus::Cancelled | ToolStatus::Declined
    ) {
        return Ok(());
    }
    let skill_names = skill_names_from_tool_output(&result.output);
    if skill_names.is_empty() {
        return Ok(());
    }
    let Some((session_id, started_at_ms)) = transaction
        .query_row(
            "SELECT session_id, started_at_ms FROM session_turns WHERE id = ?1",
            [&turn_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .map_err(|error| format!("failed to resolve the Skill invocation turn: {error}"))?
    else {
        return Ok(());
    };
    for skill_name in skill_names {
        let invocation_id = format!("{turn_id}:{skill_name}");
        transaction
            .execute(
                "
                INSERT INTO skill_invocations (
                    id,
                    turn_id,
                    session_id,
                    skill_name,
                    started_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(id) DO NOTHING
                ",
                params![
                    invocation_id,
                    turn_id,
                    session_id,
                    skill_name,
                    started_at_ms
                ],
            )
            .map_err(|error| format!("failed to import a skill invocation: {error}"))?;
    }
    update_skill_metrics(transaction, &turn_id)?;
    Ok(())
}

fn import_usage(
    transaction: &Transaction<'_>,
    record: &Record,
    usage: &Usage,
) -> Result<(), String> {
    let path = record.origin.path.to_string_lossy();
    let session_id = if let Some(session_id) = &record.session {
        Some(session_id.as_str().to_owned())
    } else {
        transaction
            .query_row(
                "SELECT session_id FROM rollout_sources WHERE path = ?1",
                [&path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("failed to find the token usage session: {error}"))?
            .flatten()
    };
    let Some(session_id) = session_id else {
        return Ok(());
    };
    let turn_id = if let Some(turn_id) = &record.turn {
        Some(turn_id.as_str().to_owned())
    } else {
        transaction
            .query_row(
                "SELECT current_turn_id FROM rollout_sources WHERE path = ?1",
                [&path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("failed to find the token usage turn: {error}"))?
            .flatten()
    };
    let turn_id = match turn_id {
        Some(turn_id) if turn_exists(transaction, &turn_id)? => Some(turn_id),
        _ => None,
    };
    let previous_turn_id = transaction
        .query_row(
            "SELECT turn_id FROM token_usage_records WHERE id = ?1",
            [record.id.as_str()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| format!("failed to inspect the existing usage record: {error}"))?
        .flatten();
    transaction
        .execute(
            "
            INSERT INTO token_usage_records (
                id,
                session_id,
                turn_id,
                source_path,
                timestamp_ms,
                cumulative_tokens,
                delta_tokens
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                turn_id = excluded.turn_id,
                source_path = excluded.source_path,
                timestamp_ms = excluded.timestamp_ms,
                cumulative_tokens = excluded.cumulative_tokens,
                delta_tokens = excluded.delta_tokens
            ",
            params![
                record.id.as_str(),
                session_id,
                turn_id,
                path,
                record.timestamp.map(|timestamp| timestamp.as_millis()),
                usage.cumulative.as_ref().map(|usage| usage.total),
                usage.delta.as_ref().map(|usage| usage.total)
            ],
        )
        .map_err(|error| format!("failed to import token usage: {error}"))?;

    if let Some(previous_turn_id) = previous_turn_id.as_deref() {
        if Some(previous_turn_id) != turn_id.as_deref() {
            refresh_turn_tokens(transaction, previous_turn_id)?;
        }
    }
    if let Some(turn_id) = turn_id.as_deref() {
        refresh_turn_tokens(transaction, turn_id)?;
    }
    Ok(())
}

fn import_terminal_turn(
    transaction: &Transaction<'_>,
    record: &Record,
    turn: &Turn,
) -> Result<(), String> {
    let path = record.origin.path.to_string_lossy();
    let turn_id = record.id.as_str();
    let started_at_ms = turn.started_at.map(|timestamp| timestamp.as_millis());
    let completed_at_ms = turn.completed_at.map(|timestamp| timestamp.as_millis());
    let duration_ms = turn
        .duration_ms
        .or_else(|| match (started_at_ms, completed_at_ms) {
            (Some(started), Some(completed)) => Some(completed.saturating_sub(started).max(0)),
            _ => None,
        });
    let status = match turn.status {
        TurnStatus::Completed => "succeeded",
        TurnStatus::Failed => "failed",
        TurnStatus::Cancelled => "cancelled",
        // Analytics exposes success, failure, cancellation, and unknown. A
        // provider interruption is a terminal cancellation in that contract.
        TurnStatus::Interrupted => "cancelled",
        TurnStatus::InProgress => "in_progress",
        _ => "unknown",
    };

    if let Some(session_id) = record.session.as_ref().map(|id| id.as_str()) {
        upsert_rollout_source(transaction, &path, Some(session_id), false)?;
        transaction
            .execute(
                "
                INSERT INTO session_turns (
                    id,
                    session_id,
                    source_path,
                    started_at_ms,
                    status
                ) VALUES (?1, ?2, ?3, ?4, 'in_progress')
                ON CONFLICT(id) DO NOTHING
                ",
                params![turn_id, session_id, path, started_at_ms],
            )
            .map_err(|error| format!("failed to recover a turn without a start event: {error}"))?;
    }
    let updated = transaction
        .execute(
            "
            UPDATE session_turns
            SET
                started_at_ms = COALESCE(started_at_ms, ?2),
                completed_at_ms = ?3,
                duration_ms = COALESCE(
                    ?4,
                    CASE
                        WHEN ?3 IS NOT NULL
                            AND COALESCE(started_at_ms, ?2) IS NOT NULL
                        THEN MAX(?3 - COALESCE(started_at_ms, ?2), 0)
                    END
                ),
                status = ?5,
                error_message = ?6
            WHERE id = ?1
            ",
            params![
                turn_id,
                started_at_ms,
                completed_at_ms,
                duration_ms,
                status,
                turn.error
            ],
        )
        .map_err(|error| format!("failed to finish a turn: {error}"))?;
    if updated == 0 {
        return Ok(());
    }
    transaction
        .execute(
            "
            UPDATE rollout_sources
            SET current_turn_id = NULL, updated_at_ms = ?3
            WHERE path = ?1 AND current_turn_id = ?2
            ",
            params![path, turn_id, repository::now_ms()],
        )
        .map_err(|error| format!("failed to clear the completed turn: {error}"))?;
    update_skill_metrics(transaction, turn_id)
}

fn update_skill_metrics(transaction: &Transaction<'_>, turn_id: &str) -> Result<(), String> {
    let (status, started_at_ms, duration_ms, total_tokens): (
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = transaction
        .query_row(
            "
            SELECT status, started_at_ms, duration_ms, total_tokens
            FROM session_turns
            WHERE id = ?1
            ",
            [turn_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| format!("failed to read turn metrics: {error}"))?;
    transaction
        .execute(
            "
            UPDATE skill_invocations
            SET
                started_at_ms = COALESCE(started_at_ms, ?2),
                duration_ms = ?3,
                total_tokens = ?4,
                status = ?5
            WHERE turn_id = ?1
            ",
            params![turn_id, started_at_ms, duration_ms, total_tokens, status],
        )
        .map_err(|error| format!("failed to update skill metrics: {error}"))?;
    Ok(())
}

fn turn_exists(transaction: &Transaction<'_>, turn_id: &str) -> Result<bool, String> {
    transaction
        .query_row(
            "SELECT 1 FROM session_turns WHERE id = ?1",
            [turn_id],
            |_| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(|error| format!("failed to resolve the token usage turn: {error}"))
}

fn refresh_turn_tokens(transaction: &Transaction<'_>, turn_id: &str) -> Result<(), String> {
    let updated = transaction
        .execute(
            "
            UPDATE session_turns
            SET total_tokens = (
                SELECT SUM(delta_tokens)
                FROM token_usage_records
                WHERE turn_id = ?1
            )
            WHERE id = ?1
            ",
            [turn_id],
        )
        .map_err(|error| format!("failed to update turn token usage: {error}"))?;
    if updated > 0 {
        update_skill_metrics(transaction, turn_id)?;
    }
    Ok(())
}

fn remove_rollout_source(transaction: &Transaction<'_>, path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    clear_rollout_records(transaction, &path)?;
    transaction
        .execute("DELETE FROM rollout_sources WHERE path = ?1", [&path])
        .map_err(|error| format!("failed to clear rollout state: {error}"))?;
    Ok(())
}

fn reset_rollout_source(transaction: &Transaction<'_>, path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    clear_rollout_records(transaction, &path)?;
    transaction
        .execute(
            "
            UPDATE rollout_sources
            SET current_turn_id = NULL, updated_at_ms = ?2
            WHERE path = ?1
            ",
            params![path, repository::now_ms()],
        )
        .map_err(|error| format!("failed to reset rollout state: {error}"))?;
    Ok(())
}

fn clear_rollout_records(transaction: &Transaction<'_>, path: &str) -> Result<(), String> {
    transaction
        .execute(
            "DELETE FROM token_usage_records WHERE source_path = ?1",
            [path],
        )
        .map_err(|error| format!("failed to clear rollout token usage: {error}"))?;
    transaction
        .execute("DELETE FROM session_turns WHERE source_path = ?1", [path])
        .map_err(|error| format!("failed to clear rollout turns: {error}"))?;
    Ok(())
}

fn upsert_rollout_source(
    transaction: &Transaction<'_>,
    path: &str,
    session_id: Option<&str>,
    authoritative: bool,
) -> Result<(), String> {
    let previous_session_id = if authoritative {
        transaction
            .query_row(
                "SELECT session_id FROM rollout_sources WHERE path = ?1",
                [path],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| format!("failed to inspect the rollout mapping: {error}"))?
            .flatten()
    } else {
        None
    };
    transaction
        .execute(
            "
            INSERT INTO rollout_sources (path, provider, session_id, updated_at_ms)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(path) DO UPDATE SET
                session_id = CASE
                    WHEN ?5 = 1 THEN excluded.session_id
                    ELSE COALESCE(rollout_sources.session_id, excluded.session_id)
                END,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![
                path,
                repository::PROVIDER,
                session_id,
                repository::now_ms(),
                authoritative as i64
            ],
        )
        .map_err(|error| format!("failed to track a rollout source: {error}"))?;

    if authoritative {
        if let Some(session_id) = session_id {
            transaction
                .execute(
                    "UPDATE session_turns SET session_id = ?2 WHERE source_path = ?1",
                    params![path, session_id],
                )
                .map_err(|error| format!("failed to update rollout turn ownership: {error}"))?;
            transaction
                .execute(
                    "UPDATE token_usage_records SET session_id = ?2 WHERE source_path = ?1",
                    params![path, session_id],
                )
                .map_err(|error| format!("failed to update rollout usage ownership: {error}"))?;
            transaction
                .execute(
                    "
                    UPDATE skill_invocations
                    SET session_id = ?2
                    WHERE turn_id IN (
                        SELECT id FROM session_turns WHERE source_path = ?1
                    )
                    ",
                    params![path, session_id],
                )
                .map_err(|error| format!("failed to update rollout skill ownership: {error}"))?;
        }
        if let Some(previous_session_id) =
            previous_session_id.filter(|previous| Some(previous.as_str()) != session_id)
        {
            transaction
                .execute(
                    "
                    DELETE FROM agent_sessions
                    WHERE id = ?1
                      AND metadata_present = 0
                      AND NOT EXISTS (
                          SELECT 1 FROM rollout_sources WHERE session_id = ?1
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM session_turns WHERE session_id = ?1
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM token_usage_records WHERE session_id = ?1
                      )
                    ",
                    [previous_session_id],
                )
                .map_err(|error| format!("failed to remove fallback session metadata: {error}"))?;
        }
    }
    Ok(())
}

fn current_turn(
    transaction: &Transaction<'_>,
    path: &str,
) -> Result<Option<(String, String, Option<i64>)>, String> {
    transaction
        .query_row(
            "
            SELECT turns.id, turns.session_id, turns.started_at_ms
            FROM rollout_sources sources
            JOIN session_turns turns ON turns.id = sources.current_turn_id
            WHERE sources.path = ?1
            ",
            [path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| format!("failed to resolve the current turn: {error}"))
}

fn skill_names_from_tool_output(output: &Value) -> BTreeSet<String> {
    let mut texts = Vec::new();
    collect_tool_output_text(output, &mut texts);
    let mut names = BTreeSet::new();
    for text in texts {
        let mut in_frontmatter = false;
        let mut name = None;
        for line in text.lines() {
            let line = line.trim();
            if line == "---" {
                if in_frontmatter {
                    if let Some(name) = name.take() {
                        names.insert(name);
                    }
                }
                in_frontmatter = !in_frontmatter;
                continue;
            }
            if in_frontmatter && name.is_none() {
                name = line
                    .strip_prefix("name:")
                    .map(str::trim)
                    .map(|value| value.trim_matches(['\'', '"']))
                    .filter(|value| is_valid_skill_name(value))
                    .map(str::to_owned);
            }
        }
    }
    names
}

fn collect_tool_output_text<'a>(value: &'a Value, texts: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => texts.push(text),
        Value::Array(items) => {
            for item in items {
                collect_tool_output_text(item, texts);
            }
        }
        Value::Object(object) => {
            if let Some(text) = object.get("text") {
                collect_tool_output_text(text, texts);
            } else if let Some(output) = object.get("output") {
                collect_tool_output_text(output, texts);
            }
        }
        _ => {}
    }
}

fn value_contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Array(items) => items.iter().any(|item| value_contains_text(item, needle)),
        Value::Object(object) => object
            .values()
            .any(|item| value_contains_text(item, needle)),
        _ => false,
    }
}

fn is_valid_skill_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
}

#[cfg(not(feature = "e2e"))]
fn log_batch(phase: &str, batch: &Batch) {
    let mut upserts = 0;
    let mut deletes = 0;
    let mut resets = 0;
    for change in &batch.changes {
        match change {
            Change::Upsert(_) => upserts += 1,
            Change::Delete(_) => deletes += 1,
            Change::Reset(_) | Change::Remove(_) => resets += 1,
            _ => {}
        }
    }
    log::info!(
        "Codex analytics {phase} batch: upserts={upserts}, deletes={deletes}, source_resets={resets}, diagnostics={}, has_more={}",
        batch.diagnostics.len(),
        batch.has_more
    );
    for diagnostic in batch.diagnostics.iter().take(20) {
        log::warn!(
            "Codex analytics diagnostic [{}]: {}",
            diagnostic.code,
            diagnostic.message
        );
    }
    if batch.diagnostics.len() > 20 {
        log::warn!(
            "{} additional Codex analytics diagnostics were omitted from the log",
            batch.diagnostics.len() - 20
        );
    }
}

#[cfg(test)]
#[path = "ingestion_tests.rs"]
mod tests;
