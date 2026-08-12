#![cfg(feature = "claude-code")]

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(feature = "claude-code-watch")]
use std::time::Duration;

#[cfg(feature = "claude-code-watch")]
use coding_agent_data::providers::claude_code::ClaudeCodeWatchOptions;
use coding_agent_data::providers::claude_code::{ClaudeCodeProvider, ClaudeCodeSource};
#[cfg(feature = "claude-code-watch")]
use coding_agent_data::WatchProvider;
use coding_agent_data::{
    Actor, AdapterCoverage, AgentInvocationStatus, ApprovalPolicy, Change, ContentBlock, EventData,
    FileChangeKind, HookStatus, MessageRole, ModeChangeKind, ModelInvocationStatus, NoticeLevel,
    Provider, QueueOperation, ReasoningVisibility, Record, RecordData, RecordId,
    SessionRelationKind, SourceCoverage, SourceLocation, StopReason, Timestamp, ToolKind,
    ToolStatus,
};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    source: ClaudeCodeSource,
    main_transcript: std::path::PathBuf,
    subagent_transcript: std::path::PathBuf,
}

impl Fixture {
    fn new(main_contents: &str, subagent_contents: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().join(".claude");
        let project_dir = config_dir.join("projects/-workspace-project");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("session-1.jsonl"), main_contents).unwrap();

        let subagents_dir = project_dir.join("session-1/subagents");
        fs::create_dir_all(&subagents_dir).unwrap();
        let subagent_transcript = subagents_dir.join("agent-worker.jsonl");
        fs::write(&subagent_transcript, subagent_contents).unwrap();

        let ignored_dir = project_dir.join("session-1/tool-results");
        fs::create_dir_all(&ignored_dir).unwrap();
        fs::write(
            ignored_dir.join("tool-output.jsonl"),
            "{\"type\":\"must-not-be-read\"}\n",
        )
        .unwrap();

        let source = ClaudeCodeSource::new(&config_dir);
        let main_transcript = source
            .projects_dir()
            .join("-workspace-project/session-1.jsonl");
        Self {
            source,
            _directory: directory,
            main_transcript,
            subagent_transcript,
        }
    }

    fn provider(&self) -> ClaudeCodeProvider {
        ClaudeCodeProvider::new(self.source.clone())
    }
}

fn user_entry(timestamp: &str) -> String {
    format!(
        "{{\"type\":\"user\",\"uuid\":\"user-1\",\"sessionId\":\"session-1\",\"cwd\":\"/workspace/project\",\"gitBranch\":\"main\",\"timestamp\":\"{timestamp}\",\"message\":{{\"role\":\"user\",\"content\":\"hello\"}}}}\n"
    )
}

fn assistant_entry(timestamp: &str) -> String {
    format!(
        "{{\"type\":\"assistant\",\"uuid\":\"assistant-1\",\"sessionId\":\"session-1\",\"cwd\":\"/workspace/project\",\"timestamp\":\"{timestamp}\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"done\"}}]}}}}\n"
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

#[test]
fn scan_normalizes_session_messages_and_subagent_entries() {
    let main = format!(
        "{}{}{}",
        user_entry("2026-01-02T03:04:05Z"),
        assistant_entry("2026-01-02T03:04:06Z"),
        "{\"type\":\"custom-title\",\"sessionId\":\"session-1\",\"customTitle\":\"Synthetic session\"}\n"
    );
    let fixture = Fixture::new(
        &main,
        "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"agentId\":\"worker\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"message\":{\"role\":\"assistant\",\"content\":[]}}\n",
    );
    let provider = fixture.provider();

    let batch = provider.scan(None).unwrap();
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.external_id == "session-1"
                            && session.title.as_deref() == Some("Synthetic session")
                            && session.cwd.as_deref()
                                == Some(std::path::Path::new("/workspace/project"))
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
                            record.data,
                            RecordData::Event(ref event)
                                if matches!(&event.data, EventData::Message(_))
                        )
                )
            })
            .count(),
        2
    );
    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record
                    .original
                    .as_ref()
                    .is_some_and(|original| original.value["type"] == "must-not-be-read")
        )
    }));
}

#[test]
fn untyped_and_custom_content_are_preserved_as_unknown() {
    let fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"unknown-content\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"_chart\",\"series\":[1]},{\"series\":[2]}]}}\n",
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();
    let content = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Event(event) => match &event.data {
                    EventData::Message(message) if message.content.len() == 2 => {
                        Some(&message.content)
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .expect("message with unknown content");

    assert!(matches!(
        &content[0],
        ContentBlock::Unknown {
            kind: Some(kind),
            value,
        } if kind == "_chart" && value["series"][0] == 1
    ));
    assert!(matches!(
        &content[1],
        ContentBlock::Unknown {
            kind: None,
            value,
        } if value["series"][0] == 2
    ));
}

#[test]
fn tool_blocks_are_normalized_without_exposing_transcript_fields() {
    let fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"assistant-tools\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"inspect first\"},{\"type\":\"tool_use\",\"id\":\"tool-1\",\"name\":\"Read\",\"input\":{\"path\":\"SKILL.md\"}}],\"usage\":{\"input_tokens\":12,\"cache_creation_input_tokens\":4,\"cache_read_input_tokens\":2,\"output_tokens\":3}}}\n{\"type\":\"user\",\"uuid\":\"tool-output\",\"parentUuid\":\"assistant-tools\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"toolUseResult\":{\"stdout\":\"ok\",\"totalDurationMs\":42},\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"tool-1\",\"content\":\"ok\"}]}}\n",
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ToolCall(call)
                                if call.call_id == "tool-1" && call.name == "Read"
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
                    RecordData::Event(event)
                        if event.actor == Actor::Agent
                            && matches!(
                                &event.data,
                                EventData::Reasoning(reasoning)
                                    if reasoning.content
                                        == vec![ContentBlock::text("inspect first")]
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
                    RecordData::Event(event)
                        if event.actor == Actor::Tool
                            && event.parent.is_some()
                            && matches!(
                                &event.data,
                                EventData::ToolResult(result)
                                    if result.status == ToolStatus::Completed
                                        && result.content
                                            == vec![ContentBlock::text("ok")]
                                        && result.output["stdout"] == "ok"
                                        && result.duration_ms == Some(42)
                            )
                )
        )
    }));
    assert!(!batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::Message(message)
                                if message.role == MessageRole::Assistant
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ToolResult(result)
                                if result.call_id == "tool-1"
                                    && result.status == ToolStatus::Completed
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
                    RecordData::UsageReport(usage)
                        if usage.cumulative.is_none()
                            && usage.delta.as_ref().map(|usage| usage.total) == Some(21)
                            && usage
                                .delta
                                .as_ref()
                                .and_then(|usage| usage.cache_creation_input)
                                == Some(4)
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session) if session.total_tokens.is_none()
                )
        )
    }));
}

