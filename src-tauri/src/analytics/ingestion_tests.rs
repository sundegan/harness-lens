use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use coding_agent_data::{
    Actor, Batch, Change, Checkpoint, DataQuality, Item, ItemData, ItemSequence, Record,
    RecordData, RecordId, Session, SourceId, SourceLocation, SourceRef, StopReason, Timestamp,
    TokenUsage, ToolCall, ToolKind, ToolResult, ToolStatus, Turn, TurnStatus, Usage,
};
use serde_json::{json, Value};

use super::{apply_batch, skill_names_from_tool_output};
use crate::analytics::repository::analytics_snapshot;
use crate::database::Database;

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);
const SOURCE_ID: &str = "codex:test";

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

fn turn_id(path: &Path, external_id: &str) -> RecordId {
    RecordId::new(format!(
        "{SOURCE_ID}:turn:{}:{external_id}",
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

fn turn_record(
    transcript: &Path,
    session: &str,
    external_id: &str,
    status: TurnStatus,
    timing: (i64, Option<i64>, Option<i64>),
    error: Option<&str>,
) -> Record {
    let (started_at, completed_at, duration_ms) = timing;
    let id = turn_id(transcript, external_id);
    let mut record = Record::new(
        id.clone(),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Turn(Turn {
            external_id: Some(external_id.to_owned()),
            status,
            started_at: Some(Timestamp::from_seconds(started_at)),
            completed_at: completed_at.map(Timestamp::from_seconds),
            duration_ms,
            error: error.map(str::to_owned),
            stop_reason: match status {
                TurnStatus::Completed => Some(StopReason::EndTurn),
                TurnStatus::Failed => Some(StopReason::Failed),
                TurnStatus::Cancelled => Some(StopReason::Cancelled),
                TurnStatus::Interrupted => Some(StopReason::Interrupted),
                _ => None,
            },
            trace_id: None,
            model_context_window: None,
            time_to_first_token_ms: None,
        }),
    );
    record.session = Some(session_id(session));
    record.turn = Some(id);
    record.timestamp = completed_at
        .or(Some(started_at))
        .map(Timestamp::from_seconds);
    record
}

fn usage_record(
    transcript: &Path,
    session: &str,
    turn: &str,
    total_tokens: i64,
    delta_tokens: Option<i64>,
) -> Record {
    let turn_id = turn_id(transcript, turn);
    let mut record = Record::new(
        RecordId::new(format!(
            "{SOURCE_ID}:usage:{}:{total_tokens}",
            transcript.to_string_lossy()
        )),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Usage(Usage {
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
    record.turn = Some(turn_id);
    record
}

fn tool_call_record(
    transcript: &Path,
    session: &str,
    turn: &str,
    call_id: &str,
    input: Value,
) -> Record {
    let mut item = Item::new(
        ItemSequence::new(1, 0),
        Actor::Agent,
        ItemData::ToolCall(ToolCall {
            call_id: call_id.to_owned(),
            name: "exec_command".to_owned(),
            namespace: None,
            title: None,
            kind: ToolKind::Execute,
            status: ToolStatus::Pending,
            input,
            locations: Vec::new(),
        }),
    );
    item.external_id = Some(call_id.to_owned());
    let mut record = Record::new(
        RecordId::new(format!("{SOURCE_ID}:tool-call:{call_id}")),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Item(item),
    );
    record.session = Some(session_id(session));
    record.turn = Some(turn_id(transcript, turn));
    record
}

fn tool_result_record(
    transcript: &Path,
    session: &str,
    turn: &str,
    call_id: &str,
    output: Value,
    success: bool,
) -> Record {
    let mut item = Item::new(
        ItemSequence::new(2, 0),
        Actor::Tool,
        ItemData::ToolResult(ToolResult {
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
    item.external_id = Some(call_id.to_owned());
    item.parent = Some(RecordId::new(format!("{SOURCE_ID}:tool-call:{call_id}")));
    let mut record = Record::new(
        RecordId::new(format!("{SOURCE_ID}:tool-result:{call_id}")),
        SourceId::new(SOURCE_ID),
        origin(transcript),
        RecordData::Item(item),
    );
    record.session = Some(session_id(session));
    record.turn = Some(turn_id(transcript, turn));
    record
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
            Change::upsert(turn_record(
                &transcript,
                "session-1",
                "turn-1",
                TurnStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "turn-1",
                "call-1",
                skill_input("octocode-research"),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "turn-1",
                "call-1",
                skill_output("---\nname: octocode-research\ndescription: Research\n---\n"),
                true,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "turn-1",
                "call-1-repeat",
                skill_input("octocode-research"),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "turn-1",
                "call-1-repeat",
                skill_output("---\nname: octocode-research\ndescription: Research\n---\n"),
                true,
            )),
            Change::upsert(usage_record(
                &transcript,
                "session-1",
                "turn-1",
                150,
                Some(150),
            )),
            Change::upsert(turn_record(
                &transcript,
                "session-1",
                "turn-1",
                TurnStatus::Completed,
                (1_700_000_000, Some(1_700_000_010), Some(10_000)),
                None,
            )),
            Change::upsert(turn_record(
                &transcript,
                "session-1",
                "turn-2",
                TurnStatus::InProgress,
                (1_700_000_020, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "turn-2",
                "call-2",
                json!({
                    "cmd": "sed /skills/octocode-research/SKILL.md && sed /skills/tauri-codegen/SKILL.md"
                }),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "turn-2",
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
                "turn-2",
                "call-failed",
                skill_input("missing"),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "turn-2",
                "call-failed",
                skill_output("---\nname: missing\ndescription: Missing\n---\n"),
                false,
            )),
            Change::upsert(usage_record(
                &transcript,
                "session-1",
                "turn-2",
                260,
                Some(110),
            )),
            Change::upsert(turn_record(
                &transcript,
                "session-1",
                "turn-2",
                TurnStatus::Failed,
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

    assert_eq!(snapshot.summary.session_count, 1);
    assert_eq!(snapshot.summary.total_tokens, 260);
    assert_eq!(snapshot.summary.skill_invocation_count, 3);
    assert_eq!(snapshot.summary.succeeded_turn_count, 1);
    assert_eq!(snapshot.summary.failed_turn_count, 1);
    assert_eq!(snapshot.sessions[0].turn_count, 2);
    assert_eq!(snapshot.sessions[0].skill_invocation_count, 3);
    assert_eq!(snapshot.sessions[0].status, "failed");
    assert!(snapshot.skills.iter().all(|skill| skill.name != "missing"));

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
fn fork_replay_does_not_inflate_session_turn_or_global_tokens() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let parent = database_path.parent().unwrap().join("parent.jsonl");
    let child = database_path.parent().unwrap().join("child.jsonl");
    let batch = Batch {
        changes: vec![
            Change::upsert(session_record("parent", "Parent", &parent, 150)),
            Change::upsert(turn_record(
                &parent,
                "parent",
                "parent-turn",
                TurnStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(usage_record(
                &parent,
                "parent",
                "parent-turn",
                100,
                Some(100),
            )),
            Change::upsert(usage_record(
                &parent,
                "parent",
                "parent-turn",
                150,
                Some(50),
            )),
            Change::upsert(turn_record(
                &parent,
                "parent",
                "parent-turn",
                TurnStatus::Completed,
                (1_700_000_000, Some(1_700_000_010), Some(10_000)),
                None,
            )),
            Change::upsert(session_record("child", "Child", &child, 150)),
            Change::upsert(turn_record(
                &child,
                "child",
                "child-turn",
                TurnStatus::InProgress,
                (1_700_000_020, None, None),
                None,
            )),
            Change::upsert(usage_record(&child, "child", "child-turn", 100, None)),
            Change::upsert(usage_record(&child, "child", "child-turn", 150, Some(50))),
            Change::upsert(turn_record(
                &child,
                "child",
                "child-turn",
                TurnStatus::Completed,
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
            "SELECT total_tokens FROM session_turns WHERE id = ?1",
            [turn_id(&child, "child-turn").as_str()],
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
fn source_scoped_turn_ids_keep_provider_reused_ids_separate() {
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
            Change::upsert(turn_record(
                transcript,
                session,
                "shared-turn-id",
                TurnStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(turn_record(
                transcript,
                session,
                "shared-turn-id",
                TurnStatus::Completed,
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
    assert_eq!(snapshot.summary.succeeded_turn_count, 2);
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .map(|session| session.turn_count)
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
    let running = turn_record(
        &transcript,
        "session-1",
        "turn-1",
        TurnStatus::InProgress,
        (1_700_000_000, None, None),
        None,
    );
    let mut terminal = turn_record(
        &transcript,
        "session-1",
        "turn-1",
        TurnStatus::Completed,
        (1_700_000_000, Some(1_700_000_010), None),
        None,
    );
    let RecordData::Turn(turn) = &mut terminal.data else {
        unreachable!();
    };
    turn.started_at = None;

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
    let current_turn_id: Option<String> = database
        .connect()
        .unwrap()
        .query_row(
            "SELECT current_turn_id FROM rollout_sources WHERE path = ?1",
            [transcript.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(current_turn_id.is_none());

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
            Change::upsert(turn_record(
                &transcript,
                "session-1",
                "turn-1",
                TurnStatus::InProgress,
                (1_700_000_000, None, None),
                None,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "turn-1",
                "call-1",
                json!({"cmd": "sed -n '1,120p' /skills/example/SKILL.md"}),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "turn-1",
                "call-1",
                json!({"output": "---\nname: example\ndescription: Example\n---\n"}),
                true,
            )),
            Change::upsert(turn_record(
                &transcript,
                "session-1",
                "turn-1",
                TurnStatus::Interrupted,
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

    assert_eq!(snapshot.summary.cancelled_turn_count, 1);
    assert_eq!(snapshot.sessions[0].status, "cancelled");
    assert_eq!(snapshot.skills[0].cancelled_count, 1);

    fs::remove_dir_all(database_path.parent().unwrap()).unwrap();
}

#[test]
fn orphan_turn_and_usage_records_do_not_abort_the_batch() {
    let database_path = test_database_path();
    let database = Database::initialize(&database_path).unwrap();
    let transcript = database_path.parent().unwrap().join("orphan.jsonl");
    let mut running = turn_record(
        &transcript,
        "missing-session",
        "turn-1",
        TurnStatus::InProgress,
        (1_700_000_000, None, None),
        None,
    );
    running.session = None;
    let mut usage = usage_record(&transcript, "missing-session", "turn-1", 100, Some(100));
    usage.session = None;
    let mut terminal = turn_record(
        &transcript,
        "missing-session",
        "turn-1",
        TurnStatus::Completed,
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
                "Orphan Skill turn",
                &transcript,
                10,
            )),
            Change::upsert(tool_call_record(
                &transcript,
                "session-1",
                "missing-turn",
                "call-1",
                json!({"cmd": "sed -n '1,120p' /skills/example/SKILL.md"}),
            )),
            Change::upsert(tool_result_record(
                &transcript,
                "session-1",
                "missing-turn",
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
