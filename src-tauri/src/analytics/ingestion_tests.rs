use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use coding_agent_data::{
    providers::{claude_code::ClaudeCodeProvider, codex::CodexProvider},
    Actor, AgentInvocation, AgentInvocationStatus, Batch, Change, Checkpoint, DataQuality, Event,
    EventData, EventSequence, Message, MessageRole, Provider, Record, RecordData, RecordId, Retry,
    Session, SourceId, SourceLocation, SourceRef, StopReason, Timestamp, TokenUsage, ToolCall,
    ToolKind, ToolResult, ToolSourceKind, ToolStatus, UsageReport,
};
use serde_json::{json, Value};

use super::{
    apply_batch as apply_provider_batch, apply_test_batch as apply_batch,
    skill_names_from_tool_output, ProviderContext,
};
use crate::analytics::model::SessionPageRequest;
use crate::analytics::repository::{
    analytics_snapshot, session_detail, session_page, skill_analysis,
};
use crate::database::Database;

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);
const SOURCE_ID: &str = "codex:test";

#[derive(Debug)]
struct RealProjectionSummary {
    provider: String,
    batches: usize,
    changes: usize,
    diagnostics: usize,
    mcp_tool_calls: i64,
}

fn test_database_path() -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "harness-lens-analytics-test-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ))
        .join("harness-lens.sqlite")
}

fn checkpoint() -> Checkpoint {
    Checkpoint::from_json(
        &json!({
            "provider": "codex",
            "source": SOURCE_ID,
            "state": {}
        })
        .to_string(),
    )
    .unwrap()
}

fn session_id(external_id: &str) -> RecordId {
    RecordId::new(format!("{SOURCE_ID}:session:{external_id}"))
}

fn invocation_id(path: &Path, external_id: &str) -> RecordId {
    RecordId::new(format!(
        "{SOURCE_ID}:invocation:{}:{external_id}",
        path.to_string_lossy()
    ))
}

fn origin(path: &Path) -> SourceRef {
    SourceRef {
        source: SourceId::new(SOURCE_ID),
        path: path.to_path_buf(),
        location: SourceLocation::JsonLine {
            line: 1,
            byte_start: None,
            byte_end: None,
        },
    }
}

fn session_record(external_id: &str, title: &str, transcript: &Path, total_tokens: i64) -> Record {
    let source = SourceId::new(SOURCE_ID);
    let mut session = Session::new(external_id);
    session.title = Some(title.to_owned());
    session.cwd = Some(PathBuf::from("/tmp/harness-lens"));
    session.transcript = Some(transcript.to_path_buf());
    session.created_at = Some(Timestamp::from_seconds(1_700_000_000));
    session.updated_at = Some(Timestamp::from_seconds(1_700_000_100));
    session.total_tokens = Some(total_tokens);
    session.model = Some("gpt-5-codex".to_owned());
    session.model_provider = Some("openai".to_owned());
    session.agent_version = Some("1.0.0".to_owned());
    session.git_branch = Some("main".to_owned());
    session.quality = DataQuality::Complete;
    let mut record = Record::new(
        session_id(external_id),
        source.clone(),
        SourceRef {
            source: SourceId::new(SOURCE_ID),
            path: PathBuf::from("/tmp/state_5.sqlite"),
            location: SourceLocation::DatabaseRecord {
                key: external_id.to_owned(),
            },
        },
        RecordData::Session(session),
    );
    record.timestamp = Some(Timestamp::from_seconds(1_700_000_100));
    record
}

fn message_record(
    transcript: &Path,
    session: &str,
    invocation: &str,
    position: u64,
    role: MessageRole,
    text: &str,
) -> Record {
    let event = Event::new(
        EventSequence::new(position, 0),
        match &role {
            MessageRole::User => Actor::User,
            MessageRole::Assistant => Actor::Agent,
            MessageRole::System | MessageRole::Developer => Actor::System,
            MessageRole::Tool => Actor::Tool,
            _ => Actor::Agent,
        },
        EventData::Message(Message {
            role,
            phase: None,
            content: vec![coding_agent_data::ContentBlock::text(text)],
        }),
    );
    let mut record = Record::new(
        RecordId::new(format!(
            "{SOURCE_ID}:message:{}:{position}",
            transcript.to_string_lossy()
        )),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Event(event),
    );
    record.session = Some(session_id(session));
    record.invocation = Some(invocation_id(transcript, invocation));
    record.timestamp = Some(Timestamp::from_seconds(
        1_700_000_000 + i64::try_from(position).unwrap(),
    ));
    record
}

