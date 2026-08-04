#![cfg(feature = "codex")]

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(feature = "codex-watch")]
use std::time::Duration;

#[cfg(feature = "codex-watch")]
use coding_agent_data::providers::codex::CodexWatchOptions;
use coding_agent_data::providers::codex::{CodexProvider, CodexSource};
#[cfg(feature = "codex-watch")]
use coding_agent_data::WatchProvider;
use coding_agent_data::{
    Actor, AdapterCoverage, AgentInvocationStatus, AgentOperation, ApprovalPolicy, Change,
    ContentAudience, ContentBlock, ContentIconTheme, ContentPriority, DataQuality, FileChangeKind,
    GoalStatus, HistoryMode, ItemData, MessageRole, ModeChangeKind, NoticeLevel, PlanStepStatus,
    Provider, RateLimitReason, RateLimitScope, ReasoningVisibility, Record, RecordData, RecordId,
    SandboxPolicy, SessionRelationKind, SourceCoverage, StopReason, TokenUsage, ToolKind,
    ToolStatus, TurnStatus,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    source: CodexSource,
    database_path: std::path::PathBuf,
    rollout_path: std::path::PathBuf,
}

impl Fixture {
    fn new(rollout_contents: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let codex_home = directory.path().join(".codex");
        let rollout_directory = codex_home.join("sessions/2026/01/02");
        fs::create_dir_all(&rollout_directory).unwrap();
        let rollout_path = rollout_directory.join("rollout-thread-1.jsonl");
        fs::write(&rollout_path, rollout_contents).unwrap();

        let database_path = codex_home.join("state_5.sqlite");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    rollout_path TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    title TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    tokens_used INTEGER NOT NULL,
                    archived INTEGER NOT NULL,
                    model TEXT,
                    model_provider TEXT,
                    cli_version TEXT,
                    agent_nickname TEXT,
                    agent_role TEXT,
                    git_branch TEXT,
                    git_sha TEXT,
                    git_origin_url TEXT,
                    sandbox_policy TEXT,
                    approval_mode TEXT,
                    reasoning_effort TEXT
                );
                ",
            )
            .unwrap();
        connection
            .execute(
                "
                INSERT INTO threads (
                    id, rollout_path, created_at_ms, updated_at_ms, title, cwd,
                    tokens_used, archived, model, model_provider, cli_version,
                    agent_nickname, agent_role, git_branch, git_sha, git_origin_url,
                    sandbox_policy, approval_mode, reasoning_effort
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18, ?19
                )
                ",
                params![
                    "thread-1",
                    rollout_path.to_string_lossy(),
                    1_700_000_000_000_i64,
                    1_700_000_001_000_i64,
                    "Initial title",
                    "/workspace/project",
                    42_i64,
                    0_i64,
                    "gpt-test",
                    "openai",
                    "0.144.6",
                    "reviewer",
                    "worker",
                    "main",
                    "abc123",
                    "https://example.com/repository.git",
                    "workspace-write",
                    "on-request",
                    "high"
                ],
            )
            .unwrap();
        drop(connection);

        let source = CodexSource::new(&codex_home, &codex_home);
        let rollout_path = source
            .codex_home()
            .join("sessions/2026/01/02/rollout-thread-1.jsonl");
        Self {
            source,
            _directory: directory,
            database_path,
            rollout_path,
        }
    }

    fn provider(&self) -> CodexProvider {
        CodexProvider::new(self.source.clone())
    }
}

fn session_meta() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\",\"timestamp\":\"2026-01-02T03:04:05Z\"}}\n"
}

fn turn_started() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-1\",\"started_at\":1704164646}}\n"
}

fn token_count() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":100,\"input_tokens\":70,\"cache_write_input_tokens\":10,\"output_tokens\":30},\"last_token_usage\":{\"total_tokens\":25,\"input_tokens\":20,\"output_tokens\":5}}}}\n"
}

fn rate_limit_only() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":null,\"rate_limits\":{\"limit_id\":\"codex\",\"limit_name\":\"Codex\",\"primary\":{\"used_percent\":12.5,\"window_minutes\":300,\"resets_at\":1767324000},\"secondary\":{\"used_percent\":40.0,\"window_minutes\":10080,\"resets_at\":1767928800},\"credits\":{\"has_credits\":true,\"unlimited\":false,\"balance\":\"4.25\"},\"individual_limit\":{\"limit\":\"100\",\"used\":\"40\",\"remaining_percent\":60,\"resets_at\":1767928800},\"spend_control_reached\":true,\"plan_type\":\"business\",\"rate_limit_reached_type\":\"workspace_member_usage_limit_reached\"}}}\n"
}

fn token_count_values(timestamp: &str, cumulative: i64, delta: i64) -> String {
    format!(
        "{{\"timestamp\":\"{timestamp}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":{cumulative},\"input_tokens\":{cumulative}}},\"last_token_usage\":{{\"total_tokens\":{delta},\"input_tokens\":{delta}}}}}}}}}\n"
    )
}

fn cumulative_token_count(timestamp: &str, cumulative: i64) -> String {
    format!(
        "{{\"timestamp\":\"{timestamp}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":{cumulative},\"input_tokens\":{cumulative},\"cached_input_tokens\":0,\"output_tokens\":0,\"reasoning_output_tokens\":0}}}}}}}}\n"
    )
}

fn estimated_token_count(timestamp: &str, cumulative: i64, estimate: i64) -> String {
    format!(
        "{{\"timestamp\":\"{timestamp}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":{cumulative},\"input_tokens\":{cumulative},\"cached_input_tokens\":0,\"output_tokens\":0,\"reasoning_output_tokens\":0}},\"last_token_usage\":{{\"total_tokens\":{estimate},\"input_tokens\":0,\"cached_input_tokens\":0,\"output_tokens\":0,\"reasoning_output_tokens\":0}}}}}}}}\n"
    )
}

fn ordinal_token_count_values(
    timestamp: &str,
    ordinal: u64,
    cumulative: i64,
    delta: i64,
) -> String {
    format!(
        "{{\"timestamp\":\"{timestamp}\",\"ordinal\":{ordinal},\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"total_tokens\":{cumulative},\"input_tokens\":{cumulative}}},\"last_token_usage\":{{\"total_tokens\":{delta},\"input_tokens\":{delta}}}}}}}}}\n"
    )
}

fn insert_thread(fixture: &Fixture, id: &str, rollout_path: &std::path::Path, tokens: i64) {
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "
            INSERT INTO threads (
                id, rollout_path, created_at_ms, updated_at_ms, title, cwd,
                tokens_used, archived, model, git_branch
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                id,
                rollout_path.to_string_lossy(),
                1_704_164_700_000_i64,
                1_704_164_701_000_i64,
                id,
                "/workspace/project",
                tokens,
                0_i64,
                "gpt-test",
                "main"
            ],
        )
        .unwrap();
}

fn turn_completed() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn-1\",\"completed_at\":1704164648}}\n"
}

fn tool_call() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"read_file\",\"call_id\":\"call-1\",\"arguments\":\"{\\\"path\\\":\\\"SKILL.md\\\"}\"}}\n"
}

fn tool_result() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:09Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"call-1\",\"output\":\"file body\"}}\n"
}

fn message() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:10Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"message-1\",\"role\":\"assistant\",\"phase\":\"final_answer\",\"content\":[{\"type\":\"output_text\",\"text\":\"done\"}]}}\n"
}

fn acp_v2_content_message() -> &'static str {
    concat!(
        r#"{"timestamp":"2026-01-02T03:04:10Z","type":"response_item","payload":{"type":"message","id":"message-v2","role":"assistant","content":[{"type":"output_text","text":"annotated","annotations":{"audience":["user","_reviewer"],"lastModified":"2026-01-02T03:04:05Z","priority":0.8}},{"type":"resource_link","uri":"file:///workspace/report.pdf","name":"report.pdf","icons":[{"src":"https://example.com/pdf-dark.png","mimeType":"image/png","sizes":["48x48"],"theme":"dark"}]},{"type":"_chart","series":[1,2,3]},{"series":[4,5,6]},{"type":"output_text","text":"invalid priority","annotations":{"priority":1.2}}]}}"#,
        "\n"
    )
}

fn reasoning() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:11Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"id\":\"reasoning-1\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"checked the files\"}],\"content\":[{\"type\":\"reasoning_text\",\"text\":\"visible reasoning\"}]}}\n"
}

fn compacted() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:12Z\",\"type\":\"compacted\",\"payload\":{\"message\":\"replacement summary\",\"window_id\":\"window-2\"}}\n"
}

fn compacted_with_camel_case_fields() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:12Z\",\"type\":\"compacted\",\"payload\":{\"summary\":\"replacement summary\",\"automatic\":true,\"tokensBefore\":120000,\"tokensAfter\":48000,\"replacementHistory\":[{\"type\":\"message\",\"role\":\"user\",\"content\":[]}],\"windowNumber\":2,\"firstWindowId\":\"window-1\",\"previousWindowId\":\"window-1\",\"windowId\":\"window-2\"}}\n"
}

fn hosted_tool_items() -> &'static str {
    "{\"timestamp\":\"2026-01-02T03:04:13Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"tool_search_call\",\"id\":\"search-item\",\"call_id\":\"search-1\",\"status\":\"in_progress\",\"arguments\":{\"query\":\"formatter\"}}}\n{\"timestamp\":\"2026-01-02T03:04:14Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"tool_search_output\",\"id\":\"search-output\",\"call_id\":\"search-1\",\"status\":\"completed\",\"execution\":\"search\",\"tools\":[{\"name\":\"formatter\"}]}}\n{\"timestamp\":\"2026-01-02T03:04:15Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"image_generation_call\",\"id\":\"image-1\",\"status\":\"completed\",\"revised_prompt\":\"diagram\",\"result\":\"encoded-image\"}}\n"
}

fn lifecycle_events() -> &'static str {
    concat!(
        "{\"timestamp\":\"2026-01-02T03:04:13Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"patch_apply_end\",\"call_id\":\"patch-1\",\"turn_id\":\"turn-1\",\"status\":\"success\",\"success\":true,\"stdout\":\"Done!\",\"stderr\":\"\",\"changes\":{\"src/new.rs\":{\"type\":\"add\",\"content\":\"fn main() {}\"},\"src/old.rs\":{\"type\":\"update\",\"move_path\":\"src/moved.rs\",\"unified_diff\":\"@@ -1 +1 @@\"}}}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:14Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"mcp_tool_call_end\",\"call_id\":\"mcp-1\",\"invocation\":{\"server\":\"docs\",\"tool\":\"search\",\"arguments\":{\"query\":\"api\"}},\"duration\":{\"secs\":1,\"nanos\":500000000},\"result\":{\"Ok\":{\"content\":[{\"type\":\"text\",\"text\":\"found\"}],\"is_error\":false}}}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:15Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"sub_agent_activity\",\"event_id\":\"agent-event-1\",\"agent_thread_id\":\"child-thread\",\"agent_path\":\"reviewer\",\"kind\":\"started\",\"occurred_at_ms\":1767323055000}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:16Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"plan_update\",\"explanation\":\"Implementation plan\",\"plan\":[{\"step\":\"Inspect\",\"status\":\"completed\"},{\"step\":\"Fix\",\"status\":\"in_progress\"},{\"step\":\"Abandon\",\"status\":\"cancelled\"}]}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:17Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"context_compacted\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:18Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_rolled_back\",\"num_turns\":2}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:19Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"future_response_item\",\"id\":\"future-1\",\"value\":{\"new\":true}}}\n"
    )
}