#[test]
fn transcript_turns_model_requests_and_file_changes_share_normalized_identity() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"user-1\",\"promptId\":\"prompt-1\",\"sessionId\":\"session-1\",\"cwd\":\"/workspace/project\",\"gitBranch\":\"main\",\"permissionMode\":\"default\",\"version\":\"2.1.220\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"user\",\"content\":\"update the file\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"assistant-tools\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"message-1\",\"role\":\"assistant\",\"model\":\"claude-test\",\"stop_reason\":\"tool_use\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"inspect first\"},{\"type\":\"redacted_thinking\",\"data\":\"opaque\"},{\"type\":\"tool_use\",\"id\":\"tool-1\",\"name\":\"mcp__filesystem__edit_file\",\"input\":{\"file_path\":\"src/lib.rs\",\"old_string\":\"old\",\"new_string\":\"new\"}}],\"usage\":{\"input_tokens\":12,\"output_tokens\":3}}}\n",
            "{\"type\":\"user\",\"uuid\":\"tool-output\",\"parentUuid\":\"assistant-tools\",\"sessionId\":\"session-1\",\"cwd\":\"/workspace/project\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"toolUseResult\":{\"filePath\":\"src/lib.rs\",\"structuredPatch\":[{\"oldStart\":1,\"newStart\":1}],\"durationMs\":42},\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"tool-1\",\"content\":\"updated\"}]}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"assistant-final\",\"requestId\":\"request-2\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:08Z\",\"message\":{\"id\":\"message-2\",\"role\":\"assistant\",\"model\":\"claude-test\",\"stop_reason\":\"end_turn\",\"content\":[{\"type\":\"text\",\"text\":\"done\"}],\"usage\":{\"input_tokens\":20,\"output_tokens\":4}}}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();
    let mut event_sequences = HashSet::new();
    for record in batch.changes.iter().filter_map(|change| match change {
        Change::Upsert(record) if matches!(record.data, RecordData::Event(_)) => Some(record),
        _ => None,
    }) {
        let RecordData::Event(event) = &record.data else {
            unreachable!("filtered to event records");
        };
        assert!(
            event_sequences.insert((
                record.origin.path.clone(),
                event.sequence.position,
                event.sequence.part,
            )),
            "derived items at one source position must have unique sequence parts"
        );
    }
    let completed_turn = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::AgentInvocation(execution)
                    if execution.status == AgentInvocationStatus::Completed =>
                {
                    Some((record.id.clone(), execution))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("completed Claude Code turn");

    assert_eq!(completed_turn.1.invocation_id, "prompt-1");
    assert_eq!(
        completed_turn.1.stop_reason,
        Some(StopReason::EndInvocation)
    );
    assert_eq!(completed_turn.1.duration_ms, Some(3_000));

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.invocation.as_ref() == Some(&completed_turn.0)
                    && matches!(
                        &record.data,
                        RecordData::Event(event)
                            if matches!(
                                &event.data,
                                EventData::ModelInvocation(invocation)
                                    if invocation.provider.as_deref() == Some("anthropic")
                                        && invocation.model.as_deref() == Some("claude-test")
                                        && invocation.status == ModelInvocationStatus::Completed
                                        && invocation.stop_reason == Some(StopReason::ToolUse)
                            )
                    )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.invocation.as_ref() == Some(&completed_turn.0)
                    && matches!(
                        &record.data,
                        RecordData::Event(event)
                            if matches!(
                                &event.data,
                                EventData::ToolCall(call)
                                    if call.name == "edit_file"
                                        && call.namespace.as_deref() == Some("filesystem")
                                        && call.kind == ToolKind::Edit
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
                if record.invocation.as_ref() == Some(&completed_turn.0)
                    && matches!(
                        &record.data,
                        RecordData::Event(event)
                            if matches!(
                                &event.data,
                                EventData::FileChange(change)
                                    if change.path
                                        == std::path::Path::new("src/lib.rs")
                                        && change.kind == FileChangeKind::Update
                                        && change.status == ToolStatus::Completed
                                        && change.diff.is_some()
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::Reasoning(reasoning)
                                if reasoning.visibility == ReasoningVisibility::Redacted
                                    && reasoning.content.is_empty()
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ExecutionContext(context)
                                if context.cwd.as_deref()
                                    == Some(std::path::Path::new("/workspace/project"))
                                    && context.approval_policy
                                        == Some(ApprovalPolicy::OnRequest)
                                    && context.sandbox_policy.is_none()
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
                    RecordData::Session(session)
                        if session.agent_version.as_deref() == Some("2.1.220")
                            && session.model_provider.as_deref() == Some("anthropic")
                )
        )
    }));
}

#[test]
fn mode_and_external_file_edit_entries_are_normalized() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"mode\",\"mode\":\"normal\",\"sessionId\":\"session-1\"}\n",
            "{\"type\":\"attachment\",\"uuid\":\"attachment-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"attachment\":{\"type\":\"edited_text_file\",\"filename\":\"src/lib.rs\",\"snippet\":\"changed\"}}\n",
            "{\"type\":\"attachment\",\"uuid\":\"attachment-2\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-03T03:04:05Z\",\"attachment\":{\"type\":\"date_change\",\"newDate\":\"2026-01-03\"}}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ModeChange(change)
                                if change.mode == "normal"
                                    && change.kind == ModeChangeKind::Selected
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::FileChange(change)
                                if change.path
                                    == std::path::Path::new("src/lib.rs")
                                    && change.kind == FileChangeKind::Update
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ExecutionContext(context)
                                if context.current_date.as_deref() == Some("2026-01-03")
                        )
                )
        )
    }));
}