fn invocation_record(
    transcript: &Path,
    session: &str,
    external_id: &str,
    status: AgentInvocationStatus,
    timing: (i64, Option<i64>, Option<i64>),
    error: Option<&str>,
) -> Record {
    let (started_at, completed_at, duration_ms) = timing;
    let id = invocation_id(transcript, external_id);
    let mut record = Record::new(
        id.clone(),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::AgentInvocation(AgentInvocation {
            invocation_id: external_id.to_owned(),
            context_id: None,
            task_id: None,
            operation: coding_agent_data::AgentOperation::Invoke,
            sender_id: None,
            receiver_ids: Vec::new(),
            child_session: None,
            status,
            started_at: Some(Timestamp::from_seconds(started_at)),
            completed_at: completed_at.map(Timestamp::from_seconds),
            duration_ms,
            error: error.map(str::to_owned),
            stop_reason: match status {
                AgentInvocationStatus::Completed => Some(StopReason::EndInvocation),
                AgentInvocationStatus::Failed => Some(StopReason::Failed),
                AgentInvocationStatus::Cancelled => Some(StopReason::Cancelled),
                AgentInvocationStatus::Interrupted => Some(StopReason::Interrupted),
                _ => None,
            },
            trace_id: None,
            model_context_window: None,
            time_to_first_token_ms: None,
            input: None,
            output: None,
            artifacts: Vec::new(),
        }),
    );
    record.session = Some(session_id(session));
    record.invocation = Some(id);
    record.timestamp = completed_at
        .or(Some(started_at))
        .map(Timestamp::from_seconds);
    record
}

fn usage_record(
    transcript: &Path,
    session: &str,
    invocation: &str,
    total_tokens: i64,
    delta_tokens: Option<i64>,
) -> Record {
    let invocation_id = invocation_id(transcript, invocation);
    let mut record = Record::new(
        RecordId::new(format!(
            "{SOURCE_ID}:usage:{}:{total_tokens}",
            transcript.to_string_lossy()
        )),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::UsageReport(UsageReport {
            model_provider: None,
            model: None,
            service_tier: None,
            request_id: None,
            invocation_id: None,
            cumulative: Some(TokenUsage {
                total: total_tokens,
                ..TokenUsage::default()
            }),
            delta: delta_tokens.map(|total| TokenUsage {
                total,
                ..TokenUsage::default()
            }),
            cost: None,
        }),
    );
    record.session = Some(session_id(session));
    record.invocation = Some(invocation_id);
    record
}

fn tool_call_record(
    transcript: &Path,
    session: &str,
    invocation: &str,
    call_id: &str,
    input: Value,
) -> Record {
    tool_call_record_with_status(
        transcript,
        session,
        invocation,
        call_id,
        input,
        ToolStatus::Pending,
    )
}

fn tool_call_record_with_status(
    transcript: &Path,
    session: &str,
    invocation: &str,
    call_id: &str,
    input: Value,
    status: ToolStatus,
) -> Record {
    let mut event = Event::new(
        EventSequence::new(1, 0),
        Actor::Agent,
        EventData::ToolCall(ToolCall {
            call_id: call_id.to_owned(),
            name: "query".to_owned(),
            namespace: Some("database".to_owned()),
            source_kind: ToolSourceKind::Mcp,
            server_name: Some("database".to_owned()),
            title: None,
            kind: ToolKind::Execute,
            status,
            input,
            locations: Vec::new(),
        }),
    );
    event.external_id = Some(call_id.to_owned());
    let mut record = Record::new(
        RecordId::new(format!("{SOURCE_ID}:tool-call:{call_id}")),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Event(event),
    );
    record.session = Some(session_id(session));
    record.invocation = Some(invocation_id(transcript, invocation));
    record
}

fn tool_result_record(
    transcript: &Path,
    session: &str,
    invocation: &str,
    call_id: &str,
    output: Value,
    success: bool,
) -> Record {
    let mut event = Event::new(
        EventSequence::new(2, 0),
        Actor::Tool,
        EventData::ToolResult(ToolResult {
            call_id: call_id.to_owned(),
            name: Some("exec_command".to_owned()),
            content: Vec::new(),
            output,
            status: if success {
                ToolStatus::Completed
            } else {
                ToolStatus::Failed
            },
            error: None,
            duration_ms: None,
        }),
    );
    event.external_id = Some(call_id.to_owned());
    event.parent = Some(RecordId::new(format!("{SOURCE_ID}:tool-call:{call_id}")));
    let mut record = Record::new(
        RecordId::new(format!("{SOURCE_ID}:tool-result:{call_id}")),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Event(event),
    );
    record.session = Some(session_id(session));
    record.invocation = Some(invocation_id(transcript, invocation));
    record
}

fn retry_record(
    transcript: &Path,
    session: &str,
    invocation: &str,
    retry_id: &str,
    parent_call_id: Option<&str>,
) -> Record {
    let mut event = Event::new(
        EventSequence::new(3, 0),
        Actor::Agent,
        EventData::Retry(Retry {
            attempt: Some(2),
            reason: Some("sanitized fixture".to_owned()),
            delay_ms: Some(100),
        }),
    );
    event.parent =
        parent_call_id.map(|call_id| RecordId::new(format!("{SOURCE_ID}:tool-call:{call_id}")));
    let mut record = Record::new(
        RecordId::new(format!("{SOURCE_ID}:retry:{retry_id}")),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Event(event),
    );
    record.session = Some(session_id(session));
    record.invocation = Some(invocation_id(transcript, invocation));
    record
}