fn terminal_tool_events() -> &'static str {
    concat!(
        "{\"timestamp\":\"2026-01-02T03:04:13Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"exec_command_end\",\"call_id\":\"exec-1\",\"turn_id\":\"turn-1\",\"command\":[\"pwd\"],\"cwd\":\"/workspace/project\",\"source\":\"agent\",\"stdout\":\"/workspace/project\\n\",\"stderr\":\"\",\"aggregated_output\":\"/workspace/project\\n\",\"formatted_output\":\"/workspace/project\\n\",\"exit_code\":0,\"duration\":{\"secs\":1,\"nanos\":250000000},\"status\":\"completed\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:13.500Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"exec-1\",\"output\":\"duplicate projection\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:14Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"web_search_end\",\"call_id\":\"web-1\",\"query\":\"Rust modules\",\"action\":{\"type\":\"search\",\"query\":\"Rust modules\"},\"results\":[{\"title\":\"Rust\"}]}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:15Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"image_generation_end\",\"call_id\":\"image-2\",\"status\":\"completed\",\"revised_prompt\":\"diagram\",\"result\":\"encoded-image\",\"saved_path\":\"/tmp/diagram.png\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:15.500Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"image_generation_call\",\"id\":\"image-2\",\"status\":\"completed\",\"result\":\"duplicate image projection\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:16Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"view_image_tool_call\",\"call_id\":\"view-1\",\"path\":\"/tmp/diagram.png\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:17Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"agent_message\",\"id\":\"agent-message-1\",\"author\":\"reviewer\",\"recipient\":\"root\",\"content\":[{\"type\":\"input_text\",\"text\":\"review complete\"}],\"internal_chat_message_metadata_passthrough\":{\"turn_id\":\"turn-1\"}}}\n"
    )
}

fn apply_changes(records: &mut HashMap<RecordId, Record>, changes: &[Change]) {
    for change in changes {
        match change {
            Change::Upsert(record) => {
                records.insert(record.id.clone(), (**record).clone());
            }
            Change::Delete(id) => {
                records.remove(id);
            }
            Change::Reset(origin) | Change::Remove(origin) => {
                records.retain(|_, record| {
                    record.origin.source != origin.source || record.origin.path != origin.path
                });
            }
            _ => {}
        }
    }
}

fn usage_deltas_for_session(changes: &[Change], external_id: &str) -> Vec<Option<i64>> {
    let suffix = format!(":session:{external_id}");
    changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record)
                if record
                    .session
                    .as_ref()
                    .is_some_and(|id| id.as_str().ends_with(&suffix)) =>
            {
                match &record.data {
                    RecordData::Usage(usage) => Some(usage.delta.as_ref().map(|usage| usage.total)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

fn usage_deltas_in_records(
    records: &HashMap<RecordId, Record>,
    external_id: &str,
) -> Vec<Option<i64>> {
    let suffix = format!(":session:{external_id}");
    let mut usage = records
        .values()
        .filter_map(|record| {
            if !record
                .session
                .as_ref()
                .is_some_and(|id| id.as_str().ends_with(&suffix))
            {
                return None;
            }
            match &record.data {
                RecordData::Usage(usage) => Some((
                    record.origin.path.clone(),
                    record.timestamp,
                    usage.delta.as_ref().map(|delta| delta.total),
                )),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    usage.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    usage.into_iter().map(|(_, _, delta)| delta).collect()
}

fn scan_into_records(
    provider: &CodexProvider,
    mut checkpoint: Option<coding_agent_data::Checkpoint>,
    records: &mut HashMap<RecordId, Record>,
) -> coding_agent_data::Checkpoint {
    for _ in 0..16 {
        let batch = provider.scan(checkpoint.as_ref()).unwrap();
        apply_changes(records, &batch.changes);
        checkpoint = Some(batch.checkpoint);
        if !batch.has_more {
            return checkpoint.unwrap();
        }
    }
    panic!("Codex fixture did not converge within 16 batches");
}

fn assert_incremental_usage_matches_fresh(
    initial_events: &str,
    appended_events: &str,
    expected_deltas: &[Option<i64>],
) {
    let fixture = Fixture::new(&format!("{}{initial_events}", session_meta()));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let mut incremental_records = HashMap::new();
    apply_changes(&mut incremental_records, &first.changes);

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(appended_events.as_bytes())
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    apply_changes(&mut incremental_records, &second.changes);

    let fresh = provider.scan(None).unwrap();
    let mut fresh_records = HashMap::new();
    apply_changes(&mut fresh_records, &fresh.changes);

    assert_eq!(incremental_records, fresh_records);
    let deltas = usage_deltas_for_session(&fresh.changes, "thread-1");
    assert_eq!(deltas, expected_deltas);
    assert_eq!(
        deltas.into_iter().flatten().sum::<i64>(),
        expected_deltas.iter().flatten().sum::<i64>()
    );
}

#[test]
fn scan_normalizes_sessions_and_turns_and_waits_for_incomplete_json() {
    let fixture = Fixture::new(&format!(
        "{}{}{}{}{}",
        session_meta(),
        turn_started(),
        token_count(),
        tool_call(),
        "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"future_event\""
    ));
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "thread-1"
                            && session.title.as_deref() == Some("Initial title")
                            && session.total_tokens == Some(42)
                            && session.model_provider.as_deref() == Some("openai")
                            && session.agent_version.as_deref() == Some("0.144.6")
                            && session.agent_name.as_deref() == Some("reviewer")
                            && session.agent_role.as_deref() == Some("worker")
                            && session.git_commit.as_deref() == Some("abc123")
                            && session.git_remote_url.as_deref()
                                == Some("https://example.com/repository.git")
                            && session.provider_attributes["codex.sandbox_policy"]
                                == "workspace-write"
                            && session.provider_attributes["codex.approval_mode"]
                                == "on-request"
                            && session.provider_attributes["codex.reasoning_effort"]
                                == "high"
                )
        )
    }));
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolCall(call)
                                if call.call_id == "call-1"
                                    && call.input["path"] == "SKILL.md"
                                    && call.locations.iter().any(|location|
                                        location.path
                                            == std::path::Path::new("SKILL.md"))
                        )
                )
        )
    }));
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Usage(usage)
                        if usage.cumulative.as_ref().map(|usage| usage.total) == Some(100)
                            && usage
                                .cumulative
                                .as_ref()
                                .and_then(|usage| usage.cache_creation_input)
                                == Some(10)
                            && usage.delta.as_ref().map(|usage| usage.total) == Some(25)
                )
        )
    }));
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Turn(turn)
                        if turn.external_id.as_deref() == Some("turn-1")
                            && turn.status == TurnStatus::InProgress
                )
        )
    }));
    assert!(!first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record
                    .original
                    .as_ref()
                    .is_some_and(|original| original.value["type"] == "future_event")
        )
    }));

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(b"}\n")
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if unknown.kind.as_deref() == Some("future_event")
                )
        )
    }));

    let third = provider.scan(Some(&second.checkpoint)).unwrap();
    assert!(third.changes.is_empty());
    assert!(!third.has_more);
}

#[test]
fn plain_rollouts_without_a_trailing_newline_are_normalized_once() {
    let mut final_message = message().to_owned();
    final_message.pop();
    let fixture = Fixture::new(&format!("{}{final_message}", session_meta()));
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Message(message)
                                if message.content == vec![ContentBlock::text("done")]
                        )
                )
        )
    }));
    assert!(provider
        .scan(Some(&first.checkpoint))
        .unwrap()
        .changes
        .is_empty());
}

#[test]
fn token_count_without_tokens_still_normalizes_rate_limits() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), rate_limit_only()));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if unknown.kind.as_deref() == Some("token_count")
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::RateLimit(limit)
                        if limit.external_id.as_deref() == Some("codex")
                            && limit.windows.len() == 2
                            && limit.windows[0].used_percent == Some(12.5)
                            && limit.credits.as_ref()
                                .and_then(|credits| credits.balance.as_deref())
                                == Some("4.25")
                            && limit.spend_limit.as_ref()
                                .and_then(|limit| limit.remaining_percent)
                                == Some(60)
                            && limit.reached_reason
                                == Some(RateLimitReason::UsageLimit)
                            && limit.reached_scope
                                == Some(RateLimitScope::WorkspaceMember)
                )
        )
    }));
}

#[test]
fn rollout_session_metadata_enriches_the_index_without_clobbering_it() {
    let fixture = Fixture::new(concat!(
        "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"cli_version\":\"1.2.3\",\"model_provider\":\"openai-fallback\",\"agent_nickname\":\"atlas\",\"agent_role\":\"reviewer\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_name_updated\",\"thread_id\":\"thread-1\",\"thread_name\":\"Renamed thread\"}}\n"
    ));
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE threads SET cli_version = NULL, model_provider = NULL, agent_nickname = NULL, agent_role = NULL WHERE id = 'thread-1'",
            [],
        )
        .unwrap();

    let batch = fixture.provider().scan(None).unwrap();
    let mut records = HashMap::new();
    apply_changes(&mut records, &batch.changes);
    let session = records
        .values()
        .find_map(|record| match &record.data {
            RecordData::Session(session) if session.external_id == "thread-1" => Some(session),
            _ => None,
        })
        .expect("merged session");

    assert_eq!(session.title.as_deref(), Some("Renamed thread"));
    assert_eq!(session.total_tokens, Some(42));
    assert_eq!(session.model.as_deref(), Some("gpt-test"));
    assert_eq!(session.model_provider.as_deref(), Some("openai-fallback"));
    assert_eq!(session.agent_version.as_deref(), Some("1.2.3"));
    assert_eq!(session.agent_name.as_deref(), Some("atlas"));
    assert_eq!(session.agent_role.as_deref(), Some("reviewer"));
    assert_eq!(session.quality, DataQuality::Complete);
    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if matches!(
                            unknown.kind.as_deref(),
                            Some("session_meta" | "thread_name_updated")
                        )
                )
        )
    }));
}

#[test]
fn incremental_session_updates_rehydrate_the_nonserialized_merged_snapshot() {
    let fixture = Fixture::new(
        "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\",\"model_provider\":\"rollout-provider\",\"base_instructions\":\"provider instructions\"}}\n",
    );
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let checkpoint: serde_json::Value = first.checkpoint.decode_state(provider.info()).unwrap();

    assert!(checkpoint["rollouts"]
        .as_object()
        .unwrap()
        .values()
        .all(|rollout| rollout["context"].get("session_snapshot").is_none()));
    assert_eq!(
        checkpoint["threads"]["thread-1"]["session"]["provider_attributes"]
            .get("codex.base_instructions"),
        None
    );

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(
            b"{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_name_updated\",\"thread_id\":\"thread-1\",\"thread_name\":\"Renamed later\"}}\n",
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    let session = second
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Session(session) if session.external_id == "thread-1" => Some(session),
                _ => None,
            },
            _ => None,
        })
        .expect("updated session");

    assert_eq!(session.title.as_deref(), Some("Renamed later"));
    assert_eq!(session.total_tokens, Some(42));
    assert_eq!(session.model.as_deref(), Some("gpt-test"));
    assert_eq!(session.model_provider.as_deref(), Some("openai"));
    assert_eq!(
        session
            .provider_attributes
            .get("codex.base_instructions")
            .and_then(serde_json::Value::as_str),
        Some("provider instructions")
    );
    assert_eq!(session.quality, DataQuality::Complete);
}

#[test]
fn inherited_session_metadata_does_not_become_an_unknown_child_record() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        "{\"timestamp\":\"2026-01-01T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent-thread\",\"cwd\":\"/workspace/parent\"}}\n",
        session_meta()
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if unknown.kind.as_deref() == Some("session_meta")
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "thread-1"
                )
        )
    }));
}

#[test]
fn rollout_owner_is_resolved_without_the_sqlite_index() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        "{\"timestamp\":\"2026-01-01T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"parent-thread\",\"cwd\":\"/workspace/parent\"}}\n",
        session_meta()
    ));
    fs::remove_file(&fixture.database_path).unwrap();

    let batch = fixture.provider().scan(None).unwrap();

    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "parent-thread"
                ) || matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if unknown.kind.as_deref() == Some("session_meta")
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "thread-1"
                )
        )
    }));
}