#[test]
fn provider_state_attachments_and_progress_entries_are_normalized() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"attachment\",\"uuid\":\"task-reminder\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"attachment\":{\"type\":\"task_reminder\",\"content\":\"Two tasks remain\",\"itemCount\":2}}\n",
            "{\"type\":\"attachment\",\"uuid\":\"skill-listing\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"attachment\":{\"type\":\"skill_listing\",\"names\":[\"review\"],\"skillCount\":1,\"isInitial\":true}}\n",
            "{\"type\":\"attachment\",\"uuid\":\"agent-listing\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"attachment\":{\"type\":\"agent_listing_delta\",\"addedTypes\":[\"worker\"],\"removedTypes\":[]}}\n",
            "{\"type\":\"attachment\",\"uuid\":\"permissions\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:08Z\",\"attachment\":{\"type\":\"command_permissions\",\"allowedTools\":[\"Read\",\"Bash(git status:*)\"]}}\n",
            "{\"type\":\"system\",\"subtype\":\"hook_started\",\"uuid\":\"hook-started\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:09Z\",\"message\":\"running hook\"}\n",
            "{\"type\":\"tool_progress\",\"uuid\":\"tool-progress\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:10Z\",\"toolUseID\":\"tool-1\",\"message\":\"reading file\"}\n",
            "{\"type\":\"progress\",\"uuid\":\"agent-progress\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:11Z\",\"data\":{\"type\":\"agent_progress\",\"description\":\"delegating work\"}}\n",
            "{\"type\":\"system\",\"subtype\":\"hook_response\",\"uuid\":\"hook-response\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:12Z\",\"hookEvent\":\"PreToolUse\",\"toolUseID\":\"tool-1\",\"response\":{\"outcome\":\"failed\",\"stderr\":\"lint failed\"}}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    for (code, message) in [
        ("task_reminder", "Two tasks remain"),
        ("hook_started", "running hook"),
        ("tool_progress", "reading file"),
        ("agent_progress", "delegating work"),
    ] {
        assert!(batch.changes.iter().any(|change| {
            matches!(
                change,
                Change::Upsert(record)
                    if matches!(
                        &record.data,
                        RecordData::Event(event)
                            if matches!(
                                &event.data,
                                EventData::Notice(notice)
                                    if notice.level == NoticeLevel::Info
                                        && notice.code.as_deref() == Some(code)
                                        && notice.message == message
                            )
                    )
            )
        }));
    }
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ExecutionContext(context)
                                if context
                                    .provider_attributes
                                    .get("skill_listing")
                                    .is_some_and(|listing| listing["names"][0] == "review")
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ExecutionContext(context)
                                if context
                                    .provider_attributes
                                    .get("agent_listing_delta")
                                    .is_some_and(
                                        |listing| listing["addedTypes"][0] == "worker"
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::ExecutionContext(context)
                                if context.permission_profile.as_ref().is_some_and(
                                    |profile| profile[0] == "Read" && profile[1]
                                        == "Bash(git status:*)"
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
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::HookResult(result)
                                if result.event.as_deref() == Some("PreToolUse")
                                    && result.tool_call_id.as_deref() == Some("tool-1")
                                    && result.status == HookStatus::Failed
                                    && result.context.iter().any(
                                        |block| matches!(
                                            block,
                                            ContentBlock::Text { text, .. }
                                                if text == "lint failed"
                                        )
                                    )
                        )
                )
        )
    }));
}

#[test]
fn stop_hook_summaries_are_normalized_as_hook_results() {
    let fixture = Fixture::new(
        &format!(
            "{}{}",
            user_entry("2026-01-02T03:04:05Z"),
            "{\"type\":\"system\",\"subtype\":\"stop_hook_summary\",\"uuid\":\"hook-summary-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"entrypoint\":\"claude-desktop-3p\",\"toolUseID\":\"tool-1\",\"hookCount\":2,\"preventedContinuation\":true,\"stopReason\":\"blocked\",\"hookInfos\":[{\"command\":\"lint\"}],\"hookErrors\":[{\"command\":\"test\",\"error\":\"failed\"}],\"hookAdditionalContext\":[{\"type\":\"text\",\"text\":\"fix tests\"}]}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::HookResult(result)
                                if result.event.as_deref() == Some("stop")
                                    && result.entrypoint.as_deref()
                                        == Some("claude-desktop-3p")
                                    && result.tool_call_id.as_deref() == Some("tool-1")
                                    && result.count == Some(2)
                                    && result.status == HookStatus::Blocked
                                    && result.infos.len() == 1
                                    && result.errors.len() == 1
                                    && result.context
                                        == vec![ContentBlock::text("fix tests")]
                        )
                )
        )
    }));
}

#[test]
fn agent_tool_calls_are_also_exposed_as_agent_invocations() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"user-1\",\"promptId\":\"prompt-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"user\",\"content\":\"delegate\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"assistant-tools\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"role\":\"assistant\",\"model\":\"claude-test\",\"stop_reason\":\"tool_use\",\"content\":[{\"type\":\"tool_use\",\"id\":\"agent-1\",\"name\":\"Agent\",\"input\":{\"subagent_type\":\"reviewer\",\"prompt\":\"review this\"}}]}}\n",
            "{\"type\":\"user\",\"uuid\":\"agent-output\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"toolUseResult\":{\"agentId\":\"worker-1\",\"output\":\"looks good\"},\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"agent-1\",\"content\":\"looks good\"}]}}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::AgentInvocation(invocation)
                                if invocation.invocation_id == "agent-1"
                                    && invocation.task_id.as_deref() == Some("worker-1")
                                    && invocation.receiver_ids == ["reviewer"]
                                    && invocation.status == AgentInvocationStatus::Completed
                        )
                )
        )
    }));
}