#[test]
fn explicit_retry_evidence_is_attributed_without_self_referencing_calls() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("explicit-retry.jsonl");
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record("session-retry", "Retry", &transcript, 0)),
            Change::upsert(invocation_record(
                &transcript,
                "session-retry",
                "invocation-retry",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-retry",
                "invocation-retry",
                "call-retry",
                json!({"value": 1}),
            )),
            Change::upsert(retry_record(
                &transcript,
                "session-retry",
                "invocation-retry",
                "attributed",
                Some("call-retry"),
            )),
            Change::upsert(retry_record(
                &transcript,
                "session-retry",
                "invocation-retry",
                "unattributed",
                None,
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &batch, "ready", "watching").unwrap();

    let connection = database.connect().unwrap();
    let call: (i64, String, Option<String>) = connection
        .query_row(
            "SELECT explicit_retry_count, retry_class, retry_of_id FROM mcp_tool_call WHERE call_id = 'call-retry'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let evidence: i64 = connection
        .query_row("SELECT COUNT(*) FROM mcp_tool_call_retry", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(call, (1, "none".to_owned(), None));
    assert_eq!(evidence, 1);

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn built_in_tool_events_remain_in_history_but_not_in_mcp_projection() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("built-in.jsonl");
    let mut built_in = tool_call_record(
        &transcript,
        "session-built-in",
        "invocation-built-in",
        "call-built-in",
        json!({"cmd": "pwd"}),
    );
    let RecordData::Event(event) = &mut built_in.data else {
        unreachable!();
    };
    let EventData::ToolCall(call) = &mut event.data else {
        unreachable!();
    };
    call.name = "exec_command".to_owned();
    call.namespace = None;
    call.source_kind = ToolSourceKind::BuiltIn;
    call.server_name = None;
    call.kind = ToolKind::Execute;

    let batch = Batch {
        changes: vec![
            Change::upsert(session_record(
                "session-built-in",
                "Built in",
                &transcript,
                0,
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-built-in",
                "invocation-built-in",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(built_in),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &batch, "ready", "watching").unwrap();

    let connection = database.connect().unwrap();
    let history_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM session_events WHERE event_type = 'tool_call'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let projection_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM mcp_tool_call", [], |row| row.get(0))
        .unwrap();
    assert_eq!(history_count, 1);
    assert_eq!(projection_count, 0);

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn creates_and_upgrades_fallback_session_for_out_of_order_records() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("out-of-order.jsonl");
    let invocation = invocation_record(
        &transcript,
        "session-out-of-order",
        "invocation-out-of-order",
        AgentInvocationStatus::InProgress,
        (1_700_000_000, None, None),
        None,
    );
    apply_batch(
        &database,
        &Batch {
            changes: vec![Change::upsert(invocation)],
            checkpoint: checkpoint(),
            diagnostics: Vec::new(),
            has_more: true,
        },
        "syncing",
        "initial_scan",
    )
    .unwrap();

    let fallback: (i64, String, String) = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT metadata_present, data_quality, title FROM agent_sessions WHERE id = ?1",
            [session_id("session-out-of-order").as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(fallback, (0, "partial".to_owned(), String::new()));

    apply_batch(
        &database,
        &Batch {
            changes: vec![Change::upsert(session_record(
                "session-out-of-order",
                "Recovered metadata",
                &transcript,
                42,
            ))],
            checkpoint: checkpoint(),
            diagnostics: Vec::new(),
            has_more: false,
        },
        "ready",
        "watching",
    )
    .unwrap();
    let upgraded: (i64, String, String) = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT metadata_present, data_quality, title FROM agent_sessions WHERE id = ?1",
            [session_id("session-out-of-order").as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        upgraded,
        (1, "complete".to_owned(), "Recovered metadata".to_owned())
    );
    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn projects_tool_calls_results_repeats_and_inferred_retries() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("tool-calls.jsonl");
    let input = json!({"path": "README.md", "options": {"b": 2, "a": 1}});
    let equivalent_input = json!({"options": {"a": 1, "b": 2}, "path": "README.md"});
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record("session-tools", "Tools", &transcript, 0)),
            Change::upsert(invocation_record(
                &transcript,
                "session-tools",
                "invocation-tools",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-tools",
                "invocation-tools",
                "failed",
                input,
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-tools",
                "invocation-tools",
                "failed",
                json!({"error": "fixture"}),
                false,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-tools",
                "invocation-tools",
                "retry",
                equivalent_input,
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-tools",
                "invocation-tools",
                "retry",
                json!({"ok": true}),
                true,
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let connection = database.connect().unwrap();
    let rows = connection
        .prepare(
            "SELECT call_id, status, has_result, repeat_index, retry_class, call_event_id IS NOT NULL, result_event_id IS NOT NULL FROM mcp_tool_call ORDER BY repeat_index",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        (
            "failed".to_owned(),
            "failed".to_owned(),
            1,
            0,
            "none".to_owned(),
            1,
            1
        )
    );
    assert_eq!(
        rows[1],
        (
            "retry".to_owned(),
            "completed".to_owned(),
            1,
            1,
            "inferred".to_owned(),
            1,
            1
        )
    );
    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn result_before_call_is_reconciled_and_declined_is_not_inferred_retry() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("out-of-order.jsonl");
    let same = json!({"cmd": "safe fixture"});
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record("session-order", "Order", &transcript, 0)),
            Change::upsert(invocation_record(
                &transcript,
                "session-order",
                "invocation-order",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-order",
                "invocation-order",
                "out-of-order",
                json!({"ok": true}),
                true,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-order",
                "invocation-order",
                "out-of-order",
                json!({"value": 1}),
            )),
            Change::upsert(tool_call_record_with_status(
                &transcript,
                "session-order",
                "invocation-order",
                "declined",
                same.clone(),
                ToolStatus::Declined,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-order",
                "invocation-order",
                "after-declined",
                same,
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let connection = database.connect().unwrap();
    let reconciled: (String, i64, i64) = connection
        .query_row(
            "SELECT tool_name, has_result, call_event_id IS NOT NULL FROM mcp_tool_call WHERE call_id = 'out-of-order'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let retry_class: String = connection
        .query_row(
            "SELECT retry_class FROM mcp_tool_call WHERE call_id = 'after-declined'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(reconciled, ("query".to_owned(), 1, 1));
    assert_eq!(retry_class, "none");
    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn deleting_tool_evidence_updates_or_removes_the_projection() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("delete-tool.jsonl");
    let call = tool_call_record(
        &transcript,
        "session-delete-tool",
        "invocation-delete-tool",
        "delete-me",
        json!({"value": 1}),
    );
    let call_id = call.id.clone();
    let result = tool_result_record(
        &transcript,
        "session-delete-tool",
        "invocation-delete-tool",
        "delete-me",
        json!({"ok": true}),
        true,
    );
    let result_id = result.id.clone();
    let initial = Batch {
        changes: vec![
            Change::upsert(session_record(
                "session-delete-tool",
                "Delete tool",
                &transcript,
                0,
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-delete-tool",
                "invocation-delete-tool",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(call),
            Change::upsert(result),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &initial, "ready", "watching").unwrap();
    let delete_result = Batch {
        changes: vec![Change::Delete(result_id)],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &delete_result, "ready", "watching").unwrap();
    let projection: (i64, String, Option<i64>, Option<i64>) = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT has_result, status, completed_at_ms, duration_ms FROM mcp_tool_call WHERE call_id = 'delete-me'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(projection, (0, "pending".to_owned(), None, None));

    let delete_call = Batch {
        changes: vec![Change::Delete(call_id)],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &delete_call, "ready", "watching").unwrap();
    let count: i64 = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM mcp_tool_call WHERE call_id = 'delete-me'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn imports_skill_reads_and_turn_metrics_from_normalized_records() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("rollout.jsonl");
    let skill_input = |name: &str| {
        json!({
            "cmd": format!("sed -n '1,240p' /skills/{name}/SKILL.md")
        })
    };
    let skill_output = |body: &str| json!({"output": body});
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record(
                "session-1",
                "Build analytics",
                &transcript,
                260,
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-1",
                "invocation-1",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "invocation-1",
                "call-1",
                skill_input("octocode-research"),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "invocation-1",
                "call-1",
                skill_output("---\nname: octocode-research\ndescription: Research\n---\n"),
                true,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "invocation-1",
                "call-1-repeat",
                skill_input("octocode-research"),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "invocation-1",
                "call-1-repeat",
                skill_output("---\nname: octocode-research\ndescription: Research\n---\n"),
                true,
            )),
            Change::upsert(usage_record(
                &transcript,
                "session-1",
                "invocation-1",
                150,
                Some(150),
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-1",
                "invocation-1",
                AgentInvocationStatus::Completed,
                (1_700_000_000, Some(1_700_000_010), Some(10_000)),
                None,
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-1",
                "invocation-2",
                AgentInvocationStatus::InProgress,
                (1_700_000_020, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "invocation-2",
                "call-2",
                json!({
                    "cmd": "sed /skills/octocode-research/SKILL.md && sed /skills/tauri-codegen/SKILL.md"
                }),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "invocation-2",
                "call-2",
                skill_output(
                    "---\nname: octocode-research\ndescription: Research\n---\n\
                     ---\nname: tauri-codegen\ndescription: Tauri\n---\n",
                ),
                true,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "invocation-2",
                "call-failed",
                skill_input("missing"),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "invocation-2",
                "call-failed",
                skill_output("---\nname: missing\ndescription: Missing\n---\n"),
                false,
            )),
            Change::upsert(usage_record(
                &transcript,
                "session-1",
                "invocation-2",
                260,
                Some(110),
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-1",
                "invocation-2",
                AgentInvocationStatus::Failed,
                (1_700_000_020, Some(1_700_000_040), Some(20_000)),
                Some("tool failed"),
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let snapshot = analytics_snapshot(&database).unwrap();
    let skill_analysis = skill_analysis(&database).unwrap();

    assert_eq!(snapshot.summary.session_count, 1);
    assert_eq!(snapshot.summary.total_tokens, 260);
    assert_eq!(snapshot.summary.skill_invocation_count, 3);
    assert_eq!(snapshot.summary.succeeded_invocation_count, 1);
    assert_eq!(snapshot.summary.failed_invocation_count, 1);
    assert_eq!(snapshot.sessions[0].invocation_count, 2);
    assert_eq!(snapshot.sessions[0].skill_invocation_count, 3);
    assert_eq!(snapshot.sessions[0].status, "failed");
    assert!(snapshot.skills.iter().all(|skill| skill.name != "missing"));
    assert_eq!(skill_analysis.skills.len(), snapshot.skills.len());

    let research = snapshot
        .skills
        .iter()
        .find(|skill| skill.name == "octocode-research")
        .unwrap();
    assert_eq!(research.invocation_count, 2);
    assert_eq!(research.succeeded_count, 1);
    assert_eq!(research.failed_count, 1);
    assert_eq!(research.average_duration_ms, Some(15_000.0));
    assert_eq!(research.max_duration_ms, Some(20_000));
    assert_eq!(research.average_tokens, Some(130.0));
    assert_eq!(research.max_tokens, Some(150));

    let tauri = snapshot
        .skills
        .iter()
        .find(|skill| skill.name == "tauri-codegen")
        .unwrap();
    assert_eq!(tauri.invocation_count, 1);
    assert_eq!(tauri.failed_count, 1);
    assert_eq!(tauri.average_duration_ms, Some(20_000.0));
    assert_eq!(tauri.average_tokens, Some(110.0));

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn stores_paginated_sessions_and_normalized_event_history() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let mut changes = Vec::new();

    for index in 0..12 {
        let session = format!("session-{index:02}");
        let transcript = database_path
            .parent()
            .unwrap()
            .join(format!("{session}.jsonl"));
        changes.push(Change::upsert(session_record(
            &session,
            &format!("Session {index:02}"),
            &transcript,
            i64::from(index),
        )));
        if index == 0 {
            changes.extend([
                Change::upsert(message_record(
                    &transcript,
                    &session,
                    "invocation-1",
                    0,
                    MessageRole::User,
                    "Show the session history",
                )),
                Change::upsert(tool_call_record(
                    &transcript,
                    &session,
                    "invocation-1",
                    "call-history",
                    json!({"cmd": "pwd"}),
                )),
                Change::upsert(tool_result_record(
                    &transcript,
                    &session,
                    "invocation-1",
                    "call-history",
                    json!({"output": "/tmp/harness-lens"}),
                    true,
                )),
            ]);
        }
    }

    let batch = Batch {
        changes,
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &batch, "ready", "watching").unwrap();

    let second_page = session_page(
        &database,
        SessionPageRequest {
            page: 2,
            page_size: 10,
            query: None,
            archived: None,
        },
    )
    .unwrap();
    assert_eq!(second_page.total, 12);
    assert_eq!(second_page.page, 2);
    assert_eq!(second_page.items.len(), 2);

    let filtered = session_page(
        &database,
        SessionPageRequest {
            page: 1,
            page_size: 10,
            query: Some("Session 00".to_owned()),
            archived: None,
        },
    )
    .unwrap();
    assert_eq!(filtered.total, 1);
    assert_eq!(filtered.items[0].event_count, 3);
    assert_eq!(filtered.items[0].model.as_deref(), Some("gpt-5-codex"));
    assert_eq!(filtered.items[0].git_branch.as_deref(), Some("main"));

    let detail = session_detail(&database, session_id("session-00").as_str())
        .unwrap()
        .unwrap();
    assert_eq!(detail.data_quality, "complete");
    assert_eq!(
        detail
            .events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        ["message", "tool_call", "tool_result"]
    );
    assert_eq!(detail.events[0].event["data"]["type"], "message");

    let first_user_message: (Option<String>, Option<String>, Option<i64>) = database
        .connect()
        .unwrap()
        .query_row(
            "
            SELECT first_user_message_text,
                   first_user_message_event_id,
                   first_user_message_timestamp_ms
            FROM agent_sessions
            WHERE id = ?1
            ",
            [session_id("session-00").as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        first_user_message.0.as_deref(),
        Some("Show the session history")
    );
    let expected_first_user_message_event_id = format!(
        "{SOURCE_ID}:message:{}:0",
        database_path
            .parent()
            .unwrap()
            .join("session-00.jsonl")
            .to_string_lossy()
    );
    assert_eq!(
        first_user_message.1.as_deref(),
        Some(expected_first_user_message_event_id.as_str())
    );
    assert_eq!(first_user_message.2, Some(1_700_000_000_000));

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn first_user_message_projection_handles_reordering_and_deletion() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("messages.jsonl");
    let late = message_record(
        &transcript,
        "session-messages",
        "invocation-messages",
        20,
        MessageRole::User,
        "late",
    );
    let early = message_record(
        &transcript,
        "session-messages",
        "invocation-messages",
        10,
        MessageRole::User,
        "early",
    );
    let mut actor_user = message_record(
        &transcript,
        "session-messages",
        "invocation-messages",
        15,
        MessageRole::Assistant,
        "actor-user",
    );
    if let RecordData::Event(event) = &mut actor_user.data {
        event.actor = Actor::User;
    }
    apply_batch(
        &database,
        &Batch {
            changes: vec![
                Change::upsert(session_record(
                    "session-messages",
                    "Messages",
                    &transcript,
                    0,
                )),
                Change::upsert(late),
                Change::upsert(early),
                Change::upsert(actor_user),
            ],
            checkpoint: checkpoint(),
            diagnostics: Vec::new(),
            has_more: false,
        },
        "ready",
        "watching",
    )
    .unwrap();

    let first_text = |database: &Database| {
        database
            .connect()
            .unwrap()
            .query_row(
                "SELECT first_user_message_text FROM agent_sessions WHERE id = ?1",
                [session_id("session-messages").as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap()
    };
    assert_eq!(first_text(&database).as_deref(), Some("early"));

    let earlier = message_record(
        &transcript,
        "session-messages",
        "invocation-messages",
        5,
        MessageRole::User,
        "earlier",
    );
    let earlier_id = earlier.id.clone();
    apply_batch(
        &database,
        &Batch {
            changes: vec![Change::upsert(earlier)],
            checkpoint: checkpoint(),
            diagnostics: Vec::new(),
            has_more: false,
        },
        "ready",
        "watching",
    )
    .unwrap();
    assert_eq!(first_text(&database).as_deref(), Some("earlier"));

    apply_batch(
        &database,
        &Batch {
            changes: vec![Change::Delete(earlier_id)],
            checkpoint: checkpoint(),
            diagnostics: Vec::new(),
            has_more: false,
        },
        "ready",
        "watching",
    )
    .unwrap();
    assert_eq!(first_text(&database).as_deref(), Some("early"));

    apply_batch(
        &database,
        &Batch {
            changes: vec![Change::Delete(RecordId::new(format!(
                "{SOURCE_ID}:message:{}:10",
                transcript.to_string_lossy()
            )))],
            checkpoint: checkpoint(),
            diagnostics: Vec::new(),
            has_more: false,
        },
        "ready",
        "watching",
    )
    .unwrap();
    assert_eq!(first_text(&database).as_deref(), Some("actor-user"));

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn fork_replay_does_not_inflate_session_turn_or_global_tokens() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let parent = database_path.parent().unwrap().join("parent.jsonl");
    let child = database_path.parent().unwrap().join("child.jsonl");
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record("parent", "Parent", &parent, 150)),
            Change::upsert(invocation_record(
                &parent,
                "parent",
                "parent-invocation",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(usage_record(
                &parent,
                "parent",
                "parent-invocation",
                100,
                Some(100),
            )),
            Change::upsert(usage_record(
                &parent,
                "parent",
                "parent-invocation",
                150,
                Some(50),
            )),
            Change::upsert(invocation_record(
                &parent,
                "parent",
                "parent-invocation",
                AgentInvocationStatus::Completed,
                (1_700_000_000, Some(1_700_000_010), Some(10_000)),
                None,
            )),
            Change::upsert(session_record("child", "Child", &child, 150)),
            Change::upsert(invocation_record(
                &child,
                "child",
                "child-invocation",
                AgentInvocationStatus::InProgress,
                (1_700_000_020, None, None),
                None,
            )),
            Change::upsert(usage_record(&child, "child", "child-invocation", 100, None)),
            Change::upsert(usage_record(
                &child,
                "child",
                "child-invocation",
                150,
                Some(50),
            )),
            Change::upsert(invocation_record(
                &child,
                "child",
                "child-invocation",
                AgentInvocationStatus::Completed,
                (1_700_000_020, Some(1_700_000_030), Some(10_000)),
                None,
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let snapshot = analytics_snapshot(&database).unwrap();
    let parent_tokens = snapshot
        .sessions
        .iter()
        .find(|session| session.title == "Parent")
        .map(|session| session.tokens_used);
    let child_tokens = snapshot
        .sessions
        .iter()
        .find(|session| session.title == "Child")
        .map(|session| session.tokens_used);
    let child_turn_tokens: Option<i64> = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT total_tokens FROM agent_invocations WHERE id = ?1",
            [invocation_id(&child, "child-invocation").as_str()],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(snapshot.summary.total_tokens, 200);
    assert_eq!(parent_tokens, Some(150));
    assert_eq!(child_tokens, Some(50));
    assert_eq!(child_turn_tokens, Some(50));

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn session_tokens_fall_back_to_the_provider_total_when_no_delta_is_attributed() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("rollout.jsonl");
    let batch = Batch {
        changes: vec![Change::upsert(session_record(
            "session-1",
            "Provider token fallback",
            &transcript,
            321,
        ))],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let snapshot = analytics_snapshot(&database).unwrap();

    assert_eq!(snapshot.summary.total_tokens, 0);
    assert_eq!(snapshot.sessions[0].tokens_used, 321);

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn source_scoped_invocation_ids_keep_provider_reused_ids_separate() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let first = database_path.parent().unwrap().join("first.jsonl");
    let second = database_path.parent().unwrap().join("second.jsonl");
    let mut changes = Vec::new();

    for (session, transcript) in [
        ("session-1", first.as_path()),
        ("session-2", second.as_path()),
    ] {
        changes.extend([
            Change::upsert(session_record(session, session, transcript, 10)),
            Change::Reset(SourceRef {
                source: SourceId::new(SOURCE_ID),
                path: transcript.to_path_buf(),
                location: SourceLocation::WholeFile,
            }),
            Change::upsert(invocation_record(
                transcript,
                session,
                "shared-invocation-id",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(invocation_record(
                transcript,
                session,
                "shared-invocation-id",
                AgentInvocationStatus::Completed,
                (1_700_000_000, Some(1_700_000_001), Some(1_000)),
                None,
            )),
        ]);
    }
    let batch = Batch {
        changes,
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let snapshot = analytics_snapshot(&database).unwrap();

    assert_eq!(snapshot.summary.session_count, 2);
    assert_eq!(snapshot.summary.succeeded_invocation_count, 2);
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .map(|session| session.invocation_count)
            .sum::<i64>(),
        2
    );

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn terminal_turn_uses_the_stored_start_time_when_the_event_omits_it() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("rollout.jsonl");
    let running = invocation_record(
        &transcript,
        "session-1",
        "invocation-1",
        AgentInvocationStatus::InProgress,
        (1_700_000_000, None, None),
        None,
    );
    let mut terminal = invocation_record(
        &transcript,
        "session-1",
        "invocation-1",
        AgentInvocationStatus::Completed,
        (1_700_000_000, Some(1_700_000_010), None),
        None,
    );
    let RecordData::AgentInvocation(invocation) = &mut terminal.data else {
        unreachable!();
    };
    invocation.started_at = None;

    let batch = Batch {
        changes: vec![
            Change::upsert(session_record(
                "session-1",
                "Duration fallback",
                &transcript,
                10,
            )),
            Change::upsert(running),
            Change::upsert(terminal),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &batch, "ready", "watching").unwrap();

    let snapshot = analytics_snapshot(&database).unwrap();
    assert_eq!(snapshot.sessions[0].observed_duration_ms, 10_000);
    let current_invocation_id: Option<String> = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT current_invocation_id FROM rollout_sources WHERE path = ?1",
            [transcript.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(current_invocation_id.is_none());

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn interrupted_turns_are_counted_as_cancelled() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("rollout.jsonl");
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record(
                "session-1",
                "Interrupted session",
                &transcript,
                10,
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-1",
                "invocation-1",
                AgentInvocationStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "invocation-1",
                "call-1",
                json!({"cmd": "sed -n '1,120p' /skills/example/SKILL.md"}),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "invocation-1",
                "call-1",
                json!({"output": "---\nname: example\ndescription: Example\n---\n"}),
                true,
            )),
            Change::upsert(invocation_record(
                &transcript,
                "session-1",
                "invocation-1",
                AgentInvocationStatus::Interrupted,
                (1_700_000_000, Some(1_700_000_010), Some(10_000)),
                None,
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let snapshot = analytics_snapshot(&database).unwrap();

    assert_eq!(snapshot.summary.cancelled_invocation_count, 1);
    assert_eq!(snapshot.sessions[0].status, "cancelled");
    assert_eq!(snapshot.skills[0].cancelled_count, 1);

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn orphan_turn_and_usage_records_do_not_abort_the_batch() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("orphan.jsonl");
    let mut running = invocation_record(
        &transcript,
        "missing-session",
        "invocation-1",
        AgentInvocationStatus::InProgress,
        (1_700_000_000, None, None),
        None,
    );
    running.session = None;
    let mut usage = usage_record(
        &transcript,
        "missing-session",
        "invocation-1",
        100,
        Some(100),
    );
    usage.session = None;
    let mut terminal = invocation_record(
        &transcript,
        "missing-session",
        "invocation-1",
        AgentInvocationStatus::Completed,
        (1_700_000_000, Some(1_700_000_010), None),
        None,
    );
    terminal.session = None;
    let batch = Batch {
        changes: vec![
            Change::upsert(running),
            Change::upsert(usage),
            Change::upsert(terminal),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    assert_eq!(
        analytics_snapshot(&database).unwrap().summary.session_count,
        0
    );

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn skill_reads_linked_to_an_unprojected_turn_do_not_abort_the_batch() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("orphan-skill.jsonl");
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record(
                "session-1",
                "Orphan Skill invocation",
                &transcript,
                10,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "missing-invocation",
                "call-1",
                json!({"cmd": "sed -n '1,120p' /skills/example/SKILL.md"}),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "missing-invocation",
                "call-1",
                json!({"output": "---\nname: example\ndescription: Example\n---\n"}),
                true,
            )),
        ],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };

    apply_batch(&database, &batch, "ready", "watching").unwrap();
    let snapshot = analytics_snapshot(&database).unwrap();

    assert_eq!(snapshot.summary.session_count, 1);
    assert_eq!(snapshot.summary.skill_invocation_count, 0);
    assert!(snapshot.skills.is_empty());

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn delete_changes_remove_projected_session_records() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("rollout.jsonl");
    let initial = Batch {
        changes: vec![Change::upsert(session_record(
            "session-1",
            "Temporary session",
            &transcript,
            10,
        ))],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &initial, "ready", "watching").unwrap();

    let deletion = Batch {
        changes: vec![Change::Delete(session_id("session-1"))],
        checkpoint: checkpoint(),
        diagnostics: Vec::new(),
        has_more: false,
    };
    apply_batch(&database, &deletion, "ready", "watching").unwrap();

    assert_eq!(
        analytics_snapshot(&database).unwrap().summary.session_count,
        0
    );
    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn skill_parser_extracts_unique_names_from_successful_tool_output() {
    let names = skill_names_from_tool_output(&json!([
        {"type": "input_text", "text": "Script completed"},
        {
            "type": "input_text",
            "text": "---\nname: valid-skill\ndescription: First\n---\n\
                     ---\nname: \"valid-skill\"\ndescription: Duplicate\n---\n\
                     ---\nname: invalid skill\ndescription: Invalid\n---\n"
        }
    ]));

    assert_eq!(names.into_iter().collect::<Vec<_>>(), vec!["valid-skill"]);
}

fn project_real_provider<P: Provider>(database: &Database, provider: &P) -> RealProjectionSummary {
    let context = ProviderContext {
        provider: provider.info().id.as_str().to_owned(),
        source_id: provider.info().source.as_str().to_owned(),
    };
    let mut checkpoint = None;
    let mut batches = 0;
    let mut changes = 0;
    let mut diagnostics = 0;
    loop {
        let batch = provider
            .scan(checkpoint.as_ref())
            .unwrap_or_else(|error| panic!("{} real-data scan failed: {error}", context.provider));
        batches += 1;
        changes += batch.changes.len();
        diagnostics += batch.diagnostics.len();
        apply_provider_batch(
            database,
            &context,
            &batch,
            if batch.has_more { "syncing" } else { "ready" },
            "real_data_validation",
        )
        .unwrap_or_else(|error| {
            panic!("{} real-data projection failed: {error}", context.provider)
        });
        checkpoint = Some(batch.checkpoint.clone());
        if !batch.has_more {
            break;
        }
    }
    let mcp_tool_calls = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM mcp_tool_call WHERE source_id = ?1",
            [&context.source_id],
            |row| row.get(0),
        )
        .unwrap();
    RealProjectionSummary {
        provider: context.provider,
        batches,
        changes,
        diagnostics,
        mcp_tool_calls,
    }
}

fn integrity_count(database: &Database, query: &str) -> i64 {
    database
        .connect()
        .unwrap()
        .query_row(query, [], |row| row.get(0))
        .unwrap()
}

#[test]
#[ignore = "scans the developer's local Codex and Claude Code sources read-only"]
fn real_local_mcp_tool_call_projection_is_consistent() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let mut summaries = Vec::new();
    let provider_filter = std::env::var("HARNESS_LENS_REAL_PROVIDER").ok();
    if provider_filter
        .as_deref()
        .is_none_or(|value| value == "codex")
    {
        match CodexProvider::discover() {
            Ok(provider) => summaries.push(project_real_provider(&database, &provider)),
            Err(error) => eprintln!("provider=codex unavailable={error}"),
        }
    }
    if provider_filter
        .as_deref()
        .is_none_or(|value| value == "claude-code")
    {
        match ClaudeCodeProvider::discover() {
            Ok(provider) => summaries.push(project_real_provider(&database, &provider)),
            Err(error) => eprintln!("provider=claude-code unavailable={error}"),
        }
    }

    let connection = database.connect().unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(connection);

    let checks = [
        (
            "foreign_keys",
            "SELECT COUNT(*) FROM pragma_foreign_key_check",
        ),
        (
            "orphan_sessions",
            "SELECT COUNT(*) FROM mcp_tool_call tc LEFT JOIN agent_sessions s ON s.id = tc.session_id WHERE s.id IS NULL",
        ),
        (
            "orphan_call_events",
            "SELECT COUNT(*) FROM mcp_tool_call tc LEFT JOIN session_events e ON e.id = tc.call_event_id WHERE tc.call_event_id IS NOT NULL AND e.id IS NULL",
        ),
        (
            "orphan_result_events",
            "SELECT COUNT(*) FROM mcp_tool_call tc LEFT JOIN session_events e ON e.id = tc.result_event_id WHERE tc.result_event_id IS NOT NULL AND e.id IS NULL",
        ),
        (
            "duplicate_source_calls",
            "SELECT COUNT(*) FROM (SELECT 1 FROM mcp_tool_call GROUP BY source_id, source_path, call_id HAVING COUNT(*) > 1)",
        ),
        (
            "orphan_retry_events",
            "SELECT COUNT(*) FROM mcp_tool_call_retry retry LEFT JOIN session_events e ON e.id = retry.id WHERE e.id IS NULL",
        ),
        (
            "non_mcp_call_events",
            "SELECT COUNT(*) FROM mcp_tool_call tc JOIN session_events e ON e.id = tc.call_event_id WHERE COALESCE(json_extract(e.event_json, '$.data.value.source_kind'), '') <> 'mcp'",
        ),
    ];
    for (name, query) in checks {
        let count = integrity_count(&database, query);
        eprintln!("integrity={name} count={count}");
        assert_eq!(count, 0, "real-data integrity check failed: {name}");
    }
    for summary in summaries {
        eprintln!(
            "provider={} batches={} changes={} diagnostics={} mcp_tool_calls={}",
            summary.provider,
            summary.batches,
            summary.changes,
            summary.diagnostics,
            summary.mcp_tool_calls
        );
    }
    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}