#[test]
fn legacy_fork_owner_is_the_filename_session_when_copied_parent_metadata_follows_it() {
    let fixture = Fixture::new(session_meta());
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        concat!(
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:05:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"child-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"child\"}]}}\n"
        ),
    )
    .unwrap();
    fs::remove_file(&fixture.database_path).unwrap();

    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.origin.path == child_path
                    && matches!(
                        &record.data,
                        RecordData::Session(session) if session.external_id == "thread-2"
                    )
        )
    }));
    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.origin.path == child_path
                    && matches!(
                        &record.data,
                        RecordData::Unknown(unknown)
                            if unknown.kind.as_deref() == Some("session_meta")
                    )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.origin.path == child_path
                    && record
                        .session
                        .as_ref()
                        .is_some_and(|id| id.as_str().ends_with(":session:thread-2"))
                    && matches!(&record.data, RecordData::Item(_))
        )
    }));
}

#[test]
fn turn_context_and_encrypted_reasoning_are_normalized_without_losing_turn_links() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        concat!(
            "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"turn-1\",\"started_at\":1704164646,\"trace_id\":\"trace-1\",\"model_context_window\":200000}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.100Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"turn-1\",\"cwd\":\"/workspace/project\",\"workspace_roots\":[\"/workspace/project\"],\"current_date\":\"2026-01-02\",\"timezone\":\"UTC\",\"approval_policy\":\"on-request\",\"approvals_reviewer\":\"user\",\"sandbox_policy\":{\"type\":\"workspace-write\"},\"network\":{\"allowed_domains\":[\"example.com\"],\"denied_domains\":[]},\"file_system_sandbox_policy\":{\"writable_roots\":[\"/workspace/project\"]},\"model\":\"gpt-test\",\"model_provider\":\"openai\",\"model_context_window\":200000,\"effort\":\"high\",\"summary\":\"detailed\",\"collaboration_mode\":{\"mode\":\"default\"},\"multi_agent_version\":\"v2\",\"multi_agent_mode\":{\"mode\":\"default\"},\"realtime_active\":true,\"comp_hash\":\"comp-1\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"id\":\"reasoning-1\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"checked\"}],\"encrypted_content\":\"opaque\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn-1\",\"completed_at\":1704164648,\"time_to_first_token_ms\":250}}\n"
        ),
        "{\"timestamp\":\"2026-01-02T03:04:09Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"late-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"done\"}],\"internal_chat_message_metadata_passthrough\":{\"turn_id\":\"turn-1\"}}}\n"
    ));
    let batch = fixture.provider().scan(None).unwrap();
    let completed_turn = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Turn(turn) if turn.status == TurnStatus::Completed => {
                    Some((record.id.clone(), turn))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("completed turn");

    assert_eq!(completed_turn.1.trace_id.as_deref(), Some("trace-1"));
    assert_eq!(completed_turn.1.model_context_window, Some(200_000));
    assert_eq!(completed_turn.1.time_to_first_token_ms, Some(250));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.turn.as_ref() == Some(&completed_turn.0)
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if matches!(
                                &item.data,
                                ItemData::ExecutionContext(context)
                                    if context.cwd.as_deref()
                                        == Some(std::path::Path::new("/workspace/project"))
                                        && context.workspace_roots
                                            == [std::path::PathBuf::from("/workspace/project")]
                                        && context.approval_policy
                                            == Some(ApprovalPolicy::OnRequest)
                                        && context.sandbox_policy
                                            == Some(SandboxPolicy::WorkspaceWrite)
                                        && context.reasoning_effort.as_deref() == Some("high")
                            )
                    )
        )
    }));
    let execution_context = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Item(item) => match &item.data {
                    ItemData::ExecutionContext(context) => Some(context),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .expect("turn execution context");
    assert_eq!(
        execution_context.reasoning_summary.as_deref(),
        Some("detailed")
    );
    assert_eq!(
        execution_context.approvals_reviewer.as_deref(),
        Some("user")
    );
    assert_eq!(
        execution_context.provider_attributes["codex.network"]["allowed_domains"][0],
        "example.com"
    );
    assert_eq!(
        execution_context.provider_attributes["codex.file_system_sandbox_policy"]["writable_roots"]
            [0],
        "/workspace/project"
    );
    assert_eq!(
        execution_context.provider_attributes["codex.multi_agent_version"],
        "v2"
    );
    assert_eq!(
        execution_context.provider_attributes["codex.multi_agent_mode"]["mode"],
        "default"
    );
    assert_eq!(
        execution_context.provider_attributes["codex.realtime_active"],
        true
    );
    assert_eq!(
        execution_context.provider_attributes["codex.comp_hash"],
        "comp-1"
    );
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Reasoning(reasoning)
                                if reasoning.visibility == ReasoningVisibility::Encrypted
                                    && reasoning.content.is_empty()
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.turn.as_ref() == Some(&completed_turn.0)
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if matches!(
                                &item.data,
                                ItemData::Message(message)
                                    if message.content
                                        == vec![ContentBlock::text("done")]
                            )
                    )
        )
    }));
}

#[test]
fn thread_settings_applied_reads_the_nested_snapshot() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\",\"thread_settings\":{\"model\":\"gpt-5.6\",\"model_provider_id\":\"openai\",\"service_tier\":\"fast\",\"approval_policy\":\"never\",\"approvals_reviewer\":\"auto_review\",\"permission_profile\":{\"managed\":{\"network\":{\"enabled\":true}}},\"active_permission_profile\":{\"id\":\"workspace-write\"},\"cwd\":\"/workspace/nested\",\"reasoning_effort\":\"high\",\"reasoning_summary\":\"detailed\",\"personality\":\"friendly\",\"collaboration_mode\":{\"mode\":\"default\"}}}}\n"
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let execution_context = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Item(item) => match &item.data {
                    ItemData::ExecutionContext(context) => Some(context),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .expect("nested thread settings execution context");

    assert_eq!(
        execution_context.cwd.as_deref(),
        Some(std::path::Path::new("/workspace/nested"))
    );
    assert_eq!(execution_context.model.as_deref(), Some("gpt-5.6"));
    assert_eq!(execution_context.model_provider.as_deref(), Some("openai"));
    assert_eq!(execution_context.service_tier.as_deref(), Some("fast"));
    assert_eq!(
        execution_context.approval_policy,
        Some(ApprovalPolicy::Never)
    );
    assert_eq!(
        execution_context.approvals_reviewer.as_deref(),
        Some("auto_review")
    );
    assert_eq!(execution_context.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(
        execution_context.reasoning_summary.as_deref(),
        Some("detailed")
    );
    assert_eq!(execution_context.personality.as_deref(), Some("friendly"));
    assert_eq!(
        execution_context.collaboration_mode.as_deref(),
        Some("default")
    );
    assert_eq!(
        execution_context
            .permission_profile
            .as_ref()
            .and_then(|profile| profile.get("managed"))
            .and_then(|managed| managed.pointer("/network/enabled"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        execution_context
            .active_permission_profile
            .as_ref()
            .and_then(|profile| profile.get("id"))
            .and_then(serde_json::Value::as_str),
        Some("workspace-write")
    );
}

#[test]
fn paginated_turn_items_are_normalized_from_structured_lifecycle_records() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        turn_started(),
        concat!(
            "{\"timestamp\":\"2026-01-02T03:04:06.050Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046050,\"item\":{\"type\":\"UserMessage\",\"id\":\"user-1\",\"content\":[{\"type\":\"text\",\"text\":\"use the skill\",\"text_elements\":[]},{\"type\":\"skill\",\"name\":\"octocode-research\",\"path\":\"/skills/octocode-research/SKILL.md\"}]}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.100Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046100,\"item\":{\"type\":\"Plan\",\"id\":\"plan-1\",\"text\":\"Inspect and update\"}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.200Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"started_at_ms\":1767323046000,\"completed_at_ms\":1767323046200,\"item\":{\"type\":\"CommandExecution\",\"id\":\"command-1\",\"command\":[\"cargo\",\"check\"],\"cwd\":\"/workspace/project\",\"parsed_cmd\":[],\"source\":\"agent\",\"status\":\"completed\",\"stdout\":\"ok\",\"exit_code\":0,\"duration\":{\"secs\":1,\"nanos\":500000000}}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.300Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046300,\"item\":{\"type\":\"McpToolCall\",\"id\":\"mcp-1\",\"server\":\"filesystem\",\"tool\":\"edit_file\",\"arguments\":{\"file_path\":\"src/lib.rs\"},\"status\":\"completed\",\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"updated\"}],\"isError\":false}}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.400Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046400,\"item\":{\"type\":\"FileChange\",\"id\":\"patch-1\",\"changes\":{\"src/new.rs\":{\"type\":\"add\",\"content\":\"new\"}},\"status\":\"completed\",\"stdout\":\"Done!\"}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.500Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046500,\"item\":{\"type\":\"CollabAgentToolCall\",\"id\":\"agent-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"thread-1\",\"receiver_thread_ids\":[\"thread-2\"],\"prompt\":\"review\",\"model\":\"gpt-test\",\"agents_states\":{}}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.600Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046600,\"item\":{\"type\":\"EnteredReviewMode\",\"id\":\"review-1\",\"target\":{\"type\":\"uncommittedChanges\"},\"user_facing_hint\":\"Review requested\"}}}\n"
        )
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Message(message)
                                if message.content.iter().any(|content|
                                    matches!(
                                        content,
                                        ContentBlock::ResourceLink { uri, name, .. }
                                            if uri == "/skills/octocode-research/SKILL.md"
                                                && name.as_deref()
                                                    == Some("octocode-research")
                                    ))
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.turn.is_some()
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if matches!(
                                &item.data,
                                ItemData::Plan(plan)
                                    if plan.text.as_deref() == Some("Inspect and update")
                            )
                    )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "command-1"
                                    && result.status == ToolStatus::Completed
                                    && result.duration_ms == Some(1_500)
                                    && result.content == vec![ContentBlock::text("ok")]
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolCall(call)
                                if call.call_id == "mcp-1"
                                    && call.namespace.as_deref() == Some("filesystem")
                                    && call.name == "edit_file"
                                    && call.locations.iter().any(|location|
                                        location.path
                                            == std::path::Path::new("src/lib.rs"))
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::FileChange(change)
                                if change.path
                                    == std::path::Path::new("src/new.rs")
                                    && change.kind == FileChangeKind::Create
                                    && change.status == ToolStatus::Completed
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::AgentInvocation(invocation)
                                if invocation.operation == AgentOperation::Spawn
                                    && invocation.status
                                        == AgentInvocationStatus::Completed
                                    && invocation.receiver_ids == ["thread-2"]
                                    && invocation.child_session.is_some()
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ModeChange(change)
                                if change.mode == "review"
                                    && change.kind == ModeChangeKind::Entered
                        )
                )
        )
    }));
}

#[test]
fn paginated_history_preserves_lineage_and_logical_ordinals() {
    let fixture = Fixture::new(concat!(
        "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"ordinal\":5,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"history_mode\":\"paginated\",\"history_base\":{\"thread_id\":\"parent-thread\",\"end_ordinal_exclusive\":5,\"end_byte_offset\":2048},\"subagent_history_start_ordinal\":6,\"context_window\":{\"window_id\":\"window-1\"}}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"ordinal\":6,\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046000,\"item\":{\"type\":\"UserMessage\",\"id\":\"user-1\",\"content\":[{\"type\":\"text\",\"text\":\"hello\",\"text_elements\":[]}]}}}\n"
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let mut records = HashMap::new();
    apply_changes(&mut records, &batch.changes);
    let session = records
        .values()
        .find_map(|record| match &record.data {
            RecordData::Session(session) if session.external_id == "thread-1" => Some(session),
            _ => None,
        })
        .expect("paginated session");
    let history = session.history.as_ref().expect("session history");
    assert_eq!(history.mode, HistoryMode::Paginated);
    assert_eq!(history.own_start_ordinal, Some(6));
    assert_eq!(history.context_window_id.as_deref(), Some("window-1"));
    let base = history.base.as_ref().expect("history base");
    assert_eq!(base.end_ordinal_exclusive, 5);
    assert_eq!(base.end_byte_offset, 2048);
    assert!(history.lineage.is_empty());
    assert!(batch
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.lineage.parent_missing"));
    assert!(session.relations.iter().any(|relation| {
        relation.kind == SessionRelationKind::Fork
            && relation
                .session
                .as_str()
                .ends_with(":session:parent-thread")
    }));

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if item.sequence.position == 2
                            && item.sequence.logical_ordinal == Some(6)
                            && matches!(&item.data, ItemData::Message(_))
                )
        )
    }));
}