#[test]
fn queue_operations_and_queued_commands_preserve_task_and_child_session_linkage() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"user-1\",\"promptId\":\"prompt-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"user\",\"content\":\"delegate\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"assistant-tools\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"role\":\"assistant\",\"model\":\"claude-test\",\"stop_reason\":\"tool_use\",\"content\":[{\"type\":\"tool_use\",\"id\":\"agent-1\",\"name\":\"Agent\",\"input\":{\"subagent_type\":\"reviewer\",\"prompt\":\"review this\"}}]}}\n",
            "{\"type\":\"user\",\"uuid\":\"agent-output\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"toolUseResult\":{\"status\":\"completed\",\"output\":\"looks good\"},\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"agent-1\",\"content\":\"looks good\"}]}}\n"
        ),
        "",
    );
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let initial_invocation = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Event(event) => match &event.data {
                    EventData::AgentInvocation(invocation)
                        if invocation.status == AgentInvocationStatus::Completed =>
                    {
                        Some((record.id.clone(), event.sequence))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .expect("terminal agent invocation");

    OpenOptions::new()
        .append(true)
        .open(&fixture.main_transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"queue-operation\",\"operation\":\"enqueue\",\"timestamp\":\"2026-01-02T03:04:08Z\",\"content\":\"{\\\"task_id\\\":\\\"worker\\\",\\\"tool_use_id\\\":\\\"agent-1\\\",\\\"task_type\\\":\\\"local_agent\\\",\\\"status\\\":\\\"completed\\\"}\"}\n",
                "{\"type\":\"queue-operation\",\"operation\":\"dequeue\",\"timestamp\":\"2026-01-02T03:04:08.500Z\",\"content\":\"<task-notification><task-id>worker</task-id><tool-use-id>agent-1</tool-use-id><task-type>local_agent</task-type><status>completed</status></task-notification>\"}\n",
                "{\"type\":\"attachment\",\"timestamp\":\"2026-01-02T03:04:09Z\",\"attachment\":{\"type\":\"queued_command\",\"prompt\":\"please follow up\"}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::InputQueue(queue)
                                if queue.operation == QueueOperation::Enqueue
                                    && queue.task_id.as_deref() == Some("worker")
                                    && queue.tool_call_id.as_deref() == Some("agent-1")
                                    && queue.task_type.as_deref() == Some("local_agent")
                                    && queue.status.as_deref() == Some("completed")
                        )
                )
        )
    }));
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::InputQueue(queue)
                                if queue.operation == QueueOperation::Dequeue
                                    && queue.task_id.as_deref() == Some("worker")
                                    && queue.tool_call_id.as_deref() == Some("agent-1")
                        )
                )
        )
    }));
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record.id == initial_invocation.0
                    && matches!(
                        &record.data,
                        RecordData::Event(event)
                            if event.sequence == initial_invocation.1
                                && matches!(
                                    &event.data,
                                    EventData::AgentInvocation(invocation)
                                        if invocation.status
                                            == AgentInvocationStatus::Completed
                                            && invocation.task_id.as_deref() == Some("worker")
                                            && invocation.child_session.is_some()
                                            && invocation.output.is_some()
                                )
                    )
        )
    }));
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if event.actor == Actor::User
                            && matches!(
                                &event.data,
                                EventData::Message(message)
                                    if message.role == MessageRole::User
                                        && message.content
                                            == vec![ContentBlock::text("please follow up")]
                            )
                )
        )
    }));
    assert!(!second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if record
                    .original
                    .as_ref()
                    .is_some_and(|original| original.value["type"] == "queue-operation")
                    && matches!(
                        &record.data,
                        RecordData::Event(event)
                            if matches!(&event.data, EventData::Message(_))
                    )
        )
    }));
}

#[test]
fn fragmented_usage_is_upserted_by_request_without_double_counting() {
    let first_fragment = "{\"type\":\"assistant\",\"uuid\":\"fragment-1\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"id\":\"message-1\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n";
    let final_fragment = "{\"type\":\"assistant\",\"uuid\":\"fragment-2\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"message-1\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":10,\"cache_creation\":{\"input_tokens\":5},\"cache_read_input_tokens\":3,\"output_tokens\":2}}}\n";
    let fixture = Fixture::new(first_fragment, "");
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    let (first_usage_id, first_total) = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::UsageReport(usage) => Some((
                    record.id.clone(),
                    usage.delta.as_ref().map(|usage| usage.total),
                )),
                _ => None,
            },
            _ => None,
        })
        .expect("the first fragment should emit usage");
    assert_eq!(first_total, Some(11));

    OpenOptions::new()
        .append(true)
        .open(&fixture.main_transcript)
        .unwrap()
        .write_all(format!("{final_fragment}{final_fragment}").as_bytes())
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    let usage_records: Vec<_> = second
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) if matches!(record.data, RecordData::UsageReport(_)) => {
                Some(record)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        usage_records.len(),
        1,
        "an identical terminal fragment must not emit another usage upsert"
    );
    assert_eq!(usage_records[0].id, first_usage_id);
    assert!(matches!(
        &usage_records[0].data,
        RecordData::UsageReport(usage)
            if usage.delta.as_ref().map(|usage| usage.total) == Some(20)
                && usage
                    .delta
                    .as_ref()
                    .and_then(|usage| usage.cache_creation_input)
                    == Some(5)
    ));
}

#[test]
fn sidechain_usage_replay_is_deduplicated_across_transcripts() {
    let fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"parent-entry\",\"requestId\":\"parent-request\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":10}}}\n",
        concat!(
            "{\"type\":\"assistant\",\"uuid\":\"replayed-entry\",\"requestId\":\"sidechain-request\",\"sessionId\":\"session-1\",\"isSidechain\":true,\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"cache_read_input_tokens\":500,\"output_tokens\":10}}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"sidechain-answer\",\"requestId\":\"answer-request\",\"sessionId\":\"session-1\",\"isSidechain\":true,\"timestamp\":\"2026-01-02T03:04:07Z\",\"message\":{\"id\":\"sidechain-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":600,\"output_tokens\":100}}}\n"
        ),
    );

    let batch = fixture.provider().scan(None).unwrap();
    let mut totals: Vec<_> = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::UsageReport(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .collect();
    totals.sort_unstable();

    assert_eq!(totals, vec![30, 700]);
}

#[test]
fn distinct_primary_requests_with_the_same_message_id_are_preserved() {
    let fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"parent-entry\",\"requestId\":\"parent-request\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":10}}}\n",
        "{\"type\":\"assistant\",\"uuid\":\"child-entry\",\"requestId\":\"child-request\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":30,\"output_tokens\":10}}}\n",
    );

    let batch = fixture.provider().scan(None).unwrap();
    let mut totals: Vec<_> = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::UsageReport(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .collect();
    totals.sort_unstable();

    assert_eq!(totals, vec![30, 40]);
}

