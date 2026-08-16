use std::collections::{BTreeMap, BTreeSet};
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
use coding_agent_data::providers::{claude_code::ClaudeCodeProvider, codex::CodexProvider};
use coding_agent_data::{
    Actor, AgentInvocation, AgentInvocationStatus, Batch, Change, ContentBlock, DataQuality, Event,
    EventData, MessageRole, Record, RecordData, Retry, Session, ToolCall, ToolKind, ToolResult,
    ToolSourceKind, ToolStatus, UsageReport,
};
#[cfg(not(feature = "e2e"))]
use coding_agent_data::{Checkpoint, Provider, WatchProvider};
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::Value;
use sha2::{Digest, Sha256};
#[cfg(not(feature = "e2e"))]
use tauri::{AppHandle, Emitter};

use super::repository;
use crate::database::Database;

#[cfg(not(feature = "e2e"))]
const ANALYTICS_UPDATED_EVENT: &str = "analytics-updated";

#[cfg(not(feature = "e2e"))]
pub struct AgentDataMonitor {
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

#[derive(Clone, Debug)]
struct ProviderContext {
    provider: String,
    source_id: String,
}

#[derive(Clone, Debug)]
struct FirstUserMessageCandidate {
    event_id: String,
    text: String,
    timestamp_ms: Option<i64>,
    sequence_position: i64,
    sequence_part: i64,
}

#[derive(Clone, Debug)]
struct StoredFirstUserMessage {
    event_id: Option<String>,
    timestamp_ms: Option<i64>,
    sequence_position: Option<i64>,
    sequence_part: Option<i64>,
}

#[cfg(not(feature = "e2e"))]
impl AgentDataMonitor {
    pub fn start(database_path: PathBuf, app: AppHandle) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let codex_path = database_path.clone();
        let codex_app = app.clone();
        let codex_stop = Arc::clone(&stop);
        let codex = thread::Builder::new()
            .name("harness-lens-codex".to_owned())
            .spawn(move || match CodexProvider::discover() {
                Ok(provider) => run_provider(codex_path, codex_app, codex_stop, provider),
                Err(error) => record_discovery_error(codex_path, codex_app, "codex", &error),
            })?;

        let claude_stop = Arc::clone(&stop);
        let claude = thread::Builder::new()
            .name("harness-lens-claude-code".to_owned())
            .spawn(move || match ClaudeCodeProvider::discover() {
                Ok(provider) => run_provider(database_path, app, claude_stop, provider),
                Err(error) => record_discovery_error(database_path, app, "claude-code", &error),
            })?;
        Ok(Self {
            stop,
            workers: vec![codex, claude],
        })
    }
}

#[cfg(not(feature = "e2e"))]
impl Drop for AgentDataMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(not(feature = "e2e"))]
fn record_discovery_error(
    database_path: PathBuf,
    app: AppHandle,
    provider: &str,
    error: &coding_agent_data::Error,
) {
    log::warn!("{provider} data monitoring is unavailable: {error}");
    if let Ok(database) = Database::initialize(database_path) {
        set_error_status(
            &database,
            &ProviderContext {
                provider: provider.to_owned(),
                source_id: format!("{provider}:unavailable"),
            },
            "unavailable",
            "discovery",
            &error.to_string(),
        );
        emit_updated(&app);
    }
}