#[test]
fn paginated_lineage_resolves_all_physical_segments_and_filters_subagent_context() {
    let fixture = Fixture::new(concat!(
        "{\"timestamp\":\"2026-01-02T03:04:00Z\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\",\"history_mode\":\"paginated\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:01Z\",\"ordinal\":1,\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"root-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"root\"}]}}\n"
    ));
    let root_len = fs::metadata(&fixture.rollout_path).unwrap().len();
    let parent_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    let parent_meta = format!(
        "{{\"timestamp\":\"2026-01-02T03:05:00Z\",\"ordinal\":2,\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread-2\",\"cwd\":\"/workspace/project\",\"history_mode\":\"paginated\",\"history_base\":{{\"thread_id\":\"thread-1\",\"end_ordinal_exclusive\":2,\"end_byte_offset\":{root_len}}},\"subagent_history_start_ordinal\":3}}}}\n"
    );
    fs::write(
        &parent_path,
        format!(
            "{parent_meta}{}",
            "{\"timestamp\":\"2026-01-02T03:05:01Z\",\"ordinal\":3,\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"parent-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"parent\"}]}}\n"
        ),
    )
    .unwrap();
    let parent_len = fs::metadata(&parent_path).unwrap().len();
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-3.jsonl");
    let child_meta = format!(
        "{{\"timestamp\":\"2026-01-02T03:06:00Z\",\"ordinal\":4,\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread-3\",\"cwd\":\"/workspace/project\",\"history_mode\":\"paginated\",\"history_base\":{{\"thread_id\":\"thread-2\",\"end_ordinal_exclusive\":4,\"end_byte_offset\":{parent_len}}},\"subagent_history_start_ordinal\":5}}}}\n"
    );
    fs::write(
        &child_path,
        format!(
            "{child_meta}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:01Z\",\"ordinal\":3,\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"copied-parent-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"copied context\"}]}}\n",
            "{\"timestamp\":\"2026-01-02T03:06:01Z\",\"ordinal\":5,\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"child-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"child\"}]}}\n"
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &parent_path, 0);
    insert_thread(&fixture, "thread-3", &child_path, 0);

    let batch = fixture.provider().scan(None).unwrap();
    let mut records = HashMap::new();
    apply_changes(&mut records, &batch.changes);
    let session = records
        .values()
        .find_map(|record| match &record.data {
            RecordData::Session(session) if session.external_id == "thread-3" => Some(session),
            _ => None,
        })
        .expect("child session");
    let history = session.history.as_ref().expect("child history");
    assert_eq!(history.lineage.len(), 3);
    assert!(history.lineage[0]
        .session
        .as_str()
        .ends_with(":session:thread-1"));
    assert_eq!(history.lineage[0].start_ordinal, 1);
    assert_eq!(history.lineage[0].end_ordinal_exclusive, Some(2));
    assert!(history.lineage[1]
        .session
        .as_str()
        .ends_with(":session:thread-2"));
    assert_eq!(history.lineage[1].start_ordinal, 3);
    assert_eq!(history.lineage[1].end_ordinal_exclusive, Some(4));
    assert!(history.lineage[2]
        .session
        .as_str()
        .ends_with(":session:thread-3"));
    assert_eq!(history.lineage[2].start_ordinal, 5);
    assert_eq!(history.lineage[2].end_ordinal_exclusive, None);

    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.origin.path == child_path
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if item.sequence.logical_ordinal == Some(3)
                    )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.origin.path == child_path
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if item.sequence.logical_ordinal == Some(5)
                    )
        )
    }));
    assert!(!batch
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code.starts_with("codex.lineage.")));
}

#[test]
fn malformed_paginated_lineage_reports_cutoff_bounds_and_cycles() {
    let fixture = Fixture::new(
        "{\"timestamp\":\"2026-01-02T03:04:00Z\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\",\"history_mode\":\"paginated\"}}\n",
    );
    let root_len = fs::metadata(&fixture.rollout_path).unwrap().len();
    let directory = fixture.source.codex_home().join("sessions/2026/01/02");
    let zero_cutoff = directory.join("rollout-thread-2.jsonl");
    let out_of_bounds = directory.join("rollout-thread-3.jsonl");
    let cycle_a = directory.join("rollout-thread-4.jsonl");
    let cycle_b = directory.join("rollout-thread-5.jsonl");
    fs::write(
        &zero_cutoff,
        format!(
            "{{\"timestamp\":\"2026-01-02T03:05:00Z\",\"ordinal\":1,\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread-2\",\"history_mode\":\"paginated\",\"history_base\":{{\"thread_id\":\"thread-1\",\"end_ordinal_exclusive\":0,\"end_byte_offset\":{root_len}}}}}}}\n"
        ),
    )
    .unwrap();
    fs::write(
        &out_of_bounds,
        format!(
            "{{\"timestamp\":\"2026-01-02T03:06:00Z\",\"ordinal\":2,\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread-3\",\"history_mode\":\"paginated\",\"history_base\":{{\"thread_id\":\"thread-1\",\"end_ordinal_exclusive\":2,\"end_byte_offset\":{}}}}}}}\n",
            root_len.saturating_add(1_000)
        ),
    )
    .unwrap();
    fs::write(
        &cycle_a,
        "{\"timestamp\":\"2026-01-02T03:07:00Z\",\"ordinal\":2,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-4\",\"history_mode\":\"paginated\",\"history_base\":{\"thread_id\":\"thread-5\",\"end_ordinal_exclusive\":2,\"end_byte_offset\":1}}}\n",
    )
    .unwrap();
    fs::write(
        &cycle_b,
        "{\"timestamp\":\"2026-01-02T03:08:00Z\",\"ordinal\":2,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-5\",\"history_mode\":\"paginated\",\"history_base\":{\"thread_id\":\"thread-4\",\"end_ordinal_exclusive\":2,\"end_byte_offset\":1}}}\n",
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &zero_cutoff, 0);
    insert_thread(&fixture, "thread-3", &out_of_bounds, 0);
    insert_thread(&fixture, "thread-4", &cycle_a, 0);
    insert_thread(&fixture, "thread-5", &cycle_b, 0);

    let batch = fixture.provider().scan(None).unwrap();
    let codes = batch
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"codex.lineage.invalid_cutoff"));
    assert!(codes.contains(&"codex.lineage.byte_offset_out_of_bounds"));
    assert!(codes.contains(&"codex.lineage.cycle"));
}

#[test]
fn a_newly_available_lineage_parent_resets_and_enriches_the_child() {
    let parent_contents = concat!(
        "{\"timestamp\":\"2026-01-02T03:03:00Z\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{\"id\":\"parent-thread\",\"history_mode\":\"paginated\"}}\n",
        "{\"timestamp\":\"2026-01-02T03:03:01Z\",\"ordinal\":1,\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"parent-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"parent\"}]}}\n"
    );
    let fixture = Fixture::new(&format!(
        "{{\"timestamp\":\"2026-01-02T03:04:00Z\",\"ordinal\":2,\"type\":\"session_meta\",\"payload\":{{\"id\":\"thread-1\",\"history_mode\":\"paginated\",\"history_base\":{{\"thread_id\":\"parent-thread\",\"end_ordinal_exclusive\":2,\"end_byte_offset\":{}}}}}}}\n",
        parent_contents.len()
    ));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    assert!(first
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.lineage.parent_missing"));

    let parent_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-parent-thread.jsonl");
    fs::write(&parent_path, parent_contents).unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Reset(origin) if origin.path == fixture.rollout_path
        )
    }));
    assert!(second
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.lineage.changed"));
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "thread-1"
                            && session
                                .history
                                .as_ref()
                                .is_some_and(|history| history.lineage.len() == 2)
                )
        )
    }));
}

#[test]
fn durable_codex_state_goal_shell_extensions_and_usage_are_normalized() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        concat!(
            "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"world_state\",\"payload\":{\"full\":true,\"state\":{\"cwd\":\"/workspace/project\",\"branch\":\"main\"}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.100Z\",\"type\":\"inter_agent_communication_metadata\",\"payload\":{\"trigger_turn\":true}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.200Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_goal_updated\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"goal\":{\"thread_id\":\"thread-1\",\"objective\":\"finish coverage\",\"status\":\"active\",\"token_budget\":1000,\"tokens_used\":125,\"time_used_seconds\":9,\"created_at\":1767323040,\"updated_at\":1767323046}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.300Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"local_shell_call\",\"id\":\"shell-item\",\"call_id\":\"shell-1\",\"status\":\"completed\",\"action\":{\"type\":\"exec\",\"command\":[\"cargo\",\"check\"],\"timeout_ms\":30000,\"working_directory\":\"/workspace/project\",\"env\":{\"RUST_LOG\":\"info\"},\"user\":null}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.400Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046400,\"item\":{\"type\":\"Extension\",\"kind\":\"image_gen.generation\",\"id\":\"image-extension-1\",\"status\":\"completed\",\"revisedPrompt\":\"diagram\",\"result\":\"encoded-image\",\"savedPath\":\"/workspace/project/diagram.png\"}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.500Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"completed_at_ms\":1767323046500,\"item\":{\"type\":\"Extension\",\"kind\":\"web.search\",\"id\":\"web-extension-1\",\"query\":\"Rust releases\",\"action\":{\"type\":\"search\",\"query\":\"Rust releases\",\"queries\":null},\"results\":[{\"title\":\"Rust\",\"url\":\"https://www.rust-lang.org\"}]}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.600Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\",\"thread_settings\":{\"model\":\"gpt-5.6\",\"model_provider_id\":\"openai\",\"service_tier\":\"fast\",\"approval_policy\":\"never\",\"approvals_reviewer\":\"user\",\"permission_profile\":\"disabled\",\"cwd\":\"/workspace/project\",\"collaboration_mode\":{\"mode\":\"default\"}}}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.700Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":100,\"input_tokens\":70,\"output_tokens\":30},\"last_token_usage\":{\"total_tokens\":25,\"input_tokens\":20,\"output_tokens\":5}}}}\n"
        )
    ));

    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::WorldState(state)
                                if state.full
                                    && state.state["branch"] == "main"
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ForkTurnBoundary(boundary)
                                if boundary.trigger_turn
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.turn.is_some()
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if matches!(
                                &item.data,
                                ItemData::Goal(goal)
                                    if goal.objective == "finish coverage"
                                        && goal.status == GoalStatus::Active
                                        && goal.token_budget == Some(1000)
                                        && goal.tokens_used == 125
                            )
                    )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolCall(call)
                                if call.call_id == "shell-1"
                                    && call.name == "local_shell"
                                    && call.kind == ToolKind::Execute
                                    && call.status == ToolStatus::Completed
                                    && call.input["command"][0] == "cargo"
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "image-extension-1"
                                    && result.content.iter().any(|content|
                                        matches!(
                                            content,
                                            ContentBlock::Image { uri: Some(uri), .. }
                                                if uri == "/workspace/project/diagram.png"
                                        ))
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "web-extension-1"
                                    && result.output["results"][0]["title"] == "Rust"
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Usage(usage)
                        if usage.model.as_deref() == Some("gpt-5.6")
                            && usage.model_provider.as_deref() == Some("openai")
                            && usage.service_tier.as_deref() == Some("fast")
                            && usage.delta.as_ref().is_some_and(|usage| usage.total == 25)
                )
        )
    }));
    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if matches!(
                            unknown.kind.as_deref(),
                            Some(
                                "world_state"
                                    | "inter_agent_communication_metadata"
                                    | "thread_goal_updated"
                            )
                        )
                )
        )
    }));
}