#[test]
fn distinct_message_ids_with_the_same_request_id_are_preserved() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"assistant\",\"uuid\":\"entry-1\",\"requestId\":\"shared-request\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"id\":\"message-1\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"entry-2\",\"requestId\":\"shared-request\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"message-2\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":5,\"output_tokens\":2}}}\n"
        ),
        "",
    );

    let batch = fixture.provider().scan(None).unwrap();
    let mut totals: Vec<_> = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::UsageReport(usage) => usage.delta.as_ref().map(|usage| usage.total),
                _ => None,
            },
            _ => None,
        })
        .collect();
    totals.sort_unstable();

    assert_eq!(totals, vec![3, 7]);
    let model_invocations: Vec<_> = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Event(event)
                    if matches!(&event.data, EventData::ModelInvocation(_)) =>
                {
                    Some(record.id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(model_invocations.len(), 2);
    assert_ne!(model_invocations[0], model_invocations[1]);
}

#[test]
fn api_error_messages_fail_the_model_and_agent_invocations() {
    for stop_reason in [
        ",\"stop_reason\":\"stop_sequence\"",
        ",\"stop_reason\":\"end_turn\"",
        "",
    ] {
        let fixture = Fixture::new(
            &format!(
                "{}{{\"type\":\"assistant\",\"uuid\":\"assistant-error\",\"requestId\":\"request-error\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"isApiErrorMessage\":true,\"error\":{{\"message\":\"upstream unavailable\"}},\"message\":{{\"id\":\"message-error\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"API Error\"}}]{stop_reason}}}}}\n",
                user_entry("2026-01-02T03:04:05Z"),
            ),
            "",
        );
        let batch = fixture.provider().scan(None).unwrap();

        assert!(batch.changes.iter().any(|change| {
            matches!(
                change,
                Change::Upsert(record)
                    if matches!(
                        &record.data,
                        RecordData::Event(event)
                            if matches!(
                                &event.data,
                                EventData::ModelInvocation(invocation)
                                    if invocation.status == ModelInvocationStatus::Failed
                                        && invocation.stop_reason == Some(StopReason::Failed)
                                        && invocation.error.as_deref()
                                            == Some("upstream unavailable")
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
                        RecordData::AgentInvocation(invocation)
                            if invocation.status == AgentInvocationStatus::Failed
                                && invocation.stop_reason == Some(StopReason::Failed)
                                && invocation.error.as_deref() == Some("upstream unavailable")
                    )
            )
        }));
        assert!(!batch.changes.iter().any(|change| {
            matches!(
                change,
                Change::Upsert(record)
                    if matches!(
                        &record.data,
                        RecordData::AgentInvocation(invocation)
                            if invocation.status == AgentInvocationStatus::Completed
                    )
            )
        }));
    }
}

#[test]
fn tool_and_reasoning_only_entries_do_not_emit_empty_messages() {
    let fixture = Fixture::new(
        &format!(
            "{}{}{}",
            user_entry("2026-01-02T03:04:05Z"),
            "{\"type\":\"assistant\",\"uuid\":\"assistant-tools\",\"requestId\":\"request-tools\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"message-tools\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"inspect first\"},{\"type\":\"tool_use\",\"id\":\"tool-1\",\"name\":\"Read\",\"input\":{\"file_path\":\"src/lib.rs\"}}],\"stop_reason\":\"tool_use\"}}\n",
            "{\"type\":\"user\",\"uuid\":\"tool-result-entry\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"tool-1\",\"content\":\"file contents\"}]}}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    let messages: Vec<_> = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Event(event) => match &event.data {
                    EventData::Message(message) => Some(message),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1);
    assert!(messages.iter().all(|message| !message.content.is_empty()));

    let model_record = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(&event.data, EventData::ModelInvocation(_))
                ) =>
            {
                Some(record)
            }
            _ => None,
        })
        .expect("model invocation");
    for event in batch.changes.iter().filter_map(|change| match change {
        Change::Upsert(record) => match &record.data {
            RecordData::Event(event)
                if matches!(
                    &event.data,
                    EventData::ToolCall(_) | EventData::Reasoning(_)
                ) =>
            {
                Some(event)
            }
            _ => None,
        },
        _ => None,
    }) {
        assert_eq!(event.parent.as_ref(), Some(&model_record.id));
    }
}

#[test]
fn nested_agent_progress_emits_usage_with_provider_metadata() {
    let fixture = Fixture::new(
        "{\"type\":\"progress\",\"sessionId\":\"session-1\",\"timestamp\":\"1970-01-01T00:00:02Z\",\"data\":{\"message\":{\"type\":\"assistant\",\"uuid\":\"nested-entry\",\"requestId\":\"nested-request\",\"timestamp\":\"1970-01-01T00:00:01Z\",\"costUSD\":0.125,\"message\":{\"id\":\"nested-message\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":3,\"output_tokens\":4,\"cache_creation\":{\"ephemeral_5m_input_tokens\":5,\"ephemeral_1h_input_tokens\":7},\"cache_read_input_tokens\":11,\"service_tier\":\"standard\",\"speed\":\"fast\"}}}}}\n",
        "",
    );

    let batch = fixture.provider().scan(None).unwrap();
    let usage_record = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) if matches!(record.data, RecordData::UsageReport(_)) => {
                Some(record)
            }
            _ => None,
        })
        .expect("nested progress usage");

    assert_eq!(usage_record.timestamp, Some(Timestamp::from_seconds(1)));
    assert!(matches!(
        &usage_record.data,
        RecordData::UsageReport(usage)
            if usage.model.as_deref() == Some("claude-test")
                && usage.service_tier.as_deref() == Some("standard")
                && usage.request_id.as_deref() == Some("nested-request")
                && usage.invocation_id.as_deref() == Some("nested-message")
                && usage.delta.as_ref().is_some_and(|tokens|
                    tokens.total == 30
                        && tokens.input == Some(3)
                        && tokens.cache_creation_input == Some(12)
                        && tokens.cache_creation_ephemeral_5m_input == Some(5)
                        && tokens.cache_creation_ephemeral_1h_input == Some(7)
                        && tokens.cached_input == Some(11)
                        && tokens.output == Some(4))
                && usage.cost.as_ref().is_some_and(|cost|
                    cost.amount == "0.125" && cost.currency == "USD")
    ));
}