#[cfg(not(feature = "e2e"))]
fn run_provider<P>(database_path: PathBuf, app: AppHandle, stop: Arc<AtomicBool>, provider: P)
where
    P: Provider + WatchProvider,
{
    let database = match Database::initialize(database_path) {
        Ok(database) => database,
        Err(error) => {
            log::error!("failed to open the analytics database: {error}");
            return;
        }
    };
    let context = ProviderContext {
        provider: provider.info().id.as_str().to_owned(),
        source_id: provider.info().source.as_str().to_owned(),
    };

    log::info!("starting {} analytics synchronization", context.provider);
    let stored_checkpoint = match repository::load_checkpoint(&database, &context.source_id) {
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
        if let Err(error) = reset_provider_data(&database, &context) {
            set_error_status(&database, &context, "error", "initial_scan", &error);
            emit_updated(&app);
            return;
        }
    }
    if let Err(error) = repository::update_sync_status(
        &database,
        &context.provider,
        &context.source_id,
        "syncing",
        "initial_scan",
        None,
    ) {
        log::warn!("failed to publish the analytics sync status: {error}");
    }
    emit_updated(&app);

    let mut batch = match provider.scan(checkpoint.as_ref()) {
        Ok(batch) => batch,
        Err(error) if checkpoint.is_some() => {
            log::warn!(
                "the saved {} checkpoint could not be used ({error}); rebuilding analytics",
                context.provider
            );
            if let Err(reset_error) = reset_provider_data(&database, &context) {
                set_error_status(&database, &context, "error", "initial_scan", &reset_error);
                emit_updated(&app);
                return;
            }
            match provider.scan(None) {
                Ok(batch) => batch,
                Err(error) => {
                    set_error_status(
                        &database,
                        &context,
                        "error",
                        "initial_scan",
                        &error.to_string(),
                    );
                    emit_updated(&app);
                    return;
                }
            }
        }
        Err(error) => {
            set_error_status(
                &database,
                &context,
                "error",
                "initial_scan",
                &error.to_string(),
            );
            emit_updated(&app);
            return;
        }
    };

    loop {
        let has_more = batch.has_more;
        if let Err(error) = apply_batch(
            &database,
            &context,
            &batch,
            if has_more { "syncing" } else { "ready" },
            if has_more { "initial_scan" } else { "watching" },
        ) {
            log::error!("failed to import {} analytics: {error}", context.provider);
            set_error_status(&database, &context, "error", "initial_scan", &error);
            emit_updated(&app);
            return;
        }
        log_diagnostics(&context.provider, &batch);
        emit_updated(&app);

        if !has_more || stop.load(Ordering::Acquire) {
            break;
        }
        batch = match provider.scan(Some(&batch.checkpoint)) {
            Ok(batch) => batch,
            Err(error) => {
                set_error_status(
                    &database,
                    &context,
                    "error",
                    "initial_scan",
                    &error.to_string(),
                );
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
            set_error_status(&database, &context, "error", "watching", &error.to_string());
            emit_updated(&app);
            return;
        }
    };
    while !stop.load(Ordering::Acquire) {
        match watcher.recv_timeout(Duration::from_millis(250)) {
            Ok(Some(batch)) => {
                let status = if batch.has_more { "syncing" } else { "ready" };
                if let Err(error) = apply_batch(&database, &context, &batch, status, "watching") {
                    log::error!(
                        "failed to import incremental {} analytics: {error}",
                        context.provider
                    );
                    set_error_status(&database, &context, "error", "watching", &error);
                    emit_updated(&app);
                    return;
                }
                log_diagnostics(&context.provider, &batch);
                emit_updated(&app);
            }
            Ok(None) => {}
            Err(coding_agent_data::Error::SubscriptionClosed) => {
                if !stop.load(Ordering::Acquire) {
                    let message =
                        format!("{} data monitoring stopped unexpectedly", context.provider);
                    log::error!("{message}");
                    set_error_status(&database, &context, "error", "watching", &message);
                    emit_updated(&app);
                }
                return;
            }
            Err(error) => {
                log::warn!(
                    "{} data monitoring transient error: {error}",
                    context.provider
                );
            }
        }
    }
}

#[cfg(not(feature = "e2e"))]
fn set_error_status(
    database: &Database,
    context: &ProviderContext,
    status: &str,
    phase: &str,
    error: &str,
) {
    if let Err(status_error) = repository::update_sync_status(
        database,
        &context.provider,
        &context.source_id,
        status,
        phase,
        Some(error),
    ) {
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
fn reset_provider_data(database: &Database, context: &ProviderContext) -> Result<(), String> {
    let mut connection = database.connect().map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin the analytics reset: {error}"))?;
    transaction
        .execute(
            "DELETE FROM agent_sessions WHERE source_id = ?1",
            [&context.source_id],
        )
        .map_err(|error| format!("failed to reset sessions: {error}"))?;
    transaction
        .execute(
            "DELETE FROM rollout_sources WHERE source_id = ?1",
            [&context.source_id],
        )
        .map_err(|error| format!("failed to reset rollout state: {error}"))?;
    transaction
        .execute(
            "DELETE FROM provider_sync_state WHERE source_id = ?1",
            [&context.source_id],
        )
        .map_err(|error| format!("failed to reset sync state: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit the analytics reset: {error}"))
}

fn apply_batch(
    database: &Database,
    context: &ProviderContext,
    batch: &Batch,
    status: &str,
    phase: &str,
) -> Result<(), String> {
    let mut connection = database.connect().map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed to begin an analytics import: {error}"))?;
    let mut affected_repeat_invocations = BTreeSet::new();
    let mut first_user_message_candidates = BTreeMap::new();
    let mut touched_message_events = BTreeMap::<String, BTreeSet<String>>::new();
    let mut recompute_first_user_messages = BTreeSet::new();

    for change in &batch.changes {
        match change {
            Change::Upsert(record) => import_record(
                &transaction,
                context,
                record,
                &mut affected_repeat_invocations,
                &mut first_user_message_candidates,
                &mut touched_message_events,
                &mut recompute_first_user_messages,
            )?,
            Change::Delete(id) => {
                mark_deleted_first_user_message(
                    &transaction,
                    id.as_str(),
                    &mut recompute_first_user_messages,
                )?;
                let affected_tool_call = transaction
                    .query_row(
                        "SELECT id, invocation_id, call_event_id = ?1, result_event_id = ?1 FROM mcp_tool_call WHERE call_event_id = ?1 OR result_event_id = ?1",
                        [id.as_str()],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<i64>>(2)?.unwrap_or(0) != 0, row.get::<_, Option<i64>>(3)?.unwrap_or(0) != 0)),
                    )
                    .optional()
                    .map_err(|error| format!("failed to inspect the deleted tool event: {error}"))?;
                let retry_invocation_id = transaction
                    .query_row(
                        "SELECT invocation_id FROM mcp_tool_call_retry WHERE id = ?1",
                        [id.as_str()],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()
                    .map_err(|error| format!("failed to inspect deleted retry evidence: {error}"))?
                    .flatten();
                let usage_invocation_id = transaction
                    .query_row(
                        "SELECT invocation_id FROM token_usage_records WHERE id = ?1",
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
                        "DELETE FROM mcp_tool_call_retry WHERE id = ?1",
                        [id.as_str()],
                    )
                    .map_err(|error| format!("failed to delete retry evidence: {error}"))?;
                transaction
                    .execute("DELETE FROM session_events WHERE id = ?1", [id.as_str()])
                    .map_err(|error| format!("failed to delete a session event: {error}"))?;
                if let Some((tool_call_id, invocation_id, deleted_call, deleted_result)) =
                    affected_tool_call
                {
                    if deleted_call {
                        transaction
                            .execute(
                                "UPDATE mcp_tool_call SET call_event_id = NULL, input_fingerprint = NULL, evidence_quality = 'unknown' WHERE id = ?1",
                                [&tool_call_id],
                            )
                            .map_err(|error| format!("failed to clear deleted tool-call evidence: {error}"))?;
                    }
                    if deleted_result {
                        let fallback_status = projected_call_status(&transaction, &tool_call_id)?;
                        transaction
                            .execute(
                                "UPDATE mcp_tool_call SET result_event_id = NULL, has_result = 0, has_error = 0, completed_at_ms = NULL, duration_ms = NULL, duration_source = 'unknown', status = ?2 WHERE id = ?1",
                                params![tool_call_id, fallback_status],
                            )
                            .map_err(|error| format!("failed to clear deleted tool-result evidence: {error}"))?;
                    }
                    let evidence_count: i64 = transaction
                        .query_row(
                            "SELECT (call_event_id IS NOT NULL) + (result_event_id IS NOT NULL) FROM mcp_tool_call WHERE id = ?1",
                            [&tool_call_id],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);
                    if evidence_count == 0 {
                        transaction
                            .execute("DELETE FROM mcp_tool_call WHERE id = ?1", [&tool_call_id])
                            .map_err(|error| {
                                format!("failed to remove an evidence-free tool call: {error}")
                            })?;
                    }
                    affected_repeat_invocations.extend(invocation_id);
                }
                transaction
                    .execute(
                        "DELETE FROM token_usage_records WHERE id = ?1",
                        [id.as_str()],
                    )
                    .map_err(|error| format!("failed to delete a usage record: {error}"))?;
                transaction
                    .execute("DELETE FROM agent_invocations WHERE id = ?1", [id.as_str()])
                    .map_err(|error| format!("failed to delete a invocation record: {error}"))?;
                transaction
                    .execute(
                        "DELETE FROM agent_sessions WHERE source_id = ?1 AND id = ?2",
                        params![context.source_id, id.as_str()],
                    )
                    .map_err(|error| format!("failed to delete a session record: {error}"))?;
                if let Some(invocation_id) = usage_invocation_id {
                    refresh_invocation_tokens(&transaction, &invocation_id)?;
                }
                affected_repeat_invocations.extend(retry_invocation_id);
            }
            Change::Reset(source) => {
                mark_source_first_user_messages(
                    &transaction,
                    context,
                    &source.path,
                    &mut recompute_first_user_messages,
                )?;
                reset_rollout_source(&transaction, context, &source.path)?;
            }
            Change::Remove(source) => {
                mark_source_first_user_messages(
                    &transaction,
                    context,
                    &source.path,
                    &mut recompute_first_user_messages,
                )?;
                remove_rollout_source(&transaction, context, &source.path)?;
            }
            _ => {}
        }
    }

    refresh_first_user_message_projection(
        &transaction,
        &first_user_message_candidates,
        &touched_message_events,
        &mut recompute_first_user_messages,
    )?;

    for invocation_id in affected_repeat_invocations {
        refresh_repeat_chain(&transaction, Some(&invocation_id))?;
    }

    let checkpoint_json = batch
        .checkpoint
        .to_json()
        .map_err(|error| format!("failed to serialize the analytics checkpoint: {error}"))?;
    repository::save_batch_state(
        &transaction,
        repository::BatchState {
            provider: &context.provider,
            source_id: &context.source_id,
            checkpoint_json: &checkpoint_json,
            status,
            phase,
            processed_records: batch.changes.len(),
            diagnostic_count: batch.diagnostics.len(),
        },
    )
    .map_err(|error| error.to_string())?;
    transaction
        .commit()
        .map_err(|error| format!("failed to commit the analytics import: {error}"))
}

fn projected_call_status(
    transaction: &Transaction<'_>,
    tool_call_id: &str,
) -> Result<&'static str, String> {
    let event_json = transaction
        .query_row(
            "SELECT event.event_json FROM mcp_tool_call call JOIN session_events event ON event.id = call.call_event_id WHERE call.id = ?1",
            [tool_call_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to load the surviving tool-call evidence: {error}"))?;
    let Some(event_json) = event_json else {
        return Ok("unknown");
    };
    let event = serde_json::from_str::<Event>(&event_json)
        .map_err(|error| format!("failed to decode the surviving tool-call evidence: {error}"))?;
    Ok(match event.data {
        EventData::ToolCall(call) => tool_status(call.status),
        _ => "unknown",
    })
}

#[cfg(test)]
fn apply_test_batch(
    database: &Database,
    batch: &Batch,
    status: &str,
    phase: &str,
) -> Result<(), String> {
    apply_batch(
        database,
        &ProviderContext {
            provider: "codex".to_owned(),
            source_id: "codex:test".to_owned(),
        },
        batch,
        status,
        phase,
    )
}

fn import_record(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    affected_repeat_invocations: &mut BTreeSet<String>,
    first_user_message_candidates: &mut BTreeMap<String, FirstUserMessageCandidate>,
    touched_message_events: &mut BTreeMap<String, BTreeSet<String>>,
    recompute_first_user_messages: &mut BTreeSet<String>,
) -> Result<(), String> {
    ensure_record_session(transaction, context, record)?;
    match &record.data {
        RecordData::Session(session) => {
            import_session(transaction, context, record, session)?;
            let first_message_event_id = transaction
                .query_row(
                    "SELECT first_user_message_event_id FROM agent_sessions WHERE id = ?1",
                    [record.id.as_str()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|error| {
                    format!("failed to inspect the imported first user message: {error}")
                })?
                .flatten();
            if first_message_event_id.is_none() {
                recompute_first_user_messages.insert(record.id.as_str().to_owned());
            }
            Ok(())
        }
        RecordData::AgentInvocation(invocation) => {
            import_agent_invocation(transaction, context, record, invocation)
        }
        RecordData::UsageReport(usage) => import_usage(transaction, context, record, usage),
        RecordData::Event(event) => {
            let session_id = import_event(transaction, context, record, event)?;
            if let Some(session_id) = session_id {
                if matches!(&event.data, EventData::Message(_)) {
                    touched_message_events
                        .entry(session_id.clone())
                        .or_default()
                        .insert(record.id.as_str().to_owned());
                }
                if let Some(candidate) = first_user_message_candidate(record, event) {
                    let replace = first_user_message_candidates
                        .get(&session_id)
                        .map(|current| is_earlier_first_user_message(&candidate, current))
                        .unwrap_or(true);
                    if replace {
                        first_user_message_candidates.insert(session_id, candidate);
                    }
                }
            }
            match &event.data {
                EventData::ToolCall(call) => {
                    if call.source_kind == ToolSourceKind::Mcp {
                        import_tool_call(
                            transaction,
                            context,
                            record,
                            call,
                            affected_repeat_invocations,
                        )?;
                    }
                    remember_skill_read_call(transaction, context, record, call)
                }
                EventData::ToolResult(result) => {
                    import_tool_result(
                        transaction,
                        context,
                        record,
                        result,
                        affected_repeat_invocations,
                    )?;
                    complete_skill_read_call(transaction, context, record, result)
                }
                EventData::Retry(retry) => {
                    import_explicit_retry(transaction, context, record, event, retry)
                }
                _ => Ok(()),
            }
        }
        RecordData::Unknown(_) => Ok(()),
        _ => Ok(()),
    }
}

fn ensure_record_session(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
) -> Result<(), String> {
    if matches!(record.data, RecordData::Session(_)) {
        return Ok(());
    }
    let Some(session_id) = record.session.as_ref().map(|id| id.as_str()) else {
        return Ok(());
    };
    let timestamp_ms = record.timestamp.map(|timestamp| timestamp.as_millis());
    transaction
        .execute(
            "
            INSERT INTO agent_sessions (
                id, provider, source_id, source_session_id, title, project_name,
                created_at_ms, updated_at_ms, metadata_present, data_quality
            ) VALUES (?1, ?2, ?3, ?1, '', '', ?4, ?4, 0, 'partial')
            ON CONFLICT(id) DO NOTHING
            ",
            params![
                session_id,
                context.provider,
                context.source_id,
                timestamp_ms
            ],
        )
        .map_err(|error| format!("failed to ensure fallback session metadata: {error}"))?;
    Ok(())
}

fn project_key(session: &Session) -> String {
    let remote = session
        .git_remote_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(normalize_git_remote);
    let cwd = session
        .cwd
        .as_deref()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    let identity = format!(
        "{}\0{}",
        remote.as_deref().unwrap_or_default(),
        cwd.as_deref().unwrap_or_default()
    );
    format!("v1:{:x}", Sha256::digest(identity.as_bytes()))
}

fn normalize_git_remote(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/').trim_end_matches(".git");
    if let Some((scheme, rest)) = trimmed.split_once("://") {
        let authority_and_path = rest.rsplit_once('@').map_or(rest, |(_, safe)| safe);
        return format!("{}://{}", scheme.to_ascii_lowercase(), authority_and_path);
    }
    trimmed.to_owned()
}

fn tool_call_id(context: &ProviderContext, path: &str, call_id: &str) -> String {
    let digest = Sha256::digest(format!("{}\0{}\0{}", context.source_id, path, call_id).as_bytes());
    format!("tool-call:{digest:x}")
}

fn canonical_json(value: &Value) -> String {
    fn canonicalize(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let sorted = map
                    .iter()
                    .map(|(key, value)| (key.clone(), canonicalize(value)))
                    .collect();
                Value::Object(sorted)
            }
            Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
            _ => value.clone(),
        }
    }
    serde_json::to_string(&canonicalize(value)).unwrap_or_else(|_| "null".to_owned())
}

fn input_fingerprint(context: &ProviderContext, call: &ToolCall) -> String {
    let identity = format!(
        "{}\0mcp\0{}\0{}\0{}\0{}",
        context.provider,
        call.namespace.as_deref().unwrap_or_default(),
        call.server_name.as_deref().unwrap_or_default(),
        call.name,
        canonical_json(&call.input)
    );
    format!("sha256:{:x}", Sha256::digest(identity.as_bytes()))
}

fn resolve_tool_owner(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
) -> Result<Option<(String, Option<String>)>, String> {
    let path = record.origin.path.to_string_lossy();
    let session_id = record
        .session
        .as_ref()
        .map(|id| id.as_str().to_owned())
        .or_else(|| {
            transaction
                .query_row(
                    "SELECT session_id FROM rollout_sources WHERE source_id = ?1 AND path = ?2",
                    params![context.source_id, path],
                    |row| row.get(0),
                )
                .optional()
                .ok()
                .flatten()
                .flatten()
        });
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let requested_invocation = record.invocation.as_ref().map(|id| id.as_str().to_owned());
    let invocation_id = match requested_invocation {
        Some(id) if invocation_exists(transaction, &id)? => Some(id),
        _ => current_invocation(transaction, context, &path)?.map(|(id, _, _)| id),
    };
    Ok(Some((session_id, invocation_id)))
}

fn import_tool_call(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    call: &ToolCall,
    affected_repeat_invocations: &mut BTreeSet<String>,
) -> Result<(), String> {
    let Some((session_id, invocation_id)) = resolve_tool_owner(transaction, context, record)?
    else {
        return Ok(());
    };
    let path = record.origin.path.to_string_lossy();
    let id = tool_call_id(context, &path, &call.call_id);
    let started_at_ms = record.timestamp.map(|timestamp| timestamp.as_millis());
    transaction
        .execute(
            "
            INSERT INTO mcp_tool_call (
                id, provider, source_id, session_id, invocation_id, source_path, call_id,
                tool_name, namespace, mcp_server, tool_kind, title,
                started_at_ms, status, call_event_id, input_fingerprint, evidence_quality
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, 'observed'
            )
            ON CONFLICT(source_id, source_path, call_id) DO UPDATE SET
                provider = excluded.provider,
                session_id = excluded.session_id,
                invocation_id = COALESCE(excluded.invocation_id, mcp_tool_call.invocation_id),
                tool_name = excluded.tool_name,
                namespace = excluded.namespace,
                mcp_server = excluded.mcp_server,
                tool_kind = excluded.tool_kind,
                title = excluded.title,
                started_at_ms = COALESCE(mcp_tool_call.started_at_ms, excluded.started_at_ms),
                status = CASE WHEN mcp_tool_call.has_result = 1 THEN mcp_tool_call.status ELSE excluded.status END,
                call_event_id = excluded.call_event_id,
                input_fingerprint = excluded.input_fingerprint,
                evidence_quality = 'observed'
            ",
            params![
                id,
                context.provider,
                context.source_id,
                session_id,
                invocation_id,
                path,
                call.call_id,
                call.name,
                call.namespace,
                call.server_name,
                tool_kind(call.kind),
                call.title,
                started_at_ms,
                tool_status(call.status),
                record.id.as_str(),
                input_fingerprint(context, call),
            ],
        )
        .map_err(|error| format!("failed to import a tool call: {error}"))?;
    reconcile_stored_mcp_result(transaction, context, &path, &call.call_id)?;
    affected_repeat_invocations.extend(invocation_id);
    Ok(())
}

fn reconcile_stored_mcp_result(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &str,
    call_id: &str,
) -> Result<(), String> {
    let stored_result = transaction
        .query_row(
            "
            SELECT id, event_json
            FROM session_events
            WHERE source_id = ?1 AND source_path = ?2 AND event_type = 'tool_result'
              AND json_extract(event_json, '$.data.value.call_id') = ?3
            ORDER BY sequence_position DESC, sequence_part DESC
            LIMIT 1
            ",
            params![context.source_id, path, call_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| format!("failed to find an earlier MCP tool result: {error}"))?;
    let Some((event_id, event_json)) = stored_result else {
        return Ok(());
    };
    let event = serde_json::from_str::<Event>(&event_json)
        .map_err(|error| format!("failed to decode an earlier MCP tool result: {error}"))?;
    let EventData::ToolResult(result) = event.data else {
        return Ok(());
    };
    let completed_at_ms = transaction
        .query_row(
            "SELECT timestamp_ms FROM session_events WHERE id = ?1",
            [&event_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(|error| format!("failed to read an earlier MCP result timestamp: {error}"))?;
    transaction
        .execute(
            "
            UPDATE mcp_tool_call
            SET completed_at_ms = ?2,
                duration_ms = COALESCE(?3,
                    CASE WHEN started_at_ms IS NOT NULL AND ?2 >= started_at_ms
                         THEN ?2 - started_at_ms END),
                duration_source = CASE
                    WHEN ?3 IS NOT NULL THEN 'normalized'
                    WHEN started_at_ms IS NOT NULL AND ?2 >= started_at_ms THEN 'event_delta'
                    ELSE 'unknown'
                END,
                status = ?4,
                has_result = 1,
                has_error = ?5,
                result_event_id = ?6
            WHERE source_id = ?1 AND source_path = ?7 AND call_id = ?8
            ",
            params![
                context.source_id,
                completed_at_ms,
                result.duration_ms,
                tool_status(result.status),
                i64::from(result.status == ToolStatus::Failed || result.error.is_some()),
                event_id,
                path,
                call_id,
            ],
        )
        .map_err(|error| format!("failed to reconcile an earlier MCP tool result: {error}"))?;
    Ok(())
}

fn import_tool_result(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    result: &ToolResult,
    affected_repeat_invocations: &mut BTreeSet<String>,
) -> Result<(), String> {
    let Some((session_id, invocation_id)) = resolve_tool_owner(transaction, context, record)?
    else {
        return Ok(());
    };
    let path = record.origin.path.to_string_lossy();
    let completed_at_ms = record.timestamp.map(|timestamp| timestamp.as_millis());
    transaction
        .execute(
            "
            UPDATE mcp_tool_call
            SET session_id = ?1,
                invocation_id = COALESCE(?2, invocation_id),
                tool_name = CASE WHEN tool_name = '' THEN ?3 ELSE tool_name END,
                completed_at_ms = ?4,
                duration_ms = COALESCE(?5,
                    CASE
                        WHEN started_at_ms IS NOT NULL AND ?4 >= started_at_ms
                        THEN ?4 - started_at_ms
                    END),
                duration_source = CASE
                    WHEN ?5 IS NOT NULL THEN 'normalized'
                    WHEN started_at_ms IS NOT NULL AND ?4 >= started_at_ms THEN 'event_delta'
                    ELSE 'unknown'
                END,
                status = ?6,
                has_result = 1,
                has_error = ?7,
                result_event_id = ?8
            WHERE source_id = ?9 AND source_path = ?10 AND call_id = ?11
            ",
            params![
                session_id,
                invocation_id,
                result.name.as_deref().unwrap_or_default(),
                completed_at_ms,
                result.duration_ms,
                tool_status(result.status),
                i64::from(result.status == ToolStatus::Failed || result.error.is_some()),
                record.id.as_str(),
                context.source_id,
                path,
                result.call_id,
            ],
        )
        .map_err(|error| format!("failed to import an MCP tool result: {error}"))?;
    if transaction.changes() > 0 {
        affected_repeat_invocations.extend(invocation_id);
    }
    Ok(())
}

fn import_explicit_retry(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    event: &Event,
    retry: &Retry,
) -> Result<(), String> {
    let Some((session_id, invocation_id)) = resolve_tool_owner(transaction, context, record)?
    else {
        return Ok(());
    };
    let path = record.origin.path.to_string_lossy();
    let tool_call_id = event.parent.as_ref().and_then(|parent| {
        transaction
            .query_row(
                "SELECT id FROM mcp_tool_call WHERE source_id = ?1 AND source_path = ?2 AND call_event_id = ?3",
                params![context.source_id, path, parent.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
    });
    let Some(tool_call_id) = tool_call_id else {
        return Ok(());
    };
    transaction
        .execute(
            "
            INSERT INTO mcp_tool_call_retry (
                id, provider, source_id, session_id, invocation_id, source_path,
                timestamp_ms, mcp_tool_call_id, attempt, delay_ms, evidence_quality
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'observed')
            ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                invocation_id = excluded.invocation_id,
                timestamp_ms = excluded.timestamp_ms,
                mcp_tool_call_id = excluded.mcp_tool_call_id,
                attempt = excluded.attempt,
                delay_ms = excluded.delay_ms
            ",
            params![
                record.id.as_str(),
                context.provider,
                context.source_id,
                session_id,
                invocation_id,
                path,
                record.timestamp.map(|timestamp| timestamp.as_millis()),
                tool_call_id,
                retry.attempt.map(i64::from),
                retry.delay_ms.and_then(|value| i64::try_from(value).ok()),
            ],
        )
        .map_err(|error| format!("failed to import explicit retry evidence: {error}"))?;
    Ok(())
}

fn refresh_repeat_chain(
    transaction: &Transaction<'_>,
    invocation_id: Option<&str>,
) -> Result<(), String> {
    let Some(invocation_id) = invocation_id else {
        return Ok(());
    };
    let mut statement = transaction
        .prepare(
            "
            SELECT id, input_fingerprint, status
            FROM mcp_tool_call
            WHERE invocation_id = ?1 AND input_fingerprint IS NOT NULL
            ORDER BY COALESCE(started_at_ms, completed_at_ms, 9223372036854775807), rowid
            ",
        )
        .map_err(|error| format!("failed to prepare repeat-chain refresh: {error}"))?;
    let rows = statement
        .query_map([invocation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("failed to query repeat-chain members: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read repeat-chain members: {error}"))?;
    drop(statement);

    let mut previous = std::collections::HashMap::<String, (String, String, i64)>::new();
    for (id, fingerprint, status) in rows {
        let group_id = format!(
            "repeat:{:x}",
            Sha256::digest(format!("{invocation_id}\0{fingerprint}").as_bytes())
        );
        let entry = previous.get(&fingerprint).cloned();
        let (repeat_of, repeat_index, inferred_retry, retry_of) = match entry {
            Some((previous_id, previous_status, index)) => (
                Some(previous_id.clone()),
                index + 1,
                previous_status == "failed",
                (previous_status == "failed").then_some(previous_id),
            ),
            None => (None, 0, false, None),
        };
        transaction
            .execute(
                "
                UPDATE mcp_tool_call
                SET repeat_group_id = ?2,
                    repeat_of_id = ?3,
                    repeat_index = ?4,
                    retry_class = CASE WHEN ?5 = 1 THEN 'inferred' ELSE 'none' END,
                    retry_of_id = ?6
                WHERE id = ?1
                ",
                params![
                    id,
                    group_id,
                    repeat_of,
                    repeat_index,
                    i64::from(inferred_retry),
                    retry_of
                ],
            )
            .map_err(|error| format!("failed to update a repeat-chain member: {error}"))?;
        previous.insert(fingerprint, (id, status, repeat_index));
    }
    Ok(())
}

fn tool_kind(value: ToolKind) -> &'static str {
    match value {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Delete => "delete",
        ToolKind::Move => "move",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::Think => "think",
        ToolKind::Fetch => "fetch",
        ToolKind::SwitchMode => "switch_mode",
        _ => "other",
    }
}

fn tool_status(value: ToolStatus) -> &'static str {
    match value {
        ToolStatus::Pending => "pending",
        ToolStatus::AwaitingApproval => "awaiting_approval",
        ToolStatus::InProgress => "in_progress",
        ToolStatus::Completed => "completed",
        ToolStatus::Failed => "failed",
        ToolStatus::Cancelled => "cancelled",
        ToolStatus::Declined => "declined",
        _ => "unknown",
    }
}

fn import_session(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
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
    let data_quality = match session.quality {
        DataQuality::Complete => "complete",
        DataQuality::Partial => "partial",
        _ => "partial",
    };

    if let Some(path) = rollout_path.as_deref() {
        transaction
            .execute(
                "
                UPDATE agent_sessions
                SET rollout_path = NULL
                WHERE source_id = ?1
                  AND rollout_path = ?2
                  AND id <> ?3
                  AND metadata_present = 0
                ",
                params![context.source_id, path, id],
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
                metadata_present,
                model,
                model_provider,
                agent_version,
                agent_name,
                agent_role,
                git_branch,
                git_commit,
                git_remote_url,
                data_quality,
                source_id,
                project_key
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
            )
            ON CONFLICT(id) DO UPDATE SET
                source_session_id = excluded.source_session_id,
                title = excluded.title,
                project_name = excluded.project_name,
                cwd = excluded.cwd,
                rollout_path = excluded.rollout_path,
                created_at_ms = excluded.created_at_ms,
                updated_at_ms = excluded.updated_at_ms,
                tokens_used = excluded.tokens_used,
                archived = excluded.archived,
                metadata_present = excluded.metadata_present,
                model = excluded.model,
                model_provider = excluded.model_provider,
                agent_version = excluded.agent_version,
                agent_name = excluded.agent_name,
                agent_role = excluded.agent_role,
                git_branch = excluded.git_branch,
                git_commit = excluded.git_commit,
                git_remote_url = excluded.git_remote_url,
                data_quality = excluded.data_quality,
                provider = excluded.provider,
                source_id = excluded.source_id,
                project_key = excluded.project_key
            ",
            params![
                id,
                context.provider,
                source_session_id,
                title,
                project_name,
                cwd,
                rollout_path,
                created_at_ms,
                updated_at_ms,
                session.total_tokens,
                session.archived as i64,
                metadata_present,
                session.model.as_deref(),
                session.model_provider.as_deref(),
                session.agent_version.as_deref(),
                session.agent_name.as_deref(),
                session.agent_role.as_deref(),
                session.git_branch.as_deref(),
                session.git_commit.as_deref(),
                session.git_remote_url.as_deref(),
                data_quality,
                context.source_id,
                project_key(session)
            ],
        )
        .map_err(|error| format!("failed to import session metadata: {error}"))?;

    if let Some(path) = rollout_path.as_deref() {
        upsert_rollout_source(
            transaction,
            context,
            path,
            Some(id),
            session.quality == DataQuality::Complete,
        )?;
    }
    Ok(())
}

fn import_event(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    event: &Event,
) -> Result<Option<String>, String> {
    let path = record.origin.path.to_string_lossy();
    let session_id = if let Some(session_id) = &record.session {
        Some(session_id.as_str().to_owned())
    } else {
        transaction
            .query_row(
                "SELECT session_id FROM rollout_sources WHERE source_id = ?1 AND path = ?2",
                params![context.source_id, path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("failed to find the session event owner: {error}"))?
            .flatten()
    };
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let event_json = serde_json::to_string(event)
        .map_err(|error| format!("failed to serialize a normalized session event: {error}"))?;
    let position = i64::try_from(event.sequence.position).unwrap_or(i64::MAX);
    let part = i64::from(event.sequence.part);
    let logical_ordinal = event
        .sequence
        .logical_ordinal
        .map(|value| i64::try_from(value).unwrap_or(i64::MAX));

    transaction
        .execute(
            "
            INSERT INTO session_events (
                id,
                provider,
                source_id,
                session_id,
                invocation_id,
                source_path,
                timestamp_ms,
                sequence_position,
                sequence_part,
                logical_ordinal,
                event_type,
                event_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                source_id = excluded.source_id,
                session_id = excluded.session_id,
                invocation_id = excluded.invocation_id,
                source_path = excluded.source_path,
                timestamp_ms = excluded.timestamp_ms,
                sequence_position = excluded.sequence_position,
                sequence_part = excluded.sequence_part,
                logical_ordinal = excluded.logical_ordinal,
                event_type = excluded.event_type,
                event_json = excluded.event_json
            ",
            params![
                record.id.as_str(),
                context.provider,
                context.source_id,
                session_id,
                record.invocation.as_ref().map(|id| id.as_str()),
                path,
                record.timestamp.map(|timestamp| timestamp.as_millis()),
                position,
                part,
                logical_ordinal,
                event_type(&event.data),
                event_json
            ],
        )
        .map_err(|error| format!("failed to import a normalized session event: {error}"))?;
    Ok(Some(session_id))
}

const MAX_FIRST_USER_MESSAGE_SESSION_IDS_PER_QUERY: usize = 500;

fn first_user_message_candidate(
    record: &Record,
    event: &Event,
) -> Option<FirstUserMessageCandidate> {
    let EventData::Message(message) = &event.data else {
        return None;
    };
    if !matches!(message.role, MessageRole::User) && !matches!(event.actor, Actor::User) {
        return None;
    }
    let text = message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            ContentBlock::Resource {
                text: Some(text), ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned();
    Some(FirstUserMessageCandidate {
        event_id: record.id.as_str().to_owned(),
        text,
        timestamp_ms: record.timestamp.map(|timestamp| timestamp.as_millis()),
        sequence_position: i64::try_from(event.sequence.position).unwrap_or(i64::MAX),
        sequence_part: i64::from(event.sequence.part),
    })
}

fn first_user_message_order(
    timestamp_ms: Option<i64>,
    sequence_position: i64,
    sequence_part: i64,
    event_id: &str,
) -> (i64, i64, i64, &str) {
    (
        timestamp_ms.unwrap_or(i64::MAX),
        sequence_position,
        sequence_part,
        event_id,
    )
}

fn is_earlier_first_user_message(
    candidate: &FirstUserMessageCandidate,
    current: &FirstUserMessageCandidate,
) -> bool {
    first_user_message_order(
        candidate.timestamp_ms,
        candidate.sequence_position,
        candidate.sequence_part,
        &candidate.event_id,
    ) < first_user_message_order(
        current.timestamp_ms,
        current.sequence_position,
        current.sequence_part,
        &current.event_id,
    )
}

fn mark_deleted_first_user_message(
    transaction: &Transaction<'_>,
    event_id: &str,
    sessions: &mut BTreeSet<String>,
) -> Result<(), String> {
    let session_id = transaction
        .query_row(
            "SELECT id FROM agent_sessions WHERE first_user_message_event_id = ?1",
            [event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("failed to inspect a deleted first user message: {error}"))?;
    if let Some(session_id) = session_id {
        sessions.insert(session_id);
    }
    Ok(())
}

fn mark_source_first_user_messages(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &Path,
    sessions: &mut BTreeSet<String>,
) -> Result<(), String> {
    let path = path.to_string_lossy();
    let mut statement = transaction
        .prepare(
            "
            SELECT sessions.id
            FROM agent_sessions sessions
            JOIN session_events events
              ON events.id = sessions.first_user_message_event_id
            WHERE events.source_id = ?1 AND events.source_path = ?2
            ",
        )
        .map_err(|error| format!("failed to prepare source first user message lookup: {error}"))?;
    let rows = statement
        .query_map(params![context.source_id, path.as_ref()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| format!("failed to query source first user messages: {error}"))?;
    for row in rows {
        sessions.insert(
            row.map_err(|error| format!("failed to read source first user message: {error}"))?,
        );
    }
    Ok(())
}

fn refresh_first_user_message_projection(
    transaction: &Transaction<'_>,
    candidates: &BTreeMap<String, FirstUserMessageCandidate>,
    touched_message_events: &BTreeMap<String, BTreeSet<String>>,
    recompute_sessions: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut session_ids = candidates.keys().cloned().collect::<BTreeSet<_>>();
    session_ids.extend(touched_message_events.keys().cloned());
    session_ids.extend(recompute_sessions.iter().cloned());
    if session_ids.is_empty() {
        return Ok(());
    }

    let stored = load_stored_first_user_messages(transaction, &session_ids)?;
    for (session_id, touched_events) in touched_message_events {
        let Some(current_event_id) = stored
            .get(session_id)
            .and_then(|message| message.event_id.as_deref())
        else {
            continue;
        };
        if touched_events.contains(current_event_id) {
            recompute_sessions.insert(session_id.clone());
        }
    }

    for (session_id, candidate) in candidates {
        if recompute_sessions.contains(session_id) {
            continue;
        }
        let should_update = match stored.get(session_id) {
            None => true,
            Some(current) if current.event_id.is_none() => true,
            Some(current) if current.sequence_position.is_none() => true,
            Some(current) => {
                first_user_message_order(
                    candidate.timestamp_ms,
                    candidate.sequence_position,
                    candidate.sequence_part,
                    &candidate.event_id,
                ) < first_user_message_order(
                    current.timestamp_ms,
                    current.sequence_position.unwrap_or(i64::MAX),
                    current.sequence_part.unwrap_or(i64::MAX),
                    current.event_id.as_deref().unwrap_or_default(),
                )
            }
        };
        if should_update {
            update_first_user_message(transaction, session_id, candidate)?;
        }
    }

    recompute_first_user_messages(transaction, recompute_sessions)
}

fn load_stored_first_user_messages(
    transaction: &Transaction<'_>,
    session_ids: &BTreeSet<String>,
) -> Result<BTreeMap<String, StoredFirstUserMessage>, String> {
    let mut stored = BTreeMap::new();
    let session_ids = session_ids.iter().collect::<Vec<_>>();
    for session_chunk in session_ids.chunks(MAX_FIRST_USER_MESSAGE_SESSION_IDS_PER_QUERY) {
        let placeholders = (0..session_chunk.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            "
            SELECT sessions.id,
                   sessions.first_user_message_event_id,
                   sessions.first_user_message_timestamp_ms,
                   events.sequence_position,
                   events.sequence_part
            FROM agent_sessions sessions
            LEFT JOIN session_events events
              ON events.id = sessions.first_user_message_event_id
            WHERE sessions.id IN ({placeholders})
            "
        );
        let mut statement = transaction.prepare(&query).map_err(|error| {
            format!("failed to prepare first user message state query: {error}")
        })?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(session_chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    StoredFirstUserMessage {
                        event_id: row.get(1)?,
                        timestamp_ms: row.get(2)?,
                        sequence_position: row.get(3)?,
                        sequence_part: row.get(4)?,
                    },
                ))
            })
            .map_err(|error| format!("failed to query first user message state: {error}"))?;
        for row in rows {
            let (session_id, message) =
                row.map_err(|error| format!("failed to read first user message state: {error}"))?;
            stored.insert(session_id, message);
        }
    }
    Ok(stored)
}

fn update_first_user_message(
    transaction: &Transaction<'_>,
    session_id: &str,
    candidate: &FirstUserMessageCandidate,
) -> Result<(), String> {
    transaction
        .execute(
            "
            UPDATE agent_sessions
            SET first_user_message_text = ?2,
                first_user_message_event_id = ?3,
                first_user_message_timestamp_ms = ?4
            WHERE id = ?1
            ",
            params![
                session_id,
                candidate.text,
                candidate.event_id,
                candidate.timestamp_ms,
            ],
        )
        .map_err(|error| format!("failed to update the first user message: {error}"))?;
    Ok(())
}

fn recompute_first_user_messages(
    transaction: &Transaction<'_>,
    session_ids: &BTreeSet<String>,
) -> Result<(), String> {
    if session_ids.is_empty() {
        return Ok(());
    }
    let session_ids = session_ids.iter().collect::<Vec<_>>();
    for session_chunk in session_ids.chunks(MAX_FIRST_USER_MESSAGE_SESSION_IDS_PER_QUERY) {
        let placeholders = (0..session_chunk.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let clear_query = format!(
            "
            UPDATE agent_sessions
            SET first_user_message_text = NULL,
                first_user_message_event_id = NULL,
                first_user_message_timestamp_ms = NULL
            WHERE id IN ({placeholders})
            "
        );
        transaction
            .execute(
                &clear_query,
                rusqlite::params_from_iter(session_chunk.iter()),
            )
            .map_err(|error| format!("failed to clear first user message projections: {error}"))?;

        let query = format!(
            "
            SELECT id, session_id, timestamp_ms, sequence_position, sequence_part, event_json
            FROM session_events
            WHERE event_type = 'message'
              AND session_id IN ({placeholders})
              AND (
                  json_extract(event_json, '$.data.value.role') = 'user'
                  OR json_extract(event_json, '$.actor') = 'user'
              )
            ORDER BY
                session_id,
                timestamp_ms IS NULL,
                timestamp_ms,
                sequence_position,
                sequence_part,
                id
            "
        );
        let mut statement = transaction
            .prepare(&query)
            .map_err(|error| format!("failed to prepare first user message rebuild: {error}"))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(session_chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|error| format!("failed to query first user message rebuild: {error}"))?;
        let mut seen = BTreeSet::new();
        for row in rows {
            let (event_id, session_id, timestamp_ms, sequence_position, sequence_part, event_json) =
                row.map_err(|error| format!("failed to read first user message rebuild: {error}"))?;
            if !seen.insert(session_id.clone()) {
                continue;
            }
            let Some(text) = user_message_text_from_json(&event_json) else {
                continue;
            };
            update_first_user_message(
                transaction,
                &session_id,
                &FirstUserMessageCandidate {
                    event_id,
                    text,
                    timestamp_ms,
                    sequence_position,
                    sequence_part,
                },
            )?;
        }
    }
    Ok(())
}

fn user_message_text_from_json(event_json: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(event_json).ok()?;
    let is_user = value.pointer("/data/value/role").and_then(Value::as_str) == Some("user")
        || value.get("actor").and_then(Value::as_str) == Some("user");
    if !is_user {
        return None;
    }
    let mut values = Vec::new();
    if let Some(content) = value.pointer("/data/value/content") {
        collect_user_message_text(content, &mut values);
    }
    Some(values.join(" ").trim().to_owned())
}

fn collect_user_message_text(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_user_message_text(item, values)),
        Value::Object(object) => {
            if matches!(
                object.get("type").and_then(Value::as_str),
                Some("text" | "resource")
            ) {
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    values.push(text.to_owned());
                }
            }
        }
        _ => {}
    }
}

fn event_type(data: &EventData) -> &'static str {
    match data {
        EventData::Message(_) => "message",
        EventData::Reasoning(_) => "reasoning",
        EventData::Plan(_) => "plan",
        EventData::ToolCall(_) => "tool_call",
        EventData::ToolResult(_) => "tool_result",
        EventData::ApprovalRequest(_) => "approval_request",
        EventData::ApprovalDecision(_) => "approval_decision",
        EventData::ModelInvocation(_) => "model_invocation",
        EventData::AgentInvocation(_) => "agent_invocation",
        EventData::FileChange(_) => "file_change",
        EventData::WorldState(_) => "world_state",
        EventData::Goal(_) => "goal",
        EventData::ForkInvocationBoundary(_) => "fork_invocation_boundary",
        EventData::InputQueue(_) => "input_queue",
        EventData::ContextCompaction(_) => "context_compaction",
        EventData::ExecutionContext(_) => "execution_context",
        EventData::ModeChange(_) => "mode_change",
        EventData::Notice(_) => "notice",
        EventData::HookResult(_) => "hook_result",
        EventData::Retry(_) => "retry",
        EventData::Rollback(_) => "rollback",
        EventData::Unknown(_) => "unknown",
        _ => "unknown",
    }
}

fn import_agent_invocation(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    invocation: &AgentInvocation,
) -> Result<(), String> {
    match invocation.status {
        AgentInvocationStatus::InProgress => {
            import_running_invocation(transaction, context, record, invocation)
        }
        AgentInvocationStatus::Completed
        | AgentInvocationStatus::Failed
        | AgentInvocationStatus::Cancelled
        | AgentInvocationStatus::Interrupted => {
            import_terminal_invocation(transaction, context, record, invocation)
        }
        _ => Ok(()),
    }
}

fn import_running_invocation(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    invocation: &AgentInvocation,
) -> Result<(), String> {
    let Some(session_id) = record.session.as_ref().map(|id| id.as_str()) else {
        return Ok(());
    };
    let path = record.origin.path.to_string_lossy();
    let invocation_id = record.id.as_str();
    let started_at_ms = invocation.started_at.map(|timestamp| timestamp.as_millis());
    upsert_rollout_source(transaction, context, &path, Some(session_id), false)?;

    transaction
        .execute(
            "
            UPDATE agent_invocations
            SET status = 'unknown'
            WHERE source_id = ?1
              AND source_path = ?2
              AND status = 'in_progress'
              AND id <> ?3
            ",
            params![context.source_id, path, invocation_id],
        )
        .map_err(|error| format!("failed to close the previous incomplete invocation: {error}"))?;
    transaction
        .execute(
            "
            UPDATE skill_invocations
            SET status = 'unknown'
            WHERE invocation_id IN (
                SELECT id
                FROM agent_invocations
                WHERE source_id = ?1 AND source_path = ?2 AND status = 'unknown'
            ) AND status = 'in_progress'
            ",
            params![context.source_id, path],
        )
        .map_err(|error| format!("failed to close incomplete skill invocations: {error}"))?;
    transaction
        .execute(
            "
            INSERT INTO agent_invocations (
                id,
                source_id,
                session_id,
                source_path,
                started_at_ms,
                status
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'in_progress')
            ON CONFLICT(id) DO UPDATE SET
                source_id = excluded.source_id,
                session_id = excluded.session_id,
                source_path = excluded.source_path,
                started_at_ms = COALESCE(agent_invocations.started_at_ms, excluded.started_at_ms)
            ",
            params![
                invocation_id,
                context.source_id,
                session_id,
                path,
                started_at_ms
            ],
        )
        .map_err(|error| format!("failed to import a task start: {error}"))?;
    transaction
        .execute(
            "
            UPDATE rollout_sources
            SET current_invocation_id = ?3, updated_at_ms = ?4
            WHERE source_id = ?1 AND path = ?2
            ",
            params![context.source_id, path, invocation_id, repository::now_ms()],
        )
        .map_err(|error| format!("failed to track the current invocation: {error}"))?;
    Ok(())
}

fn remember_skill_read_call(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    call: &ToolCall,
) -> Result<(), String> {
    if !value_contains_text(&call.input, "SKILL.md") {
        return Ok(());
    }
    let path = record.origin.path.to_string_lossy();
    let invocation_id = if let Some(invocation_id) = &record.invocation {
        invocation_id.as_str().to_owned()
    } else {
        let Some((invocation_id, _, _)) = current_invocation(transaction, context, &path)? else {
            return Ok(());
        };
        invocation_id
    };
    if !invocation_exists(transaction, &invocation_id)? {
        return Ok(());
    }
    transaction
        .execute(
            "
            INSERT INTO pending_skill_reads (source_id, source_path, call_id, invocation_id)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(source_id, source_path, call_id) DO UPDATE SET
                invocation_id = excluded.invocation_id
            ",
            params![context.source_id, path, call.call_id, invocation_id],
        )
        .map_err(|error| format!("failed to remember a possible Skill file read: {error}"))?;
    Ok(())
}

fn complete_skill_read_call(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    result: &ToolResult,
) -> Result<(), String> {
    let path = record.origin.path.to_string_lossy();
    let invocation_id: Option<String> = transaction
        .query_row(
            "
            SELECT invocation_id
            FROM pending_skill_reads
            WHERE source_id = ?1 AND source_path = ?2 AND call_id = ?3
            ",
            params![context.source_id, path, result.call_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("failed to find a pending Skill file read: {error}"))?;
    let Some(invocation_id) = invocation_id else {
        return Ok(());
    };
    transaction
        .execute(
            "DELETE FROM pending_skill_reads WHERE source_id = ?1 AND source_path = ?2 AND call_id = ?3",
            params![context.source_id, path, result.call_id],
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
            "SELECT session_id, started_at_ms FROM agent_invocations WHERE id = ?1",
            [&invocation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()
        .map_err(|error| format!("failed to resolve the Skill invocation invocation: {error}"))?
    else {
        return Ok(());
    };
    for skill_name in skill_names {
        let skill_invocation_id = format!("{invocation_id}:{skill_name}");
        transaction
            .execute(
                "
                INSERT INTO skill_invocations (
                    id,
                    invocation_id,
                    session_id,
                    skill_name,
                    started_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(id) DO NOTHING
                ",
                params![
                    skill_invocation_id,
                    invocation_id,
                    session_id,
                    skill_name,
                    started_at_ms
                ],
            )
            .map_err(|error| format!("failed to import a skill invocation: {error}"))?;
    }
    update_skill_metrics(transaction, &invocation_id)?;
    Ok(())
}

fn import_usage(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    usage: &UsageReport,
) -> Result<(), String> {
    let path = record.origin.path.to_string_lossy();
    let session_id = if let Some(session_id) = &record.session {
        Some(session_id.as_str().to_owned())
    } else {
        transaction
            .query_row(
                "SELECT session_id FROM rollout_sources WHERE source_id = ?1 AND path = ?2",
                params![context.source_id, path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("failed to find the token usage session: {error}"))?
            .flatten()
    };
    let Some(session_id) = session_id else {
        return Ok(());
    };
    let invocation_id = if let Some(invocation_id) = &record.invocation {
        Some(invocation_id.as_str().to_owned())
    } else {
        transaction
            .query_row(
                "SELECT current_invocation_id FROM rollout_sources WHERE source_id = ?1 AND path = ?2",
                params![context.source_id, path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("failed to find the token usage invocation: {error}"))?
            .flatten()
    };
    let invocation_id = match invocation_id {
        Some(invocation_id) if invocation_exists(transaction, &invocation_id)? => {
            Some(invocation_id)
        }
        _ => None,
    };
    let previous_invocation_id = transaction
        .query_row(
            "SELECT invocation_id FROM token_usage_records WHERE id = ?1",
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
                source_id,
                session_id,
                invocation_id,
                source_path,
                timestamp_ms,
                cumulative_tokens,
                delta_tokens
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                source_id = excluded.source_id,
                session_id = excluded.session_id,
                invocation_id = excluded.invocation_id,
                source_path = excluded.source_path,
                timestamp_ms = excluded.timestamp_ms,
                cumulative_tokens = excluded.cumulative_tokens,
                delta_tokens = excluded.delta_tokens
            ",
            params![
                record.id.as_str(),
                context.source_id,
                session_id,
                invocation_id,
                path,
                record.timestamp.map(|timestamp| timestamp.as_millis()),
                usage.cumulative.as_ref().map(|usage| usage.total),
                usage.delta.as_ref().map(|usage| usage.total)
            ],
        )
        .map_err(|error| format!("failed to import token usage: {error}"))?;

    if let Some(previous_invocation_id) = previous_invocation_id.as_deref() {
        if Some(previous_invocation_id) != invocation_id.as_deref() {
            refresh_invocation_tokens(transaction, previous_invocation_id)?;
        }
    }
    if let Some(invocation_id) = invocation_id.as_deref() {
        refresh_invocation_tokens(transaction, invocation_id)?;
    }
    Ok(())
}

fn import_terminal_invocation(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    record: &Record,
    invocation: &AgentInvocation,
) -> Result<(), String> {
    let path = record.origin.path.to_string_lossy();
    let invocation_id = record.id.as_str();
    let started_at_ms = invocation.started_at.map(|timestamp| timestamp.as_millis());
    let completed_at_ms = invocation
        .completed_at
        .map(|timestamp| timestamp.as_millis());
    let duration_ms = invocation
        .duration_ms
        .or_else(|| match (started_at_ms, completed_at_ms) {
            (Some(started), Some(completed)) => Some(completed.saturating_sub(started).max(0)),
            _ => None,
        });
    let status = match invocation.status {
        AgentInvocationStatus::Completed => "succeeded",
        AgentInvocationStatus::Failed => "failed",
        AgentInvocationStatus::Cancelled => "cancelled",
        // Analytics exposes success, failure, cancellation, and unknown. A
        // provider interruption is a terminal cancellation in that contract.
        AgentInvocationStatus::Interrupted => "cancelled",
        AgentInvocationStatus::InProgress => "in_progress",
        _ => "unknown",
    };

    if let Some(session_id) = record.session.as_ref().map(|id| id.as_str()) {
        upsert_rollout_source(transaction, context, &path, Some(session_id), false)?;
        transaction
            .execute(
                "
            INSERT INTO agent_invocations (
                id,
                source_id,
                session_id,
                source_path,
                started_at_ms,
                status
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'in_progress')
            ON CONFLICT(id) DO NOTHING
            ",
                params![
                    invocation_id,
                    context.source_id,
                    session_id,
                    path,
                    started_at_ms
                ],
            )
            .map_err(|error| {
                format!("failed to recover a invocation without a start event: {error}")
            })?;
    }
    let updated = transaction
        .execute(
            "
            UPDATE agent_invocations
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
                invocation_id,
                started_at_ms,
                completed_at_ms,
                duration_ms,
                status,
                invocation.error
            ],
        )
        .map_err(|error| format!("failed to finish a invocation: {error}"))?;
    if updated == 0 {
        return Ok(());
    }
    transaction
        .execute(
            "
            UPDATE rollout_sources
            SET current_invocation_id = NULL, updated_at_ms = ?4
            WHERE source_id = ?1 AND path = ?2 AND current_invocation_id = ?3
            ",
            params![context.source_id, path, invocation_id, repository::now_ms()],
        )
        .map_err(|error| format!("failed to clear the completed invocation: {error}"))?;
    update_skill_metrics(transaction, invocation_id)
}

fn update_skill_metrics(transaction: &Transaction<'_>, invocation_id: &str) -> Result<(), String> {
    let (status, started_at_ms, duration_ms, total_tokens): (
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = transaction
        .query_row(
            "
            SELECT status, started_at_ms, duration_ms, total_tokens
            FROM agent_invocations
            WHERE id = ?1
            ",
            [invocation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|error| format!("failed to read invocation metrics: {error}"))?;
    transaction
        .execute(
            "
            UPDATE skill_invocations
            SET
                started_at_ms = COALESCE(started_at_ms, ?2),
                duration_ms = ?3,
                total_tokens = ?4,
                status = ?5
            WHERE invocation_id = ?1
            ",
            params![
                invocation_id,
                started_at_ms,
                duration_ms,
                total_tokens,
                status
            ],
        )
        .map_err(|error| format!("failed to update skill metrics: {error}"))?;
    Ok(())
}

fn invocation_exists(transaction: &Transaction<'_>, invocation_id: &str) -> Result<bool, String> {
    transaction
        .query_row(
            "SELECT 1 FROM agent_invocations WHERE id = ?1",
            [invocation_id],
            |_| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(|error| format!("failed to resolve the token usage invocation: {error}"))
}

fn refresh_invocation_tokens(
    transaction: &Transaction<'_>,
    invocation_id: &str,
) -> Result<(), String> {
    let updated = transaction
        .execute(
            "
            UPDATE agent_invocations
            SET total_tokens = (
                SELECT SUM(delta_tokens)
                FROM token_usage_records
                WHERE invocation_id = ?1
            )
            WHERE id = ?1
            ",
            [invocation_id],
        )
        .map_err(|error| format!("failed to update invocation token usage: {error}"))?;
    if updated > 0 {
        update_skill_metrics(transaction, invocation_id)?;
    }
    Ok(())
}

fn remove_rollout_source(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &Path,
) -> Result<(), String> {
    let path = path.to_string_lossy();
    clear_rollout_records(transaction, context, &path)?;
    transaction
        .execute(
            "DELETE FROM rollout_sources WHERE path = ?1 AND source_id = ?2",
            params![path, context.source_id],
        )
        .map_err(|error| format!("failed to clear rollout state: {error}"))?;
    Ok(())
}

fn reset_rollout_source(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &Path,
) -> Result<(), String> {
    let path = path.to_string_lossy();
    clear_rollout_records(transaction, context, &path)?;
    transaction
        .execute(
            "
            UPDATE rollout_sources
            SET current_invocation_id = NULL, updated_at_ms = ?2
            WHERE path = ?1 AND source_id = ?3
            ",
            params![path, repository::now_ms(), context.source_id],
        )
        .map_err(|error| format!("failed to reset rollout state: {error}"))?;
    Ok(())
}

fn clear_rollout_records(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "DELETE FROM mcp_tool_call_retry WHERE source_id = ?1 AND source_path = ?2",
            params![context.source_id, path],
        )
        .map_err(|error| format!("failed to clear rollout retry evidence: {error}"))?;
    transaction
        .execute(
            "DELETE FROM mcp_tool_call WHERE source_id = ?1 AND source_path = ?2",
            params![context.source_id, path],
        )
        .map_err(|error| format!("failed to clear rollout tool calls: {error}"))?;
    transaction
        .execute(
            "DELETE FROM session_events WHERE source_id = ?1 AND source_path = ?2",
            params![context.source_id, path],
        )
        .map_err(|error| format!("failed to clear rollout session events: {error}"))?;
    transaction
        .execute(
            "DELETE FROM token_usage_records WHERE source_id = ?1 AND source_path = ?2",
            params![context.source_id, path],
        )
        .map_err(|error| format!("failed to clear rollout token usage: {error}"))?;
    transaction
        .execute(
            "DELETE FROM agent_invocations WHERE source_id = ?1 AND source_path = ?2",
            params![context.source_id, path],
        )
        .map_err(|error| format!("failed to clear rollout invocations: {error}"))?;
    Ok(())
}

fn upsert_rollout_source(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &str,
    session_id: Option<&str>,
    authoritative: bool,
) -> Result<(), String> {
    let previous_session_id = if authoritative {
        transaction
            .query_row(
                "SELECT session_id FROM rollout_sources WHERE source_id = ?1 AND path = ?2",
                params![context.source_id, path],
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
            INSERT INTO rollout_sources (path, provider, session_id, updated_at_ms, source_id)
            VALUES (?1, ?2, ?3, ?4, ?6)
            ON CONFLICT(source_id, path) DO UPDATE SET
                provider = excluded.provider,
                session_id = CASE
                    WHEN ?5 = 1 THEN excluded.session_id
                    ELSE COALESCE(rollout_sources.session_id, excluded.session_id)
                END,
                updated_at_ms = excluded.updated_at_ms
            ",
            params![
                path,
                context.provider,
                session_id,
                repository::now_ms(),
                authoritative as i64,
                context.source_id
            ],
        )
        .map_err(|error| format!("failed to track a rollout source: {error}"))?;

    if authoritative {
        if let Some(session_id) = session_id {
            transaction
                .execute(
                    "UPDATE session_events SET session_id = ?3 WHERE source_id = ?1 AND source_path = ?2",
                    params![context.source_id, path, session_id],
                )
                .map_err(|error| {
                    format!("failed to update rollout session event ownership: {error}")
                })?;
            transaction
                .execute(
                    "UPDATE agent_invocations SET session_id = ?3 WHERE source_id = ?1 AND source_path = ?2",
                    params![context.source_id, path, session_id],
                )
                .map_err(|error| {
                    format!("failed to update rollout invocation ownership: {error}")
                })?;
            transaction
                .execute(
                    "UPDATE token_usage_records SET session_id = ?3 WHERE source_id = ?1 AND source_path = ?2",
                    params![context.source_id, path, session_id],
                )
                .map_err(|error| format!("failed to update rollout usage ownership: {error}"))?;
            transaction
                .execute(
                    "UPDATE mcp_tool_call SET session_id = ?3 WHERE source_id = ?1 AND source_path = ?2",
                    params![context.source_id, path, session_id],
                )
                .map_err(|error| {
                    format!("failed to update rollout tool-call ownership: {error}")
                })?;
            transaction
                .execute(
                    "
                    UPDATE skill_invocations
                    SET session_id = ?3
                    WHERE invocation_id IN (
                        SELECT id FROM agent_invocations
                        WHERE source_id = ?1 AND source_path = ?2
                    )
                    ",
                    params![context.source_id, path, session_id],
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
                          SELECT 1 FROM agent_invocations WHERE session_id = ?1
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM token_usage_records WHERE session_id = ?1
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM session_events WHERE session_id = ?1
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM mcp_tool_call WHERE session_id = ?1
                      )
                    ",
                    [previous_session_id],
                )
                .map_err(|error| format!("failed to remove fallback session metadata: {error}"))?;
        }
    }
    Ok(())
}

fn current_invocation(
    transaction: &Transaction<'_>,
    context: &ProviderContext,
    path: &str,
) -> Result<Option<(String, String, Option<i64>)>, String> {
    transaction
        .query_row(
            "
            SELECT invocations.id, invocations.session_id, invocations.started_at_ms
            FROM rollout_sources sources
            JOIN agent_invocations invocations ON invocations.id = sources.current_invocation_id
            WHERE sources.source_id = ?1 AND sources.path = ?2
            ",
            params![context.source_id, path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| format!("failed to resolve the current invocation: {error}"))
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
            for event in items {
                collect_tool_output_text(event, texts);
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
        Value::Array(items) => items.iter().any(|event| value_contains_text(event, needle)),
        Value::Object(object) => object
            .values()
            .any(|event| value_contains_text(event, needle)),
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
fn log_diagnostics(provider: &str, batch: &Batch) {
    if batch.diagnostics.is_empty() {
        return;
    }
    for diagnostic in batch.diagnostics.iter().take(20) {
        log::warn!(
            "{provider} analytics diagnostic [{}]: {}",
            diagnostic.code,
            diagnostic.message
        );
    }
    if batch.diagnostics.len() > 20 {
        log::warn!(
            "{} additional {provider} analytics diagnostics were omitted from the log",
            batch.diagnostics.len() - 20,
        );
    }
}

#[cfg(test)]
#[path = "ingestion_tests.rs"]
mod tests;