#[test]
fn legacy_presentation_events_do_not_duplicate_canonical_response_items() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        concat!(
            "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"client_id\":\"client-1\",\"message\":\"legacy input\",\"images\":[],\"local_images\":[]}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.100Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"message-0\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"canonical input\"}]}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.200Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_reasoning\",\"text\":\"streamed summary\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06.300Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"id\":\"reasoning-1\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"canonical summary\"}]}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"message-1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"canonical answer\"}]}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:07.100Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"canonical answer\",\"phase\":\"final_answer\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"error\",\"message\":\"request failed\",\"codex_error_info\":\"internal_server_error\"}}\n"
        )
    ));
    let batch = fixture.provider().scan(None).unwrap();

    let messages = batch
        .changes
        .iter()
        .filter(|change| {
            matches!(
                change,
                Change::Upsert(record)
                    if matches!(
                        &record.data,
                        RecordData::Item(item)
                            if matches!(&item.data, ItemData::Message(_))
                    )
            )
        })
        .count();
    assert_eq!(messages, 2, "one canonical input and one canonical answer");
    let completed_turn = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Turn(turn) if turn.status == TurnStatus::Completed => {
                    Some((record, turn))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("legacy message boundaries should infer a completed turn");
    assert_eq!(completed_turn.1.duration_ms, Some(1_100));
    assert!(completed_turn
        .1
        .external_id
        .as_deref()
        .is_some_and(|id| id.starts_with("legacy:")));
    assert!(batch.changes.iter().all(|change| {
        !matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item) if matches!(&item.data, ItemData::Message(_))
                ) && record.turn.as_ref() != Some(&completed_turn.0.id)
        )
    }));
    assert_eq!(
        batch
            .changes
            .iter()
            .filter(|change| {
                matches!(
                    change,
                    Change::Upsert(record)
                        if matches!(
                            &record.data,
                            RecordData::Item(item)
                                if matches!(&item.data, ItemData::Reasoning(_))
                        )
                )
            })
            .count(),
        1
    );
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Notice(notice)
                                if notice.level == NoticeLevel::Error
                                    && notice.code.as_deref()
                                        == Some("internal_server_error")
                        )
                )
        )
    }));
}

#[test]
fn canonical_presentation_appended_later_deletes_the_legacy_projection() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"legacy answer\",\"phase\":\"final_answer\"}}\n"
    ));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let legacy_id = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Item(item) if matches!(&item.data, ItemData::Message(_)) => {
                    Some(record.id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("legacy message");
    let mut incremental = HashMap::new();
    apply_changes(&mut incremental, &first.changes);

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(
            "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"message-1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"canonical answer\"}]}}\n"
                .as_bytes(),
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(id) if id == &legacy_id)));
    apply_changes(&mut incremental, &second.changes);
    assert!(!incremental.contains_key(&legacy_id));

    let fresh = provider.scan(None).unwrap();
    let mut fresh_records = HashMap::new();
    apply_changes(&mut fresh_records, &fresh.changes);
    assert_eq!(incremental, fresh_records);
}

#[test]
fn legacy_presentation_remains_when_no_canonical_projection_ever_arrives() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"legacy only\",\"phase\":\"final_answer\"}}\n"
    ));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let legacy_id = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Item(item) if matches!(&item.data, ItemData::Message(_)) => {
                    Some(record.id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("legacy message");
    let mut incremental = HashMap::new();
    apply_changes(&mut incremental, &first.changes);

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(token_count().as_bytes())
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(!second
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(id) if id == &legacy_id)));
    apply_changes(&mut incremental, &second.changes);
    assert!(incremental.contains_key(&legacy_id));

    let fresh = provider.scan(None).unwrap();
    let mut fresh_records = HashMap::new();
    apply_changes(&mut fresh_records, &fresh.changes);
    assert_eq!(incremental, fresh_records);
}

#[test]
fn scan_normalizes_ordered_items_without_losing_tool_correlation() {
    let fixture = Fixture::new(&format!(
        "{}{}{}{}{}{}{}",
        session_meta(),
        turn_started(),
        tool_call(),
        tool_result(),
        message(),
        reasoning(),
        compacted()
    ));
    let batch = fixture.provider().scan(None).unwrap();

    let call = batch.changes.iter().find_map(|change| match change {
        Change::Upsert(record) => match &record.data {
            RecordData::Item(item) => match &item.data {
                ItemData::ToolCall(call) if call.call_id == "call-1" => Some((record, item, call)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    });
    let (call_record, call_item, call) = call.expect("tool call");
    assert_eq!(call.kind, coding_agent_data::ToolKind::Read);
    assert_eq!(call.status, ToolStatus::Pending);
    assert_eq!(call_item.actor, Actor::Agent);

    let result = batch.changes.iter().find_map(|change| match change {
        Change::Upsert(record) => match &record.data {
            RecordData::Item(item) => match &item.data {
                ItemData::ToolResult(result) if result.call_id == "call-1" => Some((item, result)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    });
    let (result_item, result) = result.expect("tool result");
    assert_eq!(result_item.parent.as_ref(), Some(&call_record.id));
    assert_eq!(result.status, ToolStatus::Completed);
    assert_eq!(result.content, vec![ContentBlock::text("file body")]);

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if item.actor == Actor::Agent
                            && matches!(
                                &item.data,
                                ItemData::Message(message)
                                    if message.role == MessageRole::Assistant
                                        && message.content
                                            == vec![ContentBlock::text("done")]
                            )
            )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Reasoning(reasoning)
                                if reasoning.summary == ["checked the files"]
                                    && reasoning.content
                                        == vec![ContentBlock::text("visible reasoning")]
                        )
            )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ContextCompaction(compaction)
                                if compaction.summary.as_deref()
                                    == Some("replacement summary")
                        )
                )
        )
    }));
}

#[test]
fn scan_normalizes_acp_v2_content_annotations_icons_and_extensions() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), acp_v2_content_message()));
    let batch = fixture.provider().scan(None).unwrap();
    let content = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Item(item) => match &item.data {
                    ItemData::Message(message)
                        if item.external_id.as_deref() == Some("message-v2") =>
                    {
                        Some(&message.content)
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .expect("v2 content message");

    assert!(matches!(
        &content[0],
        ContentBlock::Text {
            text,
            annotations: Some(annotations),
        } if text == "annotated"
            && annotations.audience
                == [ContentAudience::User, ContentAudience::Other("_reviewer".to_owned())]
            && annotations.last_modified.is_some()
            && annotations.priority == ContentPriority::new(0.8)
    ));
    assert!(matches!(
        &content[1],
        ContentBlock::ResourceLink {
            uri,
            name: Some(name),
            icons,
            ..
        } if uri == "file:///workspace/report.pdf"
            && name == "report.pdf"
            && icons.len() == 1
            && icons[0].src == "https://example.com/pdf-dark.png"
            && icons[0].mime_type.as_deref() == Some("image/png")
            && icons[0].sizes == ["48x48"]
            && icons[0].theme == Some(ContentIconTheme::Dark)
    ));
    assert!(matches!(
        &content[2],
        ContentBlock::Unknown {
            kind: Some(kind),
            value,
        } if kind == "_chart" && value["series"][2] == 3
    ));
    assert!(matches!(
        &content[3],
        ContentBlock::Unknown {
            kind: None,
            value,
        } if value["series"][1] == 5
    ));
    assert!(matches!(
        &content[4],
        ContentBlock::Text {
            annotations: Some(annotations),
            ..
        } if annotations.priority.is_none()
    ));
}

#[test]
fn scan_normalizes_hosted_tool_results() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), hosted_tool_items()));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolCall(call)
                                if call.call_id == "search-1"
                                    && call.status == ToolStatus::InProgress
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "search-1"
                                    && result.status == ToolStatus::Completed
                                    && result.output["tools"][0]["name"] == "formatter"
                                    && result.content.is_empty()
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "image-1"
                                    && result.status == ToolStatus::Completed
                                    && result.content
                                        == vec![ContentBlock::Image {
                                            mime_type: None,
                                            uri: None,
                                            data: Some("encoded-image".to_owned()),
                                            annotations: None,
                                        }]
                        )
                )
        )
    }));
}

#[test]
fn compacted_events_normalize_camel_case_fields() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        compacted_with_camel_case_fields()
    ));

    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ContextCompaction(compaction)
                                if compaction.summary.as_deref() == Some("replacement summary")
                                    && compaction.automatic == Some(true)
                                    && compaction.tokens_before == Some(120000)
                                    && compaction.tokens_after == Some(48000)
                                    && compaction.replacement_history.as_ref()
                                        .is_some_and(|history| history.len() == 1)
                                    && compaction.window_number == Some(2)
                                    && compaction.first_window_id.as_deref()
                                        == Some("window-1")
                                    && compaction.previous_window_id.as_deref()
                                        == Some("window-1")
                                    && compaction.window_id.as_deref() == Some("window-2")
                        )
                )
        )
    }));
}

#[test]
fn scan_normalizes_patch_mcp_subagent_and_context_lifecycle_events() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        turn_started(),
        lifecycle_events()
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolCall(call)
                                if call.call_id == "patch-1"
                                    && call.kind == ToolKind::Edit
                                    && call.status == ToolStatus::Completed
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "patch-1"
                                    && result.content
                                        == vec![ContentBlock::text("Done!")]
                                    && result.output["changes"]["src/new.rs"]["type"]
                                        == "add"
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::FileChange(change)
                                if change.kind == FileChangeKind::Create
                                    && change.path == std::path::Path::new("src/new.rs")
                                    && change.status == ToolStatus::Completed
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::FileChange(change)
                                if change.kind == FileChangeKind::Move
                                    && change.old_path.as_deref()
                                        == Some(std::path::Path::new("src/old.rs"))
                                    && change.path
                                        == std::path::Path::new("src/moved.rs")
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "mcp-1"
                                    && result.status == ToolStatus::Completed
                                    && result.duration_ms == Some(1_500)
                                    && result.content
                                        == vec![ContentBlock::text("found")]
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::AgentInvocation(invocation)
                                if invocation.operation == AgentOperation::Spawn
                                    && invocation.status
                                        == AgentInvocationStatus::InProgress
                                    && invocation.child_session.is_some()
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Plan(plan)
                                if plan.text.as_deref() == Some("Implementation plan")
                                    && plan.steps.len() == 3
                                    && plan.steps[1].status
                                        == PlanStepStatus::InProgress
                                    && plan.steps[2].status
                                        == PlanStepStatus::Cancelled
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(&item.data, ItemData::ContextCompaction(_))
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::Rollback(rollback)
                                if rollback.turns_removed == Some(2)
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if item.external_id.as_deref() == Some("future-1")
                            && matches!(
                                &item.data,
                                ItemData::Unknown(unknown)
                                    if unknown.kind.as_deref()
                                        == Some("future_response_item")
                            )
                )
        )
    }));
}

#[test]
fn unknown_response_item_keeps_its_explicit_turn_after_terminal_context_is_cleared() {
    let fixture = Fixture::new(&format!(
        "{}{}{}{}",
        session_meta(),
        turn_started(),
        turn_completed(),
        "{\"timestamp\":\"2026-01-02T03:04:09Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"future_response_item\",\"id\":\"future-after-turn\",\"internal_chat_message_metadata_passthrough\":{\"turn_id\":\"turn-1\"}}}\n",
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let record = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if item.external_id.as_deref() == Some("future-after-turn")
                ) =>
            {
                Some(record)
            }
            _ => None,
        })
        .expect("unknown response item");

    assert!(record
        .turn
        .as_ref()
        .is_some_and(|id| id.as_str().ends_with(":turn-1")));
}