#[test]
fn advisor_iterations_emit_independent_usage() {
    let fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"assistant-entry\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-05-22T02:34:40Z\",\"costUSD\":1.23,\"message\":{\"id\":\"message-1\",\"model\":\"main-model\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"iterations\":[{\"type\":\"message\",\"model\":\"main-model\",\"input_tokens\":1,\"output_tokens\":2},{\"type\":\"advisor_message\",\"model\":\"advisor-model\",\"input_tokens\":10,\"output_tokens\":2,\"cache_creation_input_tokens\":3,\"cache_read_input_tokens\":4}]}}}\n",
        "",
    );

    let batch = fixture.provider().scan(None).unwrap();
    let mut usages: Vec<_> = batch
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::UsageReport(usage) => Some((
                    usage.model.as_deref(),
                    usage.delta.as_ref().map(|tokens| tokens.total),
                    usage.cost.as_ref().map(|cost| cost.amount.as_str()),
                )),
                _ => None,
            },
            _ => None,
        })
        .collect();
    usages.sort_unstable();

    assert_eq!(
        usages,
        vec![
            (Some("advisor-model"), Some(19), None),
            (Some("main-model"), Some(3), Some("1.23")),
        ]
    );
}

#[test]
fn negative_usage_components_do_not_reduce_token_totals() {
    let fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"assistant-entry\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"id\":\"message-1\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"total_tokens\":-100,\"input_tokens\":-20,\"cache_creation_input_tokens\":-10,\"cache_read_input_tokens\":3,\"output_tokens\":2}}}\n",
        "",
    );

    let batch = fixture.provider().scan(None).unwrap();
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::UsageReport(usage)
                        if usage.delta.as_ref().is_some_and(|tokens|
                            tokens.total == 5
                                && tokens.input.is_none()
                                && tokens.cache_creation_input.is_none()
                                && tokens.cached_input == Some(3)
                                && tokens.output == Some(2))
                )
        )
    }));
}

#[test]
fn usage_is_rebuilt_after_a_transcript_changes_sidechain_classification() {
    let replay = "{\"type\":\"assistant\",\"uuid\":\"replayed-entry\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"isSidechain\":true,\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"cache_read_input_tokens\":500,\"output_tokens\":10}}}\n";
    let primary = "{\"type\":\"assistant\",\"uuid\":\"primary-entry\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":10}}}\n";
    let fixture = Fixture::new(replay, "");
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    let replay_id = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) if matches!(record.data, RecordData::UsageReport(_)) => {
                Some(record.id.clone())
            }
            _ => None,
        })
        .expect("sidechain usage");

    fs::write(&fixture.main_transcript, primary).unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Reset(source) if source.path == fixture.main_transcript
        )
    }));
    assert!(second.changes.iter().any(
        |change| matches!(change, Change::Upsert(record) if record.id == replay_id
        && matches!(
            &record.data,
            RecordData::UsageReport(usage)
                if usage.delta.as_ref().map(|usage| usage.total) == Some(30)
        ))
    ));
    assert!(!second
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(id) if id == &replay_id)));
}

#[test]
fn nested_sidechain_replay_is_replaced_by_primary_usage_incrementally() {
    let replay = "{\"type\":\"progress\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06.001Z\",\"data\":{\"message\":{\"type\":\"assistant\",\"uuid\":\"replayed-entry\",\"requestId\":\"sidechain-request\",\"isSidechain\":true,\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"cache_read_input_tokens\":500,\"output_tokens\":10}}}}}\n";
    let primary = "{\"type\":\"assistant\",\"uuid\":\"primary-entry\",\"requestId\":\"primary-request\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"output_tokens\":10}}}\n";
    let fixture = Fixture::new(replay, "");
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    let replay_id = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) if matches!(record.data, RecordData::UsageReport(_)) => {
                Some(record.id.clone())
            }
            _ => None,
        })
        .expect("nested sidechain usage");

    fs::write(&fixture.subagent_transcript, primary).unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    let primary_id = second
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::UsageReport(usage)
                        if usage.delta.as_ref().map(|tokens| tokens.total) == Some(30)
                ) =>
            {
                Some(record.id.clone())
            }
            _ => None,
        })
        .expect("primary usage");
    assert_ne!(primary_id, replay_id);
    assert!(second
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(id) if id == &replay_id)));

    let mut incremental_records = HashMap::new();
    apply_changes(&mut incremental_records, &first.changes);
    apply_changes(&mut incremental_records, &second.changes);
    let fresh = provider.scan(None).unwrap();
    let mut fresh_records = HashMap::new();
    apply_changes(&mut fresh_records, &fresh.changes);
    assert_eq!(incremental_records, fresh_records);
}

#[test]
fn child_session_linkage_requires_an_explicit_provider_agent_id() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"user-1\",\"promptId\":\"prompt-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"user\",\"content\":\"delegate\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"assistant-tools\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"role\":\"assistant\",\"model\":\"claude-test\",\"stop_reason\":\"tool_use\",\"content\":[{\"type\":\"tool_use\",\"id\":\"agent-1\",\"name\":\"Agent\",\"input\":{\"subagent_type\":\"reviewer\"}}]}}\n",
            "{\"type\":\"user\",\"uuid\":\"agent-output\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"toolUseResult\":{\"agentId\":\"worker-1\",\"output\":\"done\"},\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"agent-1\",\"content\":\"done\"}]}}\n"
        ),
        concat!(
            "{\"type\":\"user\",\"uuid\":\"child-user\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:08Z\",\"message\":{\"role\":\"user\",\"content\":\"work\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"child-answer\",\"sessionId\":\"session-1\",\"agentId\":\"worker-1\",\"timestamp\":\"2026-01-02T03:04:09Z\",\"message\":{\"role\":\"assistant\",\"content\":\"done\"}}\n"
        ),
    );
    let batch = fixture.provider().scan(None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::AgentInvocation(invocation)
                                if invocation.invocation_id == "agent-1"
                                    && invocation.child_session.is_some()
                        )
                )
        )
    }));
}