#[test]
fn scan_normalizes_terminal_tools_and_inter_agent_messages() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        turn_started(),
        terminal_tool_events()
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "exec-1"
                                    && result.status == ToolStatus::Completed
                                    && result.duration_ms == Some(1_250)
                                    && result.output["exit_code"] == 0
                                    && result.content
                                        == vec![ContentBlock::text(
                                            "/workspace/project\n"
                                        )]
                        )
                )
        )
    }));
    assert_eq!(
        batch
            .changes
            .iter()
            .filter(|change| {
                matches!(
                    change,
                    Change::Upsert(record)
                        if matches!(
                            &record.data,
                            RecordData::Item(item)
                                if matches!(
                                    &item.data,
                                    ItemData::ToolResult(result)
                                        if result.call_id == "exec-1"
                                )
                        )
                )
            })
            .count(),
        1,
        "the response-item projection must not replace the richer terminal result"
    );
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "web-1"
                                    && result.output["results"][0]["title"] == "Rust"
                                    && result.content.is_empty()
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolCall(call)
                                if call.call_id == "view-1"
                                    && call.kind == ToolKind::Read
                                    && call.locations.first().is_some_and(|location|
                                        location.path == std::path::Path::new("/tmp/diagram.png")
                                    )
                        )
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::ToolResult(result)
                                if result.call_id == "image-2"
                                    && result.content == vec![ContentBlock::Image {
                                        mime_type: None,
                                        uri: Some("/tmp/diagram.png".to_owned()),
                                        data: Some("encoded-image".to_owned()),
                                        annotations: None,
                                    }]
                        )
                )
        )
    }));
    assert_eq!(
        batch
            .changes
            .iter()
            .filter(|change| {
                matches!(
                    change,
                    Change::Upsert(record)
                        if matches!(
                            &record.data,
                            RecordData::Item(item)
                                if matches!(
                                    &item.data,
                                    ItemData::ToolResult(result)
                                        if result.call_id == "image-2"
                                )
                        )
                )
            })
            .count(),
        1,
        "the image response projection must not replace the richer terminal result"
    );
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Item(item)
                        if matches!(
                            &item.data,
                            ItemData::AgentInvocation(invocation)
                                if invocation.operation == AgentOperation::SendInput
                                    && invocation.sender_id.as_deref() == Some("reviewer")
                                    && invocation.receiver_ids == ["root"]
                                    && invocation.task_id.as_deref() == Some("turn-1")
                        )
                )
        )
    }));
}

#[test]
fn spawn_relationship_changes_upsert_the_child_session() {
    let fixture = Fixture::new(session_meta());
    let connection = Connection::open(&fixture.database_path).unwrap();
    connection
        .execute_batch(
            "
            CREATE TABLE thread_spawn_edges (
                parent_thread_id TEXT NOT NULL,
                child_thread_id TEXT NOT NULL PRIMARY KEY,
                status TEXT NOT NULL
            );
            INSERT INTO thread_spawn_edges (parent_thread_id, child_thread_id, status)
            VALUES ('parent-1', 'thread-1', 'completed');
            ",
        )
        .unwrap();
    drop(connection);
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.relations.iter().any(|relation|
                            relation.kind == SessionRelationKind::Child
                                && relation.session.as_str().ends_with(":session:parent-1")
                        )
                )
        )
    }));

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE thread_spawn_edges SET parent_thread_id = 'parent-2' WHERE child_thread_id = 'thread-1'",
            [],
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.relations.iter().any(|relation|
                            relation.session.as_str().ends_with(":session:parent-2")
                        )
                )
        )
    }));

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute("DELETE FROM thread_spawn_edges", [])
        .unwrap();
    let third = provider.scan(Some(&second.checkpoint)).unwrap();
    assert!(third.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session) if session.relations.is_empty()
                )
        )
    }));
}

#[test]
fn incomplete_spawn_edge_reads_keep_the_last_snapshot_and_retry() {
    let fixture = Fixture::new(session_meta());
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            CREATE TABLE thread_spawn_edges (
                parent_thread_id TEXT NOT NULL,
                child_thread_id TEXT NOT NULL PRIMARY KEY
            );
            INSERT INTO thread_spawn_edges (parent_thread_id, child_thread_id)
            VALUES ('parent-1', 'thread-1');
            ",
        )
        .unwrap();
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let first_state: serde_json::Value = first.checkpoint.decode_state(provider.info()).unwrap();

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            DROP TABLE thread_spawn_edges;
            CREATE TABLE thread_spawn_edges (
                parent_thread TEXT NOT NULL,
                child_thread_id TEXT NOT NULL PRIMARY KEY
            );
            INSERT INTO thread_spawn_edges (parent_thread, child_thread_id)
            VALUES ('lost-parent', 'thread-1');
            ",
        )
        .unwrap();

    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.state_db.spawn_edges_unavailable"));
    let second_state: serde_json::Value = second.checkpoint.decode_state(provider.info()).unwrap();
    assert_eq!(second_state["database"], first_state["database"]);
    assert_eq!(second_state["threads"], first_state["threads"]);

    let third = provider.scan(Some(&second.checkpoint)).unwrap();
    assert!(third
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.state_db.spawn_edges_unavailable"));

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute_batch(
            "
            DROP TABLE thread_spawn_edges;
            CREATE TABLE thread_spawn_edges (
                parent_thread_id TEXT NOT NULL,
                child_thread_id TEXT NOT NULL PRIMARY KEY
            );
            INSERT INTO thread_spawn_edges (parent_thread_id, child_thread_id)
            VALUES ('parent-2', 'thread-1');
            ",
        )
        .unwrap();

    let fourth = provider.scan(Some(&third.checkpoint)).unwrap();
    assert!(fourth.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.relations.iter().any(|relation|
                            relation.session.as_str().ends_with(":session:parent-2")
                        )
                )
        )
    }));
}

#[test]
fn metadata_changes_are_detected_without_timestamp_changes() {
    let fixture = Fixture::new(session_meta());
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE threads SET title = 'Renamed' WHERE id = 'thread-1'",
            [],
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.title.as_deref() == Some("Renamed")
                )
        )
    }));
}

#[test]
fn malformed_thread_identity_does_not_emit_false_deletions() {
    let fixture = Fixture::new(session_meta());
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let session_id = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record)
                if matches!(&record.data, RecordData::Session(session) if session.external_id == "thread-1") =>
            {
                Some(record.id.clone())
            }
            _ => None,
        })
        .expect("indexed session");

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute("UPDATE threads SET id = NULL WHERE id = 'thread-1'", [])
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    assert!(second
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.state_db.thread_without_id"));
    assert!(second
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.state_db.deletions_suppressed"));
    assert!(!second
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(id) if id == &session_id)));
}

#[test]
fn terminal_turn_preserves_start_time_and_derives_duration() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        turn_started(),
        turn_completed()
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Turn(turn)
                        if turn.status == TurnStatus::Completed
                            && turn.started_at.map(|value| value.as_millis())
                                == Some(1_704_164_646_000)
                            && turn.completed_at.map(|value| value.as_millis())
                                == Some(1_704_164_648_000)
                            && turn.duration_ms == Some(2_000)
                )
        )
    }));
}

#[test]
fn aborted_turn_preserves_interruption_semantics() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        turn_started(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\",\"turn_id\":\"turn-1\",\"reason\":\"interrupted\",\"started_at\":1704164646,\"completed_at\":1704164648,\"duration_ms\":2000}}\n"
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Turn(turn)
                        if turn.status == TurnStatus::Interrupted
                            && turn.stop_reason.as_ref() == Some(&StopReason::Interrupted)
                            && turn.duration_ms == Some(2_000)
                )
        )
    }));
}

#[test]
fn aborted_turn_inherits_the_started_turn_identity() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        turn_started(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"turn_aborted\",\"reason\":\"interrupted\"}}\n"
    ));
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Turn(turn)
                        if turn.external_id.as_deref() == Some("turn-1")
                            && turn.status == TurnStatus::Interrupted
                )
        )
    }));
}

#[test]
fn deleted_index_rows_fall_back_to_the_matching_rollout_session() {
    let fixture = Fixture::new(session_meta());
    let second_rollout = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &second_rollout,
        "{\"timestamp\":\"2026-01-02T04:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"cwd\":\"/workspace/second\"}}\n",
    )
    .unwrap();
    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "
            INSERT INTO threads (
                id, rollout_path, created_at_ms, updated_at_ms, title, cwd,
                tokens_used, archived, model, git_branch
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                "thread-2",
                second_rollout.to_string_lossy(),
                1_700_000_002_000_i64,
                1_700_000_003_000_i64,
                "Second thread",
                "/workspace/second",
                12_i64,
                0_i64,
                "gpt-test",
                "feature"
            ],
        )
        .unwrap();
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute("DELETE FROM threads WHERE id = 'thread-2'", [])
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "thread-2"
                            && session.quality == DataQuality::Partial
                )
        )
    }));
}

#[test]
fn checkpoint_round_trip_preserves_position_and_source_identity() {
    let first_fixture = Fixture::new(session_meta());
    let first_provider = first_fixture.provider();
    let first = first_provider.scan(None).unwrap();

    let encoded = first.checkpoint.to_json().unwrap();
    let decoded = coding_agent_data::Checkpoint::from_json(&encoded).unwrap();
    assert_eq!(decoded.source(), &first_provider.info().source);
    assert!(first_provider
        .scan(Some(&decoded))
        .unwrap()
        .changes
        .is_empty());

    let encoded_value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert!(encoded_value["state"].get("version").is_none());

    let second_fixture = Fixture::new(session_meta());
    let error = second_fixture
        .provider()
        .scan(Some(&decoded))
        .expect_err("a checkpoint must not cross source instances");
    assert!(matches!(
        error,
        coding_agent_data::Error::InvalidCheckpoint(_)
    ));
}

#[test]
fn compressed_rollouts_without_a_trailing_newline_are_normalized_once() {
    let fixture = Fixture::new("");
    fs::remove_file(&fixture.rollout_path).unwrap();
    let compressed_path = fixture.rollout_path.with_extension("jsonl.zst");
    let compressed = zstd::stream::encode_all(
        b"{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"future_event\"}".as_slice(),
        1,
    )
    .unwrap();
    fs::write(compressed_path, compressed).unwrap();
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Unknown(unknown)
                        if unknown.kind.as_deref() == Some("future_event")
                )
        )
    }));
    assert!(provider
        .scan(Some(&first.checkpoint))
        .unwrap()
        .changes
        .is_empty());
}

#[test]
fn repeated_cumulative_usage_does_not_repeat_delta_and_missing_delta_is_derived() {
    let repeated = token_count();
    let advanced_without_last = "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":140,\"input_tokens\":100,\"cache_write_input_tokens\":10,\"output_tokens\":40}}}}\n";
    let fixture = Fixture::new(&format!(
        "{}{}{}{}",
        session_meta(),
        token_count(),
        repeated,
        advanced_without_last
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let usage = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => Some(usage),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(usage.len(), 3);
    assert_eq!(usage[0].delta.as_ref().map(|usage| usage.total), Some(25));
    assert!(usage[1].delta.is_none());
    assert_eq!(usage[2].delta.as_ref().map(|usage| usage.total), Some(40));
    assert_eq!(
        usage[2].delta.as_ref().and_then(|usage| usage.input),
        Some(30)
    );
    assert_eq!(
        usage[2].delta.as_ref().and_then(|usage| usage.output),
        Some(10)
    );
}

#[test]
fn reasoning_tokens_remain_an_output_breakdown_without_inflating_totals() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":100,\"input_tokens\":70,\"output_tokens\":30,\"reasoning_output_tokens\":20},\"last_token_usage\":{\"total_tokens\":25,\"input_tokens\":20,\"output_tokens\":5,\"reasoning_output_tokens\":4}}}}\n",
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let usage = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => Some(usage),
                _ => None,
            },
            _ => None,
        })
        .unwrap();

    assert_eq!(usage.delta.as_ref().map(|usage| usage.total), Some(25));
    assert_eq!(usage.delta.as_ref().and_then(|usage| usage.output), Some(5));
    assert_eq!(
        usage
            .delta
            .as_ref()
            .and_then(|usage| usage.reasoning_output),
        Some(4)
    );
}

#[test]
fn legacy_usage_aliases_derive_total_without_double_counting_reasoning() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"prompt_tokens\":70,\"cached_tokens\":40,\"completion_tokens\":30,\"reasoning_tokens\":20},\"last_token_usage\":{\"prompt_tokens\":70,\"cached_tokens\":40,\"completion_tokens\":30,\"reasoning_tokens\":20}}}}\n",
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let usage = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => Some(usage),
                _ => None,
            },
            _ => None,
        })
        .expect("legacy usage observation");
    let cumulative = usage.cumulative.as_ref().expect("cumulative usage");
    let delta = usage.delta.as_ref().expect("additive usage");

    assert_eq!(cumulative.total, 100);
    assert_eq!(cumulative.input, Some(70));
    assert_eq!(cumulative.cached_input, Some(40));
    assert_eq!(cumulative.output, Some(30));
    assert_eq!(cumulative.reasoning_output, Some(20));
    assert_eq!(delta, cumulative);
}

#[test]
fn newly_available_cumulative_components_are_not_treated_as_component_deltas() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":100}}}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:09Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":150,\"input_tokens\":130,\"output_tokens\":20}}}}\n",
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let usage = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => Some(usage),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(usage.len(), 2);
    assert!(usage[0].delta.is_none());
    let delta = usage[1].delta.as_ref().expect("safe total delta");
    assert_eq!(delta.total, 50);
    assert_eq!(delta.input, None);
    assert_eq!(delta.output, None);
}

#[test]
fn first_cumulative_usage_without_a_reported_delta_uses_a_zero_baseline() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":40,\"input_tokens\":30,\"output_tokens\":10}}}}\n",
    ));

    let batch = fixture.provider().scan(None).unwrap();
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Usage(usage)
                        if usage.delta.as_ref().map(|usage| usage.total) == Some(40)
                            && usage.delta.as_ref().and_then(|usage| usage.input) == Some(30)
                            && usage.delta.as_ref().and_then(|usage| usage.output) == Some(10)
                )
        )
    }));
}

#[test]
fn interleaved_cumulative_usage_is_contained_and_incremental_scan_matches_fresh_scan() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        cumulative_token_count("2026-01-02T03:04:08Z", 100_000),
        cumulative_token_count("2026-01-02T03:04:09Z", 5_000),
    ));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let mut incremental_records = HashMap::new();
    apply_changes(&mut incremental_records, &first.changes);

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(
            format!(
                "{}{}{}",
                cumulative_token_count("2026-01-02T03:04:10Z", 101_000),
                cumulative_token_count("2026-01-02T03:04:11Z", 6_000),
                cumulative_token_count("2026-01-02T03:04:12Z", 102_000),
            )
            .as_bytes(),
        )
        .unwrap();

    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    apply_changes(&mut incremental_records, &second.changes);
    let fresh = provider.scan(None).unwrap();
    let mut fresh_records = HashMap::new();
    apply_changes(&mut fresh_records, &fresh.changes);

    assert_eq!(incremental_records, fresh_records);
    assert_eq!(
        usage_deltas_for_session(&fresh.changes, "thread-1"),
        [Some(100_000), None, Some(1_000), None, Some(1_000)]
    );
    assert_eq!(
        usage_deltas_for_session(&fresh.changes, "thread-1")
            .into_iter()
            .flatten()
            .sum::<i64>(),
        102_000
    );
}

#[test]
fn divergent_cumulative_totals_prefer_trustworthy_reported_deltas() {
    assert_incremental_usage_matches_fresh(
        &token_count_values("2026-01-02T03:04:08Z", 100, 100),
        &format!(
            "{}{}",
            token_count_values("2026-01-02T03:04:09Z", 1_000, 40),
            token_count_values("2026-01-02T03:04:10Z", 1_050, 50),
        ),
        &[Some(100), Some(40), Some(50)],
    );
}

#[test]
fn cross_component_divergence_keeps_the_provider_reported_delta_whole() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":110,\"input_tokens\":100,\"cached_input_tokens\":20,\"output_tokens\":10},\"last_token_usage\":{\"total_tokens\":110,\"input_tokens\":100,\"cached_input_tokens\":20,\"output_tokens\":10}}}}\n",
        "{\"timestamp\":\"2026-01-02T03:04:09Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"total_tokens\":176,\"input_tokens\":160,\"cached_input_tokens\":40,\"output_tokens\":16},\"last_token_usage\":{\"total_tokens\":60,\"input_tokens\":50,\"cached_input_tokens\":20,\"output_tokens\":10}}}}\n",
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let deltas = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => usage.delta.as_ref(),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        deltas[1],
        &TokenUsage {
            total: 60,
            input: Some(50),
            cache_creation_input: None,
            cache_creation_ephemeral_5m_input: None,
            cache_creation_ephemeral_1h_input: None,
            cached_input: Some(20),
            output: Some(10),
            reasoning_output: None,
        }
    );
}

#[test]
fn divergent_accounting_continues_to_trust_reported_deltas() {
    assert_incremental_usage_matches_fresh(
        &format!(
            "{}{}",
            token_count_values("2026-01-02T03:04:08Z", 100, 100),
            token_count_values("2026-01-02T03:04:09Z", 1_000, 40),
        ),
        &token_count_values("2026-01-02T03:04:10Z", 1_050, 60),
        &[Some(100), Some(40), Some(60)],
    );
}

#[test]
fn repeated_divergent_snapshot_is_not_counted_more_than_once() {
    assert_incremental_usage_matches_fresh(
        &token_count_values("2026-01-02T03:04:08Z", 50, 100),
        &format!(
            "{}{}",
            token_count_values("2026-01-02T03:04:09Z", 50, 100),
            token_count_values("2026-01-02T03:04:10Z", 50, 100),
        ),
        &[Some(100), None, None],
    );
}

#[test]
fn nonconsecutive_cumulative_reemission_is_suppressed_across_incremental_scans() {
    assert_incremental_usage_matches_fresh(
        &format!(
            "{}{}",
            token_count_values("2026-01-02T03:04:08Z", 50, 10),
            token_count_values("2026-01-02T03:04:09Z", 100, 10),
        ),
        &cumulative_token_count("2026-01-02T03:04:10Z", 50),
        &[Some(10), Some(10), None],
    );
}

#[test]
fn interleaved_lineage_caps_large_reported_delta_at_watermark_growth() {
    assert_incremental_usage_matches_fresh(
        &format!(
            "{}{}",
            token_count_values("2026-01-02T03:04:08Z", 100_000, 100_000),
            token_count_values("2026-01-02T03:04:09Z", 5_000, 5_000),
        ),
        &token_count_values("2026-01-02T03:04:10Z", 101_000, 100_000),
        &[Some(100_000), None, Some(1_000)],
    );
}

#[test]
fn total_only_snapshot_recovers_from_counted_baseline_after_divergence() {
    assert_incremental_usage_matches_fresh(
        &format!(
            "{}{}",
            token_count_values("2026-01-02T03:04:08Z", 100, 100),
            token_count_values("2026-01-02T03:04:09Z", 1_000, 40),
        ),
        &cumulative_token_count("2026-01-02T03:04:10Z", 180),
        &[Some(100), Some(40), Some(40)],
    );
}

#[test]
fn estimated_total_only_last_usage_is_evidence_but_not_additive() {
    let fixture = Fixture::new(&format!(
        "{}{}{}{}",
        session_meta(),
        token_count_values("2026-01-02T03:04:08Z", 100, 100),
        estimated_token_count("2026-01-02T03:04:09Z", 50, 40),
        token_count_values("2026-01-02T03:04:10Z", 110, 60),
    ));

    let batch = fixture.provider().scan(None).unwrap();
    let usage = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => Some(usage),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(usage.len(), 3);
    assert_eq!(usage[0].delta.as_ref().map(|usage| usage.total), Some(100));
    assert_eq!(
        usage[1].cumulative.as_ref().map(|usage| usage.total),
        Some(50)
    );
    assert!(usage[1].delta.is_none());
    assert_eq!(usage[2].delta.as_ref().map(|usage| usage.total), Some(10));
}

#[test]
fn identical_usage_across_sessions_is_attributed_once_without_hiding_raw_records() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), token_count()));
    let duplicate_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &duplicate_path,
        format!(
            "{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"cwd\":\"/workspace/other\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"different session\"}]}}\n",
            token_count(),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &duplicate_path, 100);

    let mut records = HashMap::new();
    scan_into_records(&fixture.provider(), None, &mut records);

    let usage = records
        .values()
        .filter(|record| matches!(record.data, RecordData::Usage(_)))
        .collect::<Vec<_>>();
    assert_eq!(usage.len(), 2);
    assert!(usage.iter().all(|record| record.original.is_some()));
    assert_eq!(usage_deltas_in_records(&records, "thread-1"), [Some(25)]);
    assert_eq!(usage_deltas_in_records(&records, "thread-2"), [None]);
    assert_eq!(
        usage
            .iter()
            .filter_map(|record| match &record.data {
                RecordData::Usage(usage) => usage.delta.as_ref().map(|delta| delta.total),
                _ => None,
            })
            .sum::<i64>(),
        25
    );
}

#[test]
fn a_new_canonical_usage_owner_makes_incremental_and_fresh_scans_converge() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), token_count()));
    let provider = fixture.provider();
    let mut incremental_records = HashMap::new();
    let checkpoint = scan_into_records(&provider, None, &mut incremental_records);

    let canonical_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-0-thread-2.jsonl");
    fs::write(
        &canonical_path,
        format!(
            "{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"cwd\":\"/workspace/other\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"created later\"}]}}\n",
            token_count(),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &canonical_path, 100);

    scan_into_records(&provider, Some(checkpoint), &mut incremental_records);
    let mut fresh_records = HashMap::new();
    scan_into_records(&provider, None, &mut fresh_records);

    assert_eq!(incremental_records, fresh_records);
    assert_eq!(
        usage_deltas_in_records(&incremental_records, "thread-1"),
        [None]
    );
    assert_eq!(
        usage_deltas_in_records(&incremental_records, "thread-2"),
        [Some(25)]
    );
}

#[test]
fn removing_a_usage_owner_promotes_the_remaining_raw_observation() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), token_count()));
    let duplicate_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &duplicate_path,
        format!(
            "{}{}",
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"cwd\":\"/workspace/other\"}}\n",
            token_count(),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &duplicate_path, 100);
    let provider = fixture.provider();
    let mut incremental_records = HashMap::new();
    let checkpoint = scan_into_records(&provider, None, &mut incremental_records);
    assert_eq!(
        usage_deltas_in_records(&incremental_records, "thread-2"),
        [None]
    );

    fs::remove_file(&fixture.rollout_path).unwrap();
    scan_into_records(&provider, Some(checkpoint), &mut incremental_records);
    let mut fresh_records = HashMap::new();
    scan_into_records(&provider, None, &mut fresh_records);

    assert_eq!(incremental_records, fresh_records);
    assert!(usage_deltas_in_records(&incremental_records, "thread-1").is_empty());
    assert_eq!(
        usage_deltas_in_records(&incremental_records, "thread-2"),
        [Some(25)]
    );
}

#[test]
fn dense_usage_long_after_a_fork_is_not_treated_as_rewritten_history() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        token_count_values("2026-01-02T03:04:30Z", 100, 100),
    ));
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-01-02T04:00:00.100Z", 40, 40),
            token_count_values("2026-01-02T04:00:00.200Z", 70, 30),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 70);

    let batch = fixture.provider().scan(None).unwrap();

    assert_eq!(
        usage_deltas_for_session(&batch.changes, "thread-2"),
        [Some(40), Some(30)]
    );
}

#[test]
fn structural_fork_boundary_wins_over_a_matching_parent_usage_suffix() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        token_count_values("2026-01-02T03:04:10Z", 100, 100),
        token_count_values("2026-01-02T03:04:20Z", 150, 50),
    ));
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-01-02T03:05:00Z", 100, 100),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 150);
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    assert_eq!(usage_deltas_for_session(&first.changes, "thread-2"), [None]);

    OpenOptions::new()
        .append(true)
        .open(&child_path)
        .unwrap()
        .write_all(
            format!(
                "{}{}{}{}",
                "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"child-turn\"}}\n",
                "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"child-turn\"}}\n",
                "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"inter_agent_communication_metadata\",\"payload\":{\"trigger_turn\":true}}\n",
                token_count_values("2026-01-02T03:05:01Z", 150, 50),
            )
            .as_bytes(),
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    assert_eq!(
        usage_deltas_for_session(&second.changes, "thread-2"),
        [Some(50)]
    );
}