#[test]
fn usage_replay_record_is_removed_when_its_transcript_is_deleted() {
    let replay = "{\"type\":\"assistant\",\"uuid\":\"replayed-entry\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"isSidechain\":true,\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"id\":\"shared-message\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":20,\"cache_read_input_tokens\":500,\"output_tokens\":10}}}\n";
    let fixture = Fixture::new(replay, "");
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let replay_id = first
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::UsageReport(usage)
                    if usage.delta.as_ref().map(|usage| usage.total) == Some(530) =>
                {
                    Some(record.id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("sidechain usage");
    fs::remove_file(&fixture.main_transcript).unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(id) if id == &replay_id)));
}

#[test]
fn structural_entries_preserve_item_links_forks_and_compaction() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"attachment\",\"uuid\":\"attachment-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"attachment\":{\"filePath\":\"notes.txt\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"reply-1\",\"parentUuid\":\"attachment-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"role\":\"assistant\",\"content\":\"done\"}}\n",
            "{\"type\":\"system\",\"subtype\":\"stop_hook_summary\",\"uuid\":\"system-1\",\"parentUuid\":\"reply-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:07Z\",\"message\":null}\n",
            "{\"type\":\"system\",\"subtype\":\"compact_boundary\",\"uuid\":\"compact-1\",\"logicalParentUuid\":\"reply-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:08Z\",\"content\":\"summary\"}\n",
            "{\"type\":\"fork-context-ref\",\"sessionId\":\"session-1\",\"forkedSessionId\":\"parent-session\",\"timestamp\":\"2026-01-02T03:04:09Z\"}\n",
            "{\"type\":\"assistant\",\"uuid\":\"inherited-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-01T03:04:05Z\",\"forkedFrom\":{\"sessionId\":\"parent-session\",\"messageUuid\":\"original-message\"},\"message\":{\"id\":\"inherited-message\",\"role\":\"assistant\",\"content\":\"copied\",\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":100,\"output_tokens\":20}}}\n"
        ),
        "",
    );
    let batch = fixture.provider().scan(None).unwrap();

    let attachment = batch
        .changes
        .iter()
        .find_map(|change| match change {
            Change::Upsert(record) => match &record.data {
                RecordData::Event(event)
                    if event.external_id.as_deref() == Some("attachment-1") =>
                {
                    Some((record, event))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("attachment event");
    assert_eq!(attachment.1.actor, Actor::Environment);
    assert!(matches!(
        attachment.1.data,
        EventData::Unknown(ref unknown) if unknown.kind.as_deref() == Some("attachment")
    ));

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if event.external_id.as_deref() == Some("reply-1")
                            && event.parent.as_ref() == Some(&attachment.0.id)
                )
        )
    }));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if event.external_id.as_deref() == Some("system-1")
                            && event.parent.is_some()
                            && matches!(
                                &event.data,
                                EventData::HookResult(result)
                                    if result.status == HookStatus::Completed
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
                    RecordData::Event(event)
                        if event.external_id.as_deref() == Some("compact-1")
                            && event.parent.is_some()
                            && matches!(
                                &event.data,
                                EventData::ContextCompaction(compaction)
                                    if compaction.summary.as_deref() == Some("summary")
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
                    RecordData::Event(event)
                        if event.external_id.as_deref() == Some("inherited-1")
                            && event.inherited_from.is_some()
                )
        )
    }));
    assert!(!batch
        .changes
        .iter()
        .any(|change| matches!(change, Change::Upsert(record) if matches!(record.data, RecordData::UsageReport(_)))));
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Session(session)
                        if session.relations.iter().any(|relation| {
                            relation.kind == SessionRelationKind::Fork
                                && relation.session.as_str().contains("parent-session")
                        })
                            && session.created_at.map(|value| value.as_millis())
                                == Some(1_767_323_045_000)
                )
        )
    }));
}

#[test]
fn incremental_scan_waits_for_a_complete_line() {
    let mut partial = assistant_entry("2026-01-02T03:04:06Z");
    partial.pop();
    partial.pop();
    let fixture = Fixture::new(&user_entry("2026-01-02T03:04:05Z"), "");
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    OpenOptions::new()
        .append(true)
        .open(&fixture.main_transcript)
        .unwrap()
        .write_all(partial.as_bytes())
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second.changes.is_empty());

    OpenOptions::new()
        .append(true)
        .open(&fixture.main_transcript)
        .unwrap()
        .write_all(b"}\n")
        .unwrap();
    let third = provider.scan(Some(&second.checkpoint)).unwrap();
    assert!(third.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    record.data,
                    RecordData::Event(ref event)
                        if matches!(&event.data, EventData::Message(_))
                )
        )
    }));
}