#[test]
fn paginated_fork_keeps_dense_usage_at_or_after_its_owned_ordinal() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        token_count_values("2026-01-02T03:04:30Z", 100, 100),
    ));
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"history_mode\":\"paginated\",\"subagent_history_start_ordinal\":1,\"cwd\":\"/workspace/project\"}}\n",
            ordinal_token_count_values("2026-01-02T03:05:00.100Z", 1, 40, 40),
            ordinal_token_count_values("2026-01-02T03:05:00.200Z", 2, 70, 30),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 70);

    let batch = fixture.provider().scan(None).unwrap();

    assert_eq!(
        usage_deltas_for_session(&batch.changes, "thread-2"),
        [Some(40), Some(30)]
    );
}

#[test]
fn paginated_history_does_not_attribute_usage_without_an_owned_ordinal() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"history_mode\":\"paginated\",\"subagent_history_start_ordinal\":2,\"cwd\":\"/workspace/project\"}}\n",
        token_count_values("2026-01-02T03:05:00.100Z", 40, 40),
        ordinal_token_count_values("2026-01-02T03:05:00.200Z", 2, 70, 30),
    ));

    let batch = fixture.provider().scan(None).unwrap();

    assert_eq!(
        usage_deltas_for_session(&batch.changes, "thread-1"),
        [Some(30)]
    );
}

#[test]
fn fork_replay_is_not_attributed_twice_and_incremental_scan_keeps_its_position() {
    let parent = format!(
        "{}{}{}{}{}",
        session_meta(),
        turn_started(),
        token_count_values("2026-01-02T03:04:07Z", 100, 100),
        token_count_values("2026-01-02T03:04:30Z", 150, 50),
        token_count_values("2026-01-02T03:06:00Z", 175, 25),
    );
    let fixture = Fixture::new(&parent);
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"child-turn\",\"started_at\":1704164700}}\n",
            token_count_values("2026-01-02T03:05:00Z", 100, 100),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 175);
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    let mut incremental_records = HashMap::new();
    apply_changes(&mut incremental_records, &first.changes);
    let child_usage = first
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record)
                if record
                    .session
                    .as_ref()
                    .is_some_and(|id| id.as_str().ends_with(":session:thread-2")) =>
            {
                match &record.data {
                    RecordData::Usage(usage) => Some(usage),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(child_usage.len(), 1);
    assert_eq!(
        child_usage[0].cumulative.as_ref().map(|usage| usage.total),
        Some(100)
    );
    assert!(child_usage[0].delta.is_none());

    OpenOptions::new()
        .append(true)
        .open(&child_path)
        .unwrap()
        .write_all(
            format!(
                "{}{}",
                token_count_values("2026-01-02T03:05:00Z", 150, 50),
                token_count_values("2026-01-02T03:07:00Z", 175, 25),
            )
            .as_bytes(),
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    apply_changes(&mut incremental_records, &second.changes);
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record
                    .session
                    .as_ref()
                    .is_some_and(|id| id.as_str().ends_with(":session:thread-2"))
                    && matches!(
                        &record.data,
                        RecordData::Usage(usage)
                            if usage.delta.as_ref().map(|usage| usage.total) == Some(25)
                    )
        )
    }));

    let fresh = provider.scan(None).unwrap();
    let mut full_records = HashMap::new();
    apply_changes(&mut full_records, &fresh.changes);
    assert_eq!(incremental_records, full_records);
    let attributed_total = fresh
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .sum::<i64>();
    assert_eq!(attributed_total, 200);
}

#[test]
fn copied_legacy_items_link_to_their_immediate_parent_records() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"parent answer\"}]}}\n"
    ));
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        concat!(
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"parent answer\"}]}}\n"
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 0);

    let batch = fixture.provider().scan(None).unwrap();
    let parent = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record)
                if record.origin.path == fixture.rollout_path
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if item.external_id.as_deref() == Some("shared-message")
                    ) =>
            {
                Some(record.id.clone())
            }
            _ => None,
        })
        .expect("parent message");
    let child = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record)
                if record.origin.path == child_path
                    && matches!(
                        &record.data,
                        RecordData::Item(item)
                            if item.external_id.as_deref() == Some("shared-message")
                    ) =>
            {
                match &record.data {
                    RecordData::Item(item) => Some(item),
                    _ => None,
                }
            }
            _ => None,
        })
        .expect("copied child message");
    assert_eq!(child.inherited_from.as_ref(), Some(&parent));
}

#[test]
fn numeric_fork_timestamp_excludes_parent_usage_recorded_after_the_fork() {
    let fixture = Fixture::new(&format!(
        "{}{}{}",
        session_meta(),
        token_count_values("2026-07-10T08:01:00Z", 100, 100),
        token_count_values("2026-07-10T08:03:00Z", 150, 50),
    ));
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}{}",
            "{\"timestamp\":1783670520,\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-07-10T08:02:00Z", 100, 100),
            token_count_values("2026-07-10T08:04:00Z", 150, 50),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 150);

    let batch = fixture.provider().scan(None).unwrap();
    let attributed_total = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .sum::<i64>();

    assert_eq!(attributed_total, 200);
}

#[test]
fn missing_fork_parent_uses_the_rewritten_burst_fallback() {
    let fixture = Fixture::new(session_meta());
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"missing-parent\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-01-02T03:05:00.100Z", 100, 100),
            token_count_values("2026-01-02T03:05:00.200Z", 300, 200),
            token_count_values("2026-01-02T03:05:08Z", 350, 50),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 350);

    let batch = fixture.provider().scan(None).unwrap();
    let child_deltas = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record)
                if record
                    .session
                    .as_ref()
                    .is_some_and(|id| id.as_str().ends_with(":session:thread-2")) =>
            {
                match &record.data {
                    RecordData::Usage(usage) => Some(usage.delta.as_ref().map(|usage| usage.total)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(child_deltas, [None, None, Some(50)]);
    assert!(batch
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "codex.usage_replay.parent_missing"));
}

#[test]
fn compacted_fork_prefix_falls_back_to_the_rewritten_usage_burst() {
    let fixture = Fixture::new(&format!(
        "{}{}{}{}",
        session_meta(),
        token_count_values("2026-01-02T03:04:01Z", 100, 100),
        token_count_values("2026-01-02T03:04:02Z", 300, 200),
        token_count_values("2026-01-02T03:04:03Z", 600, 300),
    ));
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    fs::write(
        &child_path,
        format!(
            "{}{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-01-02T03:05:00.100Z", 200, 200),
            token_count_values("2026-01-02T03:05:00.200Z", 500, 300),
            token_count_values("2026-01-02T03:05:08Z", 550, 50),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &child_path, 550);

    let batch = fixture.provider().scan(None).unwrap();
    let attributed_total = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .sum::<i64>();

    assert_eq!(attributed_total, 650);
}

#[test]
fn nested_forks_compare_against_each_raw_parent_stream() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        session_meta(),
        token_count_values("2026-01-02T03:04:01Z", 100, 100),
    ));
    let parent_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-2.jsonl");
    let child_path = fixture
        .source
        .codex_home()
        .join("sessions/2026/01/02/rollout-thread-3.jsonl");
    fs::write(
        &parent_path,
        format!(
            "{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:05:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-2\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-01-02T03:05:00Z", 100, 100),
            token_count_values("2026-01-02T03:06:00Z", 150, 50),
        ),
    )
    .unwrap();
    fs::write(
        &child_path,
        format!(
            "{}{}{}{}",
            "{\"timestamp\":\"2026-01-02T03:07:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-3\",\"forked_from_id\":\"thread-2\",\"cwd\":\"/workspace/project\"}}\n",
            token_count_values("2026-01-02T03:07:00Z", 100, 100),
            token_count_values("2026-01-02T03:07:00Z", 150, 50),
            token_count_values("2026-01-02T03:08:00Z", 175, 25),
        ),
    )
    .unwrap();
    insert_thread(&fixture, "thread-2", &parent_path, 150);
    insert_thread(&fixture, "thread-3", &child_path, 175);

    let batch = fixture.provider().scan(None).unwrap();
    let attributed_total = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Usage(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .sum::<i64>();

    assert_eq!(attributed_total, 175);
}

#[test]
fn a_session_that_names_itself_as_parent_keeps_its_usage() {
    let fixture = Fixture::new(&format!(
        "{}{}",
        "{\"timestamp\":\"2026-01-02T03:04:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"forked_from_id\":\"thread-1\",\"cwd\":\"/workspace/project\"}}\n",
        token_count_values("2026-01-02T03:04:01Z", 100, 100),
    ));

    let batch = fixture.provider().scan(None).unwrap();
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Usage(usage)
                        if usage.delta.as_ref().map(|usage| usage.total) == Some(100)
                )
        )
    }));
}

#[cfg(unix)]
#[test]
fn unchanged_rollout_catalog_uses_the_metadata_only_fast_path() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new(&format!("{}{}", session_meta(), token_count()));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let original_permissions = fs::metadata(&fixture.rollout_path).unwrap().permissions();
    let mut blocked_permissions = original_permissions.clone();
    blocked_permissions.set_mode(0o000);
    fs::set_permissions(&fixture.rollout_path, blocked_permissions).unwrap();

    let result = provider.scan(Some(&first.checkpoint));

    fs::set_permissions(&fixture.rollout_path, original_permissions).unwrap();
    let second = result.unwrap();
    assert!(second.changes.is_empty());
    assert!(second.diagnostics.is_empty());
    assert!(!second.has_more);
}

#[test]
fn append_scan_converges_with_a_fresh_full_scan() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), turn_started()));
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let mut incremental_records = HashMap::new();
    apply_changes(&mut incremental_records, &first.changes);

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(
            format!(
                "{}{}{}{}{}",
                token_count(),
                tool_call(),
                tool_result(),
                message(),
                turn_completed()
            )
            .as_bytes(),
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(!second.has_more);
    apply_changes(&mut incremental_records, &second.changes);

    let fresh = provider.scan(None).unwrap();
    assert!(!fresh.has_more);
    let mut full_records = HashMap::new();
    apply_changes(&mut full_records, &fresh.changes);

    assert_eq!(incremental_records, full_records);
}

#[test]
#[cfg(feature = "codex-watch")]
fn subscription_emits_incremental_changes() {
    let fixture = Fixture::new(&format!("{}{}", session_meta(), turn_started()));
    let provider = fixture.provider().with_watch_options(CodexWatchOptions {
        debounce: Duration::from_millis(20),
        reconcile_interval: Duration::from_millis(50),
    });
    let initial = provider.scan(None).unwrap();
    let subscription = provider.watch(initial.checkpoint).unwrap();

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(
            b"{\"timestamp\":\"2026-01-02T03:04:08Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"turn-1\",\"completed_at\":1704164648}}\n",
        )
        .unwrap();

    let batch = subscription
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .expect("the subscription should emit a batch");
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Turn(turn) if turn.status == TurnStatus::Completed
                )
        )
    }));
}

#[test]
fn provider_info_is_stable_and_source_scoped() {
    let first = Fixture::new(session_meta()).provider();
    let second = Fixture::new(session_meta()).provider();

    assert_eq!(first.info().id.as_str(), "codex");
    assert_eq!(first.info().name, "Codex");
    assert_ne!(first.info().source, second.info().source);
    let model_invocation = first
        .coverage()
        .iter()
        .find(|coverage| coverage.capability == "model_invocation")
        .expect("model invocation coverage");
    assert_eq!(model_invocation.source, SourceCoverage::NotPersisted);
    assert_eq!(model_invocation.adapter, AdapterCoverage::NotApplicable);
    let world_state = first
        .coverage()
        .iter()
        .find(|coverage| coverage.capability == "world_state")
        .expect("world-state coverage");
    assert_eq!(world_state.source, SourceCoverage::Persisted);
    assert_eq!(world_state.adapter, AdapterCoverage::Normalized);
}