#[test]
fn transcript_without_a_trailing_newline_is_normalized_once() {
    let mut entry = assistant_entry("2026-01-02T03:04:06Z");
    entry.pop();
    let fixture = Fixture::new(&entry, "");
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            Change::Upsert(record)
                if matches!(
                    &record.data,
                    RecordData::Event(event)
                        if matches!(
                            &event.data,
                            EventData::Message(message)
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
fn checkpoint_round_trip_is_source_scoped() {
    let first_fixture = Fixture::new(
        "{\"type\":\"assistant\",\"uuid\":\"usage-entry\",\"requestId\":\"request-1\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"id\":\"message-1\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}}\n",
        "",
    );
    let first_provider = first_fixture.provider();
    let first = first_provider.scan(None).unwrap();
    let encoded = first.checkpoint.to_json().unwrap();
    let decoded = coding_agent_data::Checkpoint::from_json(&encoded).unwrap();

    assert!(first_provider
        .scan(Some(&decoded))
        .unwrap()
        .changes
        .is_empty());

    let encoded_value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert!(encoded_value["state"].get("version").is_none());

    let second_fixture = Fixture::new(&user_entry("2026-01-02T03:04:05Z"), "");
    assert!(matches!(
        second_fixture.provider().scan(Some(&decoded)),
        Err(coding_agent_data::Error::InvalidCheckpoint(_))
    ));
}

#[test]
fn rewritten_and_removed_transcripts_emit_reset_and_removal() {
    let fixture = Fixture::new(
        &format!(
            "{}{}",
            user_entry("2026-01-02T03:04:05Z"),
            assistant_entry("2026-01-02T03:04:06Z")
        ),
        "",
    );
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    fs::write(&fixture.main_transcript, user_entry("2026-01-03T03:04:05Z")).unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second.changes.iter().any(|change| {
        matches!(
            change,
            Change::Reset(source) if source.path == fixture.main_transcript
        )
    }));

    fs::remove_file(&fixture.main_transcript).unwrap();
    let third = provider.scan(Some(&second.checkpoint)).unwrap();
    assert!(third.changes.iter().any(|change| {
        matches!(
            change,
            Change::Remove(source) if source.path == fixture.main_transcript
        )
    }));
    assert!(third.changes.iter().any(|change| {
        matches!(
            change,
            Change::Delete(id) if id.as_str().contains(":session:-workspace-project:session-1")
        )
    }));

    fs::remove_file(&fixture.subagent_transcript).unwrap();
    let fourth = provider.scan(Some(&third.checkpoint)).unwrap();
    assert!(fourth
        .changes
        .iter()
        .any(|change| matches!(change, Change::Delete(_))));
}

#[test]
fn session_summary_is_rebuilt_when_the_owner_changes() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"main-user\",\"sessionId\":\"session-1\",\"cwd\":\"/workspace/main\",\"timestamp\":\"2026-01-02T03:04:05Z\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
            "{\"type\":\"custom-title\",\"sessionId\":\"session-1\",\"customTitle\":\"Main-only title\"}\n"
        ),
        "{\"type\":\"assistant\",\"uuid\":\"worker-reply\",\"sessionId\":\"session-1\",\"agentId\":\"worker\",\"cwd\":\"/workspace/subagent\",\"timestamp\":\"2026-01-02T03:04:06Z\",\"message\":{\"role\":\"assistant\",\"content\":\"done\"}}\n",
    );
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    fs::remove_file(&fixture.main_transcript).unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();

    let mut records = HashMap::new();
    apply_changes(&mut records, &first.changes);
    apply_changes(&mut records, &second.changes);

    assert!(records.values().any(|record| {
        matches!(
            &record.data,
            RecordData::Session(session)
                if session.external_id == "session-1:worker"
                    && session.cwd.as_deref()
                        == Some(std::path::Path::new("/workspace/subagent"))
                    && session.relations.iter().any(|relation|
                        relation.kind == SessionRelationKind::Child)
        )
    }));
}

#[test]
fn malformed_lines_report_exact_source_location() {
    let fixture = Fixture::new(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"user-1\",\"sessionId\":\"session-1\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
            "{\"type\":\"assistant\"\n"
        ),
        "",
    );

    let batch = fixture.provider().scan(None).unwrap();
    let diagnostic = batch
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "claude_code.transcript.invalid_json")
        .expect("the malformed line should produce a diagnostic");
    let origin = diagnostic
        .origin
        .as_ref()
        .expect("line diagnostics should preserve their origin");

    assert_eq!(origin.path, fixture.main_transcript);
    assert!(matches!(
        origin.location,
        SourceLocation::JsonLine {
            line: 2,
            byte_start: Some(_),
            byte_end: Some(_),
        }
    ));
}

#[test]
fn excessive_diagnostics_are_bounded_and_report_truncation() {
    let transcript = "not-json\n".repeat(1_001);
    let fixture = Fixture::new(&transcript, "");

    let batch = fixture.provider().scan(None).unwrap();

    assert_eq!(batch.diagnostics.len(), 1_000);
    assert_eq!(
        batch
            .diagnostics
            .last()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("claude_code.diagnostics.truncated")
    );
}

#[test]
fn append_scan_converges_with_a_fresh_full_scan() {
    let fixture = Fixture::new(&user_entry("2026-01-02T03:04:05Z"), "");
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();
    let mut incremental_records = HashMap::new();
    apply_changes(&mut incremental_records, &first.changes);

    OpenOptions::new()
        .append(true)
        .open(&fixture.main_transcript)
        .unwrap()
        .write_all(
            format!(
                "{}{}",
                assistant_entry("2026-01-02T03:04:06Z"),
                "{\"type\":\"custom-title\",\"sessionId\":\"session-1\",\"customTitle\":\"Incremental title\"}\n"
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
#[cfg(feature = "claude-code-watch")]
fn subscription_emits_new_transcript_entries() {
    let fixture = Fixture::new(&user_entry("2026-01-02T03:04:05Z"), "");
    let provider = fixture
        .provider()
        .with_watch_options(ClaudeCodeWatchOptions {
            debounce: Duration::from_millis(20),
            reconcile_interval: Duration::from_millis(50),
        });
    let initial = provider.scan(None).unwrap();
    let subscription = provider.watch(initial.checkpoint).unwrap();

    OpenOptions::new()
        .append(true)
        .open(&fixture.main_transcript)
        .unwrap()
        .write_all(assistant_entry("2026-01-02T03:04:06Z").as_bytes())
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
                    record.data,
                    RecordData::Event(ref event)
                        if matches!(&event.data, EventData::Message(_))
                )
        )
    }));
}

#[test]
fn provider_info_is_stable_and_source_scoped() {
    let first = Fixture::new(&user_entry("2026-01-02T03:04:05Z"), "").provider();
    let second = Fixture::new(&user_entry("2026-01-02T03:04:05Z"), "").provider();

    assert_eq!(first.info().id.as_str(), "claude-code");
    assert_eq!(first.info().name, "Claude Code");
    assert_ne!(first.info().source, second.info().source);
    let input_queue = first
        .coverage()
        .iter()
        .find(|coverage| coverage.capability == "input_queue")
        .expect("input queue coverage");
    assert_eq!(input_queue.source, SourceCoverage::Persisted);
    assert_eq!(input_queue.adapter, AdapterCoverage::Normalized);
    let file_audit = first
        .coverage()
        .iter()
        .find(|coverage| coverage.capability == "os_file_audit")
        .expect("file-audit coverage");
    assert_eq!(file_audit.source, SourceCoverage::NotPersisted);
    assert_eq!(file_audit.adapter, AdapterCoverage::NotApplicable);
}
