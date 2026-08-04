use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::providers::shared::{
    content::{annotations as content_annotations, icons as content_icons},
    normalize::{actor_for_message, approval_policy, message_role},
    time::parse_rfc3339,
    tool::{file_changes as normalized_file_changes, ObservedTool},
};
use crate::{
    Actor, AgentInvocation, AgentInvocationStatus, AgentOperation, ContentBlock, ContextCompaction,
    Cost, DataQuality, ExecutionContext, FileChange, FileChangeKind, HookResult, HookStatus,
    InputQueue, Item, ItemData, ItemSequence, Message, MessageRole, ModeChange, ModeChangeKind,
    ModelInvocation, ModelInvocationStatus, OriginalData, ProviderInfo, QueueOperation, Reasoning,
    ReasoningVisibility, Record, RecordData, RecordId, Session, SessionRelation,
    SessionRelationKind, SourceLocation, SourceRef, StopReason, Timestamp, TokenUsage, ToolCall,
    ToolResult, ToolStatus, Turn, TurnStatus, UnknownItem, UnknownRecord, Usage,
};

use super::checkpoint::{SessionSummary, TranscriptContext, UsageSnapshot};
use super::ClaudeCodeSource;

const CONTENT_BLOCK_PART_STRIDE: u32 = 1 << 16;

#[derive(Clone, Copy)]
pub(super) struct Position {
    pub line: u64,
    pub byte_start: u64,
    pub byte_end: u64,
}

fn stop_reason(value: &str) -> StopReason {
    match value {
        "end_turn" | "completed" | "complete" | "success" | "succeeded" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "max_turn_requests" => StopReason::MaxTurnRequests,
        "stop_sequence" => StopReason::StopSequence,
        "tool_use" => StopReason::ToolUse,
        "pause_turn" => StopReason::PauseTurn,
        "model_context_window_exceeded" | "context_window_exceeded" => {
            StopReason::ContextWindowExceeded
        }
        "refusal" | "refused" => StopReason::Refusal,
        "cancelled" | "canceled" => StopReason::Cancelled,
        "interrupted" | "aborted" => StopReason::Interrupted,
        "failed" | "error" => StopReason::Failed,
        value => StopReason::Other(value.to_owned()),
    }
}

fn queue_operation(value: &str) -> QueueOperation {
    match value {
        "enqueue" => QueueOperation::Enqueue,
        "dequeue" => QueueOperation::Dequeue,
        "remove" => QueueOperation::Remove,
        "popAll" | "pop_all" | "pop-all" => QueueOperation::PopAll,
        value => QueueOperation::Other(value.to_owned()),
    }
}

/// Stable identity derived from one Claude Code transcript artifact.
///
/// Claude stores a subagent transcript below its parent transcript directory.
/// The path is the durable identity; JSONL fields only enrich the normalized
/// session and must not be used to rename it when a file is edited.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct TranscriptIdentity {
    pub project_key: String,
    pub external_id: String,
    pub transcript_key: String,
    pub parent_external_id: Option<String>,
    pub agent_id: Option<String>,
    pub is_main: bool,
}

pub(super) fn default_summary(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    path: &Path,
) -> Option<SessionSummary> {
    let identity = transcript_session_identity(source, path)?;
    Some(SessionSummary {
        project_key: identity.project_key.clone(),
        session: Session {
            external_id: identity.external_id,
            title: None,
            cwd: None,
            transcript: Some(path.to_path_buf()),
            created_at: None,
            updated_at: None,
            total_tokens: None,
            archived: false,
            model: None,
            model_provider: Some("anthropic".to_owned()),
            agent_version: None,
            agent_name: identity.agent_id,
            agent_role: identity.is_main.then_some("main".to_owned()),
            git_branch: None,
            git_commit: None,
            git_remote_url: None,
            relations: identity
                .parent_external_id
                .as_deref()
                .map(|parent| {
                    vec![SessionRelation::new(
                        SessionRelationKind::Child,
                        session_record_id(info, &identity.project_key, parent),
                    )]
                })
                .unwrap_or_default(),
            history: None,
            provider_attributes: Default::default(),
            quality: DataQuality::Partial,
        },
    })
}

pub(super) fn session_record_id(
    info: &ProviderInfo,
    project_key: &str,
    external_id: &str,
) -> RecordId {
    RecordId::scoped(
        &info.source,
        "session",
        format!("{project_key}:{external_id}"),
    )
}

pub(super) fn summary_record(info: &ProviderInfo, path: &Path, summary: &SessionSummary) -> Record {
    Record {
        id: session_record_id(info, &summary.project_key, &summary.session.external_id),
        source: info.source.clone(),
        session: None,
        turn: None,
        timestamp: summary.session.updated_at,
        origin: SourceRef {
            source: info.source.clone(),
            path: path.to_path_buf(),
            location: SourceLocation::WholeFile,
        },
        data: RecordData::Session(summary.session.clone()),
        original: None,
    }
}

pub(super) fn update_summary(info: &ProviderInfo, summary: &mut SessionSummary, value: &Value) {
    if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
        summary.session.cwd = Some(PathBuf::from(cwd));
    }
    if let Some(branch) = value.get("gitBranch").and_then(Value::as_str) {
        summary.session.git_branch = Some(branch.to_owned());
    }
    if let Some(version) = value
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| !version.is_empty())
    {
        summary.session.agent_version = Some(version.to_owned());
    }
    if let Some(agent_id) = value
        .get("agentId")
        .or_else(|| value.get("agent_id"))
        .and_then(Value::as_str)
        .filter(|agent_id| !agent_id.is_empty())
    {
        summary.session.agent_name = Some(agent_id.to_owned());
    }
    if let Some(model) = value
        .get("message")
        .and_then(|message| message.get("model"))
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty() && *model != "<synthetic>")
    {
        summary.session.model = Some(model.to_owned());
    }
    if !is_inherited(value) {
        if let Some(timestamp) = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339)
        {
            if summary
                .session
                .created_at
                .is_none_or(|current| timestamp < current)
            {
                summary.session.created_at = Some(timestamp);
            }
            if summary
                .session
                .updated_at
                .is_none_or(|current| timestamp > current)
            {
                summary.session.updated_at = Some(timestamp);
            }
        }
    }
    if let Some(parent_external_id) = forked_session_id(value) {
        let relation = SessionRelation {
            kind: SessionRelationKind::Fork,
            session: session_record_id(info, &summary.project_key, parent_external_id),
        };
        if !summary.session.relations.contains(&relation) {
            summary.session.relations.push(relation);
        }
    }
    match value.get("type").and_then(Value::as_str) {
        Some("custom-title") => {
            if let Some(title) = value.get("customTitle").and_then(Value::as_str) {
                summary.session.title = Some(title.to_owned());
            }
        }
        Some("ai-title") if summary.session.title.is_none() => {
            if let Some(title) = value.get("aiTitle").and_then(Value::as_str) {
                summary.session.title = Some(title.to_owned());
            }
        }
        _ => {}
    }
}

pub(super) fn line_records(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    path: &Path,
    value: &Value,
    position: Position,
    context: &mut TranscriptContext,
    child_sessions: &BTreeMap<String, RecordId>,
) -> Vec<Record> {
    let identity = transcript_session_identity(source, path);
    let session = identity
        .as_ref()
        .map(|identity| session_record_id(info, &identity.project_key, &identity.external_id));
    let item_scope = identity
        .as_ref()
        .map(|identity| identity.transcript_key.clone());
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339);
    let artifact = path
        .strip_prefix(source.config_dir())
        .unwrap_or(path)
        .to_string_lossy();
    let base = format!("{artifact}:byte:{}", position.byte_start);
    let origin = SourceRef {
        source: info.source.clone(),
        path: path.to_path_buf(),
        location: SourceLocation::JsonLine {
            line: position.line,
            byte_start: Some(position.byte_start),
            byte_end: Some(position.byte_end),
        },
    };
    let original = OriginalData {
        format: "claude-code.transcript".to_owned(),
        value: value.clone(),
    };
    let entry_type = value.get("type").and_then(Value::as_str);
    let message = value.get("message").unwrap_or(&Value::Null);
    let external_id = value
        .get("uuid")
        .or_else(|| message.get("id"))
        .and_then(Value::as_str);
    let item_id = entry_record_id(info, item_scope.as_deref(), external_id, &base);
    let parent = parent_record_id(info, item_scope.as_deref(), value);
    let inherited_from = inherited_item_record_id(info, identity.as_ref(), value);

    if (entry_type == Some("system")
        && value.get("subtype").and_then(Value::as_str) == Some("compact_boundary"))
        || value.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
    {
        let summary = if value.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
            let summary = content_text(message.get("content").unwrap_or(&Value::Null));
            (!summary.is_empty()).then_some(summary)
        } else {
            value
                .get("content")
                .or_else(|| value.get("summary"))
                .and_then(Value::as_str)
                .filter(|summary| !summary.is_empty())
                .map(str::to_owned)
        };
        return vec![Record {
            id: item_id,
            source: info.source.clone(),
            session,
            turn: context.current_turn.clone(),
            timestamp,
            origin,
            data: RecordData::Item(Item {
                external_id: external_id.map(str::to_owned),
                sequence: ItemSequence::new(position.line, 0),
                parent,
                inherited_from,
                actor: Actor::System,
                agent_id: None,
                data: ItemData::ContextCompaction(ContextCompaction {
                    summary,
                    automatic: None,
                    tokens_before: None,
                    tokens_after: None,
                    replacement_history: None,
                    window_number: None,
                    first_window_id: None,
                    previous_window_id: None,
                    window_id: None,
                }),
            }),
            original: Some(original),
        }];
    }

    if entry_type == Some("system")
        && value.get("subtype").and_then(Value::as_str) == Some("stop_hook_summary")
    {
        let infos = value
            .get("hookInfos")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let errors = value
            .get("hookErrors")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let prevented_continuation = value.get("preventedContinuation").and_then(Value::as_bool);
        let status = if prevented_continuation == Some(true) {
            HookStatus::Blocked
        } else if !errors.is_empty() {
            HookStatus::Failed
        } else {
            HookStatus::Completed
        };
        return vec![Record {
            id: item_id,
            source: info.source.clone(),
            session,
            turn: context.current_turn.clone(),
            timestamp,
            origin,
            data: RecordData::Item(Item {
                external_id: external_id.map(str::to_owned),
                sequence: ItemSequence::new(position.line, 0),
                parent,
                inherited_from,
                actor: Actor::System,
                agent_id: None,
                data: ItemData::HookResult(HookResult {
                    event: Some("stop".to_owned()),
                    entrypoint: value
                        .get("entrypoint")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    tool_call_id: value
                        .get("toolUseID")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    count: value.get("hookCount").and_then(Value::as_u64),
                    status,
                    prevented_continuation,
                    stop_reason: value
                        .get("stopReason")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    infos,
                    errors,
                    context: normalize_message_content(
                        value.get("hookAdditionalContext").unwrap_or(&Value::Null),
                    ),
                }),
            }),
            original: Some(original),
        }];
    }

    if entry_type == Some("mode") {
        if let Some(mode) = value
            .get("mode")
            .and_then(Value::as_str)
            .filter(|mode| !mode.is_empty())
        {
            return vec![Record {
                id: item_id,
                source: info.source.clone(),
                session,
                turn: context.current_turn.clone(),
                timestamp,
                origin,
                data: RecordData::Item(Item {
                    external_id: external_id.map(str::to_owned),
                    sequence: ItemSequence::new(position.line, 0),
                    parent,
                    inherited_from,
                    actor: Actor::System,
                    agent_id: None,
                    data: ItemData::ModeChange(ModeChange {
                        mode: mode.to_owned(),
                        kind: ModeChangeKind::Selected,
                        description: None,
                    }),
                }),
                original: Some(original),
            }];
        }
    }

    if entry_type == Some("queue-operation") {
        let queue = normalize_input_queue(value);
        if let (Some(tool_call_id), Some(task_id)) =
            (queue.tool_call_id.as_ref(), queue.task_id.as_ref())
        {
            context
                .queue_tasks
                .insert(tool_call_id.clone(), task_id.clone());
        }
        let mut records = vec![Record {
            id: RecordId::scoped(&info.source, "input-queue", &base),
            source: info.source.clone(),
            session: session.clone(),
            turn: context.current_turn.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Item(Item {
                external_id: external_id.map(str::to_owned),
                sequence: ItemSequence::new(position.line, 0),
                parent: parent.clone(),
                inherited_from,
                actor: Actor::System,
                agent_id: value
                    .get("agentId")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                data: ItemData::InputQueue(queue.clone()),
            }),
            original: Some(original),
        }];
        if let Some(tool_call_id) = queue.tool_call_id.as_deref() {
            if let Some(previous) = context.agent_invocations.get(tool_call_id).cloned() {
                records.push(merge_agent_invocation(context, child_sessions, previous));
            }
        }
        return records;
    }

    if entry_type == Some("attachment") {
        let attachment = value.get("attachment").unwrap_or(&Value::Null);
        match attachment.get("type").and_then(Value::as_str) {
            Some("queued_command") => {
                if let Some(prompt) = attachment
                    .get("prompt")
                    .and_then(Value::as_str)
                    .filter(|prompt| !prompt.is_empty())
                {
                    return vec![Record {
                        id: item_id,
                        source: info.source.clone(),
                        session,
                        turn: context.current_turn.clone(),
                        timestamp,
                        origin,
                        data: RecordData::Item(Item {
                            external_id: external_id.map(str::to_owned).or_else(|| {
                                attachment
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned)
                            }),
                            sequence: ItemSequence::new(position.line, 0),
                            parent,
                            inherited_from,
                            actor: Actor::User,
                            agent_id: None,
                            data: ItemData::Message(Message {
                                role: MessageRole::User,
                                phase: None,
                                content: vec![ContentBlock::text(prompt)],
                            }),
                        }),
                        original: Some(original),
                    }];
                }
            }
            Some("edited_text_file") => {
                if let Some(path) = attachment
                    .get("filename")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                {
                    return vec![Record {
                        id: item_id,
                        source: info.source.clone(),
                        session,
                        turn: context.current_turn.clone(),
                        timestamp,
                        origin,
                        data: RecordData::Item(Item {
                            external_id: external_id.map(str::to_owned),
                            sequence: ItemSequence::new(position.line, 0),
                            parent,
                            inherited_from,
                            actor: Actor::Environment,
                            agent_id: value
                                .get("agentId")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            data: ItemData::FileChange(FileChange {
                                path: PathBuf::from(path),
                                old_path: None,
                                kind: FileChangeKind::Update,
                                diff: None,
                                status: ToolStatus::Completed,
                            }),
                        }),
                        original: Some(original),
                    }];
                }
            }
            Some("date_change") => {
                if let Some(current_date) = attachment
                    .get("newDate")
                    .and_then(Value::as_str)
                    .filter(|date| !date.is_empty())
                {
                    return vec![Record {
                        id: item_id,
                        source: info.source.clone(),
                        session,
                        turn: context.current_turn.clone(),
                        timestamp,
                        origin,
                        data: RecordData::Item(Item {
                            external_id: external_id.map(str::to_owned),
                            sequence: ItemSequence::new(position.line, 0),
                            parent,
                            inherited_from,
                            actor: Actor::System,
                            agent_id: None,
                            data: ItemData::ExecutionContext(ExecutionContext {
                                current_date: Some(current_date.to_owned()),
                                ..ExecutionContext::default()
                            }),
                        }),
                        original: Some(original),
                    }];
                }
            }
            _ => {}
        }
    }

    // These entries update the separately emitted session summary or maintain
    // Claude Code UI state; they are not ordered agent activities. `last-prompt`
    // is retained as an opaque record because it is persisted provider data,
    // but it is not necessarily a new user message.
    if entry_type == Some("last-prompt") {
        return vec![Record {
            id: RecordId::scoped(&info.source, "unknown", base),
            source: info.source.clone(),
            session,
            turn: context.current_turn.clone(),
            timestamp,
            origin,
            data: RecordData::Unknown(UnknownRecord {
                kind: Some("last-prompt".to_owned()),
            }),
            original: Some(original),
        }];
    }
    if matches!(entry_type, Some("custom-title" | "ai-title")) {
        return Vec::new();
    }

    let is_message = matches!(entry_type, Some("user" | "assistant"))
        || (entry_type == Some("system") && message.is_object());
    if is_message && message.is_object() {
        let role = message_role(
            message
                .get("role")
                .and_then(Value::as_str)
                .or(entry_type)
                .unwrap_or_default(),
        );
        let content = message
            .get("content")
            .cloned()
            .unwrap_or_else(|| message.clone());
        let agent_id = value
            .get("agentId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut records = Vec::new();
        let primary_user = role == MessageRole::User && is_primary_user_message(value, &content);
        if primary_user {
            records.extend(begin_turn(
                info,
                item_scope.as_deref(),
                &base,
                value,
                timestamp,
                session.clone(),
                origin.clone(),
                context,
            ));
        }
        let turn = context.current_turn.clone();
        let invocation = (role == MessageRole::Assistant)
            .then(|| {
                model_invocation_record(
                    info,
                    item_scope.as_deref(),
                    &base,
                    position.line,
                    value,
                    message,
                    timestamp,
                    session.clone(),
                    turn.clone(),
                    origin.clone(),
                    parent.clone(),
                )
            })
            .flatten();
        let message_parent = invocation
            .as_ref()
            .map(|record| record.id.clone())
            .or(parent);
        let message_part = u32::from(invocation.is_some());
        records.extend(invocation);
        records.push(Record {
            id: item_id.clone(),
            source: info.source.clone(),
            session: session.clone(),
            turn: turn.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Item(Item {
                external_id: external_id.map(str::to_owned),
                sequence: ItemSequence::new(position.line, message_part),
                parent: message_parent,
                inherited_from,
                actor: actor_for_message(&role),
                agent_id: agent_id.clone(),
                data: ItemData::Message(Message {
                    role: role.clone(),
                    phase: None,
                    content: normalize_message_content(&content),
                }),
            }),
            original: Some(original),
        });
        if primary_user {
            if let Some(execution_context) = execution_context(value, message) {
                records.push(Record {
                    id: RecordId::scoped(
                        &info.source,
                        "execution-context",
                        format!("{base}:context"),
                    ),
                    source: info.source.clone(),
                    session: session.clone(),
                    turn: turn.clone(),
                    timestamp,
                    origin: origin.clone(),
                    data: RecordData::Item(Item {
                        external_id: None,
                        sequence: ItemSequence::new(position.line, message_part.saturating_add(1)),
                        parent: Some(item_id.clone()),
                        inherited_from: None,
                        actor: Actor::System,
                        agent_id: None,
                        data: ItemData::ExecutionContext(execution_context),
                    }),
                    original: None,
                });
            }
        }
        if let Value::Array(blocks) = &content {
            for (index, block) in blocks.iter().enumerate() {
                let sequence = content_block_sequence(position.line, index, message_part);
                match block.get("type").and_then(Value::as_str) {
                    Some("tool_use" | "server_tool_use" | "mcp_tool_use") => {
                        if let Some(call_id) = block.get("id").and_then(Value::as_str) {
                            let provider_name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            let input = block.get("input").cloned().unwrap_or(Value::Null);
                            let namespace = mcp_namespace(value, block);
                            let observed =
                                ObservedTool::new(provider_name, namespace.as_deref(), &input);
                            context
                                .tool_calls
                                .insert(call_id.to_owned(), observed.clone());
                            records.push(Record {
                                id: tool_call_record_id(info, item_scope.as_deref(), call_id),
                                source: info.source.clone(),
                                session: session.clone(),
                                turn: turn.clone(),
                                timestamp,
                                origin: origin.clone(),
                                data: RecordData::Item(Item {
                                    external_id: Some(call_id.to_owned()),
                                    sequence,
                                    parent: Some(item_id.clone()),
                                    inherited_from: inherited_tool_call_record_id(
                                        info,
                                        identity.as_ref(),
                                        value,
                                        call_id,
                                    ),
                                    actor: Actor::Agent,
                                    agent_id: agent_id.clone(),
                                    data: ItemData::ToolCall(ToolCall {
                                        call_id: call_id.to_owned(),
                                        title: None,
                                        kind: observed.kind,
                                        name: observed.name.clone(),
                                        namespace: observed.namespace.clone(),
                                        status: ToolStatus::Pending,
                                        input,
                                        locations: observed.locations.clone(),
                                    }),
                                }),
                                original: None,
                            });
                            if is_agent_tool(&observed.name) {
                                let invocation = agent_invocation_record(
                                    info,
                                    item_scope.as_deref(),
                                    call_id,
                                    &observed,
                                    AgentInvocationStatus::InProgress,
                                    None,
                                    None,
                                    sequence,
                                    timestamp,
                                    session.clone(),
                                    turn.clone(),
                                    origin.clone(),
                                    agent_id.clone(),
                                    child_sessions,
                                );
                                records.push(merge_agent_invocation(
                                    context,
                                    child_sessions,
                                    invocation,
                                ));
                            }
                        }
                    }
                    Some(block_type) if is_tool_result_type(block_type) => {
                        if let Some(call_id) = block.get("tool_use_id").and_then(Value::as_str) {
                            let observed = context.tool_calls.remove(call_id);
                            let status = tool_result_status(value, block, call_id);
                            let output = tool_result_output(value, block, call_id);
                            records.push(Record {
                                id: RecordId::scoped(
                                    &info.source,
                                    "tool-result",
                                    format!("{base}:{index}:{call_id}"),
                                ),
                                source: info.source.clone(),
                                session: session.clone(),
                                turn: turn.clone(),
                                timestamp,
                                origin: origin.clone(),
                                data: RecordData::Item(Item {
                                    external_id: Some(call_id.to_owned()),
                                    sequence,
                                    parent: Some(tool_call_record_id(
                                        info,
                                        item_scope.as_deref(),
                                        call_id,
                                    )),
                                    inherited_from: None,
                                    actor: Actor::Tool,
                                    agent_id: None,
                                    data: ItemData::ToolResult(ToolResult {
                                        call_id: call_id.to_owned(),
                                        name: observed.as_ref().map(|tool| tool.name.clone()),
                                        output: output.clone(),
                                        content: normalize_content(
                                            block.get("content").unwrap_or(&Value::Null),
                                        ),
                                        status,
                                        error: tool_result_error(value, block, call_id),
                                        duration_ms: tool_result_duration_ms(value, block, call_id),
                                    }),
                                }),
                                original: None,
                            });
                            if let Some(observed) = observed.as_ref() {
                                records.extend(file_change_records(
                                    info,
                                    item_scope.as_deref(),
                                    &base,
                                    call_id,
                                    observed,
                                    &output,
                                    status,
                                    sequence,
                                    timestamp,
                                    session.clone(),
                                    turn.clone(),
                                    origin.clone(),
                                ));
                                if is_agent_tool(&observed.name) {
                                    let invocation = agent_invocation_record(
                                        info,
                                        item_scope.as_deref(),
                                        call_id,
                                        observed,
                                        agent_status(status),
                                        Some(output),
                                        tool_result_error(value, block, call_id),
                                        sequence,
                                        timestamp,
                                        session.clone(),
                                        turn.clone(),
                                        origin.clone(),
                                        agent_id.clone(),
                                        child_sessions,
                                    );
                                    records.push(merge_agent_invocation(
                                        context,
                                        child_sessions,
                                        invocation,
                                    ));
                                }
                            }
                        }
                    }
                    Some("thinking" | "redacted_thinking") => {
                        let redacted =
                            block.get("type").and_then(Value::as_str) == Some("redacted_thinking");
                        records.push(Record {
                            id: RecordId::scoped(
                                &info.source,
                                "reasoning",
                                format!("{base}:{index}"),
                            ),
                            source: info.source.clone(),
                            session: session.clone(),
                            turn: turn.clone(),
                            timestamp,
                            origin: origin.clone(),
                            data: RecordData::Item(Item {
                                external_id: None,
                                sequence,
                                parent: Some(item_id.clone()),
                                inherited_from: None,
                                actor: Actor::Agent,
                                agent_id: agent_id.clone(),
                                data: ItemData::Reasoning(Reasoning {
                                    summary: Vec::new(),
                                    content: if redacted {
                                        Vec::new()
                                    } else {
                                        block
                                            .get("thinking")
                                            .and_then(Value::as_str)
                                            .map(ContentBlock::text)
                                            .into_iter()
                                            .collect()
                                    },
                                    visibility: if redacted {
                                        ReasoningVisibility::Redacted
                                    } else {
                                        ReasoningVisibility::Visible
                                    },
                                }),
                            }),
                            original: None,
                        });
                    }
                    _ => {}
                }
            }
        }
        if role == MessageRole::Assistant {
            if let Some(reason) = terminal_turn_reason(message) {
                if let Some(turn_record) =
                    finish_turn(info, value, reason, timestamp, session, origin, context)
                {
                    records.push(turn_record);
                }
            }
        }
        return records;
    }

    if external_id.is_some() || parent.is_some() {
        vec![Record {
            id: item_id,
            source: info.source.clone(),
            session,
            turn: context.current_turn.clone(),
            timestamp,
            origin,
            data: RecordData::Item(Item {
                external_id: external_id.map(str::to_owned),
                sequence: ItemSequence::new(position.line, 0),
                parent,
                inherited_from,
                actor: actor_for_entry(entry_type),
                agent_id: value
                    .get("agentId")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                data: ItemData::Unknown(UnknownItem {
                    kind: entry_type.map(str::to_owned),
                }),
            }),
            original: Some(original),
        }]
    } else {
        vec![Record {
            id: RecordId::scoped(&info.source, "unknown", base),
            source: info.source.clone(),
            session,
            turn: context.current_turn.clone(),
            timestamp,
            origin,
            data: RecordData::Unknown(UnknownRecord {
                kind: entry_type.map(str::to_owned),
            }),
            original: Some(original),
        }]
    }
}

fn normalize_input_queue(value: &Value) -> InputQueue {
    let content = value.get("content").and_then(|content| {
        content
            .as_str()
            .map(str::to_owned)
            .or_else(|| (!content.is_null()).then(|| content.to_string()))
    });
    let encoded = content
        .as_deref()
        .and_then(|content| serde_json::from_str::<Value>(content).ok());
    let task_id = queue_field(value, &["task_id", "taskId"])
        .or_else(|| {
            encoded
                .as_ref()
                .and_then(|value| queue_field(value, &["task_id", "taskId"]))
        })
        .or_else(|| {
            content
                .as_deref()
                .and_then(|value| xml_tag(value, "task-id"))
        })
        .map(str::to_owned);
    let tool_call_id = queue_field(
        value,
        &["tool_use_id", "toolUseId", "tool_call_id", "toolCallId"],
    )
    .or_else(|| {
        encoded.as_ref().and_then(|value| {
            queue_field(
                value,
                &["tool_use_id", "toolUseId", "tool_call_id", "toolCallId"],
            )
        })
    })
    .or_else(|| {
        content
            .as_deref()
            .and_then(|value| xml_tag(value, "tool-use-id"))
    })
    .map(str::to_owned);
    let task_type = queue_field(value, &["task_type", "taskType"])
        .or_else(|| {
            encoded
                .as_ref()
                .and_then(|value| queue_field(value, &["task_type", "taskType"]))
        })
        .or_else(|| {
            content
                .as_deref()
                .and_then(|value| xml_tag(value, "task-type"))
        })
        .map(str::to_owned);
    let status = queue_field(value, &["status"])
        .or_else(|| {
            encoded
                .as_ref()
                .and_then(|value| queue_field(value, &["status"]))
        })
        .or_else(|| {
            content
                .as_deref()
                .and_then(|value| xml_tag(value, "status"))
        })
        .map(str::to_owned);
    InputQueue {
        operation: queue_operation(
            value
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        content,
        task_id,
        tool_call_id,
        task_type,
        status,
    }
}

fn queue_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    })
}

fn xml_tag<'a>(value: &'a str, tag: &str) -> Option<&'a str> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = value.find(&start_tag)?.saturating_add(start_tag.len());
    let end = value[start..].find(&end_tag)?.saturating_add(start);
    let value = value[start..end].trim();
    (!value.is_empty()).then_some(value)
}

#[allow(clippy::too_many_arguments)]
fn begin_turn(
    info: &ProviderInfo,
    item_scope: Option<&str>,
    fallback: &str,
    value: &Value,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    origin: SourceRef,
    context: &mut TranscriptContext,
) -> Vec<Record> {
    let external_id = value
        .get("promptId")
        .or_else(|| value.get("uuid"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned();
    let id = turn_record_id(info, item_scope, &external_id);
    if context.current_turn.as_ref() == Some(&id) {
        return Vec::new();
    }

    let mut records = Vec::new();
    if let Some(previous_id) = context.current_turn.take() {
        let started_at = context.current_turn_started_at.take();
        records.push(Record {
            id: previous_id,
            source: info.source.clone(),
            session: session.clone(),
            turn: None,
            timestamp,
            origin: origin.clone(),
            data: RecordData::Turn(Turn {
                external_id: context.current_turn_external_id.take(),
                status: TurnStatus::Interrupted,
                started_at,
                completed_at: timestamp,
                duration_ms: elapsed_millis(started_at, timestamp),
                error: None,
                stop_reason: Some(StopReason::Interrupted),
                trace_id: None,
                model_context_window: None,
                time_to_first_token_ms: None,
            }),
            original: None,
        });
    }

    context.current_turn = Some(id.clone());
    context.current_turn_external_id = Some(external_id.clone());
    context.current_turn_started_at = timestamp;
    context.tool_calls.clear();
    records.push(Record {
        id,
        source: info.source.clone(),
        session,
        turn: None,
        timestamp,
        origin,
        data: RecordData::Turn(Turn {
            external_id: Some(external_id),
            status: TurnStatus::InProgress,
            started_at: timestamp,
            completed_at: None,
            duration_ms: None,
            error: None,
            stop_reason: None,
            trace_id: None,
            model_context_window: None,
            time_to_first_token_ms: None,
        }),
        original: Some(OriginalData {
            format: "claude-code.transcript".to_owned(),
            value: value.clone(),
        }),
    });
    records
}

fn is_primary_user_message(value: &Value, content: &Value) -> bool {
    if is_inherited(value)
        || value.get("isMeta").and_then(Value::as_bool) == Some(true)
        || value
            .get("sourceToolUseID")
            .or_else(|| value.get("sourceToolUseId"))
            .is_some_and(|value| !value.is_null())
    {
        return false;
    }

    match content {
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(blocks) => blocks.iter().any(|block| {
            !block
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(is_tool_result_type)
        }),
        value => !value.is_null(),
    }
}

fn turn_record_id(info: &ProviderInfo, item_scope: Option<&str>, external_id: &str) -> RecordId {
    RecordId::scoped(
        &info.source,
        "turn",
        item_scope
            .map(|scope| format!("{scope}:{external_id}"))
            .unwrap_or_else(|| external_id.to_owned()),
    )
}

fn execution_context(value: &Value, message: &Value) -> Option<ExecutionContext> {
    let cwd = value.get("cwd").and_then(Value::as_str).map(PathBuf::from);
    let workspace_roots = value
        .get("workspaceRoots")
        .or_else(|| value.get("workspace_roots"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|root| {
            root.as_str()
                .or_else(|| root.get("path").and_then(Value::as_str))
                .map(PathBuf::from)
        })
        .collect();
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty() && *model != "<synthetic>")
        .map(str::to_owned);
    let permission_mode = value
        .get("permissionMode")
        .or_else(|| value.get("permission_mode"))
        .and_then(Value::as_str);
    let approval_policy = permission_mode.map(approval_policy);
    let context = ExecutionContext {
        cwd,
        workspace_roots,
        model,
        model_provider: Some("anthropic".to_owned()),
        service_tier: None,
        model_context_window: None,
        reasoning_effort: None,
        reasoning_summary: None,
        personality: None,
        current_date: None,
        timezone: None,
        approval_policy,
        approvals_reviewer: None,
        sandbox_policy: None,
        permission_profile: None,
        active_permission_profile: None,
        collaboration_mode: None,
        provider_attributes: Default::default(),
    };
    (context.cwd.is_some()
        || !context.workspace_roots.is_empty()
        || context.model.is_some()
        || permission_mode.is_some())
    .then_some(context)
}

#[allow(clippy::too_many_arguments)]
fn model_invocation_record(
    info: &ProviderInfo,
    item_scope: Option<&str>,
    fallback: &str,
    line: u64,
    value: &Value,
    message: &Value,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    turn: Option<RecordId>,
    origin: SourceRef,
    parent: Option<RecordId>,
) -> Option<Record> {
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty() && *model != "<synthetic>")?;
    let invocation_id = value
        .get("requestId")
        .or_else(|| message.get("id"))
        .or_else(|| value.get("uuid"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let identity = invocation_id.unwrap_or(fallback);
    let scoped_identity = item_scope
        .map(|scope| format!("{scope}:{identity}"))
        .unwrap_or_else(|| identity.to_owned());
    let raw_stop_reason = message
        .get("stop_reason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.is_empty());
    let failed = value.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true);
    let status = if failed {
        ModelInvocationStatus::Failed
    } else if raw_stop_reason.is_some() {
        ModelInvocationStatus::Completed
    } else {
        ModelInvocationStatus::InProgress
    };
    Some(Record {
        id: RecordId::scoped(&info.source, "model-invocation", scoped_identity),
        source: info.source.clone(),
        session,
        turn,
        timestamp,
        origin,
        data: RecordData::Item(Item {
            external_id: invocation_id.map(str::to_owned),
            sequence: ItemSequence::new(line, 0),
            parent,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: value
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::to_owned),
            data: ItemData::ModelInvocation(ModelInvocation {
                invocation_id: invocation_id.map(str::to_owned),
                provider: Some("anthropic".to_owned()),
                service_tier: usage_service_tier(value, message),
                model: Some(model.to_owned()),
                status,
                usage: message.get("usage").and_then(normalize_usage),
                cost: cost_usd(value),
                stop_reason: raw_stop_reason.map(stop_reason),
                duration_ms: value
                    .get("durationMs")
                    .or_else(|| message.get("duration_ms"))
                    .and_then(Value::as_i64),
                error: failed.then(|| {
                    value
                        .get("error")
                        .or_else(|| value.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("Claude Code model invocation failed")
                        .to_owned()
                }),
            }),
        }),
        original: None,
    })
}

fn terminal_turn_reason(message: &Value) -> Option<StopReason> {
    let reason = message
        .get("stop_reason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.is_empty())?;
    (!matches!(reason, "tool_use" | "pause_turn")).then(|| stop_reason(reason))
}

fn finish_turn(
    info: &ProviderInfo,
    value: &Value,
    reason: StopReason,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    origin: SourceRef,
    context: &mut TranscriptContext,
) -> Option<Record> {
    let id = context.current_turn.take()?;
    let external_id = context.current_turn_external_id.take();
    let started_at = context.current_turn_started_at.take();
    context.tool_calls.clear();
    let failed = matches!(reason, StopReason::Failed);
    Some(Record {
        id,
        source: info.source.clone(),
        session,
        turn: None,
        timestamp,
        origin,
        data: RecordData::Turn(Turn {
            external_id,
            status: if failed {
                TurnStatus::Failed
            } else {
                TurnStatus::Completed
            },
            started_at,
            completed_at: timestamp,
            duration_ms: elapsed_millis(started_at, timestamp),
            error: failed.then(|| {
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Claude Code turn failed")
                    .to_owned()
            }),
            stop_reason: Some(reason),
            trace_id: None,
            model_context_window: None,
            time_to_first_token_ms: None,
        }),
        original: Some(OriginalData {
            format: "claude-code.transcript".to_owned(),
            value: value.clone(),
        }),
    })
}

fn elapsed_millis(started_at: Option<Timestamp>, completed_at: Option<Timestamp>) -> Option<i64> {
    match (started_at, completed_at) {
        (Some(started), Some(completed)) => Some(
            completed
                .as_millis()
                .saturating_sub(started.as_millis())
                .max(0),
        ),
        _ => None,
    }
}

fn content_block_sequence(line: u64, index: usize, message_part: u32) -> ItemSequence {
    let max_block = u32::MAX / CONTENT_BLOCK_PART_STRIDE;
    let block = u32::try_from(index)
        .unwrap_or(max_block)
        .saturating_add(1)
        .min(max_block);
    ItemSequence::new(
        line,
        block
            .saturating_mul(CONTENT_BLOCK_PART_STRIDE)
            .saturating_add(message_part),
    )
}

fn mcp_namespace(value: &Value, block: &Value) -> Option<String> {
    block
        .get("server_name")
        .or_else(|| block.get("serverName"))
        .or_else(|| block.get("server"))
        .and_then(Value::as_str)
        .filter(|server| !server.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("attributionMcpServer")
                .or_else(|| value.get("attribution_mcp_server"))
                .and_then(|server| {
                    server
                        .as_str()
                        .or_else(|| server.get("name").and_then(Value::as_str))
                        .or_else(|| server.get("server").and_then(Value::as_str))
                })
                .filter(|server| !server.is_empty())
                .map(str::to_owned)
        })
}

fn is_agent_tool(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "agent" | "task")
}

fn agent_status(status: ToolStatus) -> AgentInvocationStatus {
    match status {
        ToolStatus::Pending => AgentInvocationStatus::Pending,
        ToolStatus::AwaitingApproval | ToolStatus::InProgress => AgentInvocationStatus::InProgress,
        ToolStatus::Completed => AgentInvocationStatus::Completed,
        ToolStatus::Failed => AgentInvocationStatus::Failed,
        ToolStatus::Cancelled => AgentInvocationStatus::Cancelled,
        ToolStatus::Declined => AgentInvocationStatus::Rejected,
        ToolStatus::Unknown => AgentInvocationStatus::Unknown,
    }
}

#[allow(clippy::too_many_arguments)]
fn agent_invocation_record(
    info: &ProviderInfo,
    item_scope: Option<&str>,
    call_id: &str,
    tool: &ObservedTool,
    status: AgentInvocationStatus,
    output: Option<Value>,
    error: Option<String>,
    sequence: ItemSequence,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    turn: Option<RecordId>,
    origin: SourceRef,
    sender_id: Option<String>,
    child_sessions: &BTreeMap<String, RecordId>,
) -> Record {
    let identity = item_scope
        .map(|scope| format!("{scope}:{call_id}"))
        .unwrap_or_else(|| call_id.to_owned());
    let receiver_ids = tool
        .input
        .get("subagent_type")
        .or_else(|| tool.input.get("subagentType"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .into_iter()
        .collect();
    let task_id = output
        .as_ref()
        .and_then(|output| {
            output
                .get("agentId")
                .or_else(|| output.get("agent_id"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned);
    let child_session = task_id
        .as_deref()
        .and_then(|agent_id| child_sessions.get(agent_id))
        .cloned();
    Record {
        id: RecordId::scoped(&info.source, "agent-invocation", identity),
        source: info.source.clone(),
        session,
        turn,
        timestamp,
        origin,
        data: RecordData::Item(Item {
            external_id: Some(call_id.to_owned()),
            sequence: ItemSequence::new(sequence.position, sequence.part.saturating_add(1)),
            parent: Some(tool_call_record_id(info, item_scope, call_id)),
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: sender_id.clone(),
            data: ItemData::AgentInvocation(AgentInvocation {
                invocation_id: call_id.to_owned(),
                context_id: item_scope.map(str::to_owned),
                task_id,
                operation: AgentOperation::Spawn,
                sender_id,
                receiver_ids,
                child_session,
                status,
                input: Some(tool.input.clone()),
                output,
                artifacts: Vec::new(),
                error,
            }),
        }),
        original: None,
    }
}

fn merge_agent_invocation(
    context: &mut TranscriptContext,
    child_sessions: &BTreeMap<String, RecordId>,
    incoming: Record,
) -> Record {
    let Some((call_id, incoming_invocation)) = (match &incoming.data {
        RecordData::Item(item) => match &item.data {
            ItemData::AgentInvocation(invocation) => {
                Some((invocation.invocation_id.clone(), invocation.clone()))
            }
            _ => None,
        },
        _ => None,
    }) else {
        return incoming;
    };
    let previous = context.agent_invocations.get(&call_id).cloned();
    let mut merged = previous.unwrap_or_else(|| incoming.clone());
    if let RecordData::Item(item) = &mut merged.data {
        if let ItemData::AgentInvocation(invocation) = &mut item.data {
            if agent_status_rank(incoming_invocation.status) >= agent_status_rank(invocation.status)
            {
                invocation.status = incoming_invocation.status;
            }
            if incoming_invocation.output.is_some() {
                invocation.output = incoming_invocation.output;
            }
            if incoming_invocation.error.is_some() {
                invocation.error = incoming_invocation.error;
            }
            if incoming_invocation.input.is_some() {
                invocation.input = incoming_invocation.input;
            }
            if incoming_invocation.task_id.is_some() {
                invocation.task_id = incoming_invocation.task_id;
            }
            if incoming_invocation.child_session.is_some() {
                invocation.child_session = incoming_invocation.child_session;
            }
            for receiver in incoming_invocation.receiver_ids {
                if !invocation.receiver_ids.contains(&receiver) {
                    invocation.receiver_ids.push(receiver);
                }
            }
            if invocation.task_id.is_none() {
                invocation.task_id = context.queue_tasks.get(&call_id).cloned();
            }
            if invocation.child_session.is_none() {
                invocation.child_session = invocation
                    .task_id
                    .as_deref()
                    .and_then(|task_id| child_sessions.get(task_id))
                    .cloned();
            }
        }
    }
    context.agent_invocations.insert(call_id, merged.clone());
    merged
}

fn agent_status_rank(status: AgentInvocationStatus) -> u8 {
    match status {
        AgentInvocationStatus::Pending => 0,
        AgentInvocationStatus::InProgress
        | AgentInvocationStatus::InputRequired
        | AgentInvocationStatus::AuthorizationRequired => 1,
        AgentInvocationStatus::Completed
        | AgentInvocationStatus::Failed
        | AgentInvocationStatus::Cancelled
        | AgentInvocationStatus::Rejected
        | AgentInvocationStatus::Interrupted
        | AgentInvocationStatus::Unknown => 2,
    }
}

#[allow(clippy::too_many_arguments)]
fn file_change_records(
    info: &ProviderInfo,
    item_scope: Option<&str>,
    base: &str,
    call_id: &str,
    tool: &ObservedTool,
    output: &Value,
    status: ToolStatus,
    sequence: ItemSequence,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    turn: Option<RecordId>,
    origin: SourceRef,
) -> Vec<Record> {
    normalized_file_changes(tool, output, status)
        .into_iter()
        .enumerate()
        .map(|(index, change)| Record {
            id: RecordId::scoped(
                &info.source,
                "file-change",
                format!("{base}:{call_id}:{}", change.path.to_string_lossy()),
            ),
            source: info.source.clone(),
            session: session.clone(),
            turn: turn.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Item(Item {
                external_id: None,
                sequence: ItemSequence::new(
                    sequence.position,
                    sequence
                        .part
                        .saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
                        .saturating_add(2),
                ),
                parent: Some(tool_call_record_id(info, item_scope, call_id)),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: ItemData::FileChange(change),
            }),
            original: None,
        })
        .collect()
}

fn entry_record_id(
    info: &ProviderInfo,
    item_scope: Option<&str>,
    external_id: Option<&str>,
    fallback: &str,
) -> RecordId {
    RecordId::scoped(
        &info.source,
        "item",
        external_id
            .and_then(|external_id| item_scope.map(|scope| format!("{scope}:{external_id}")))
            .unwrap_or_else(|| fallback.to_owned()),
    )
}

fn parent_record_id(
    info: &ProviderInfo,
    item_scope: Option<&str>,
    value: &Value,
) -> Option<RecordId> {
    value
        .get("parentUuid")
        .and_then(Value::as_str)
        .or_else(|| value.get("logicalParentUuid").and_then(Value::as_str))
        .filter(|parent_id| !parent_id.is_empty())
        .and_then(|parent_id| {
            item_scope
                .map(|scope| RecordId::scoped(&info.source, "item", format!("{scope}:{parent_id}")))
        })
}

fn inherited_item_record_id(
    info: &ProviderInfo,
    identity: Option<&TranscriptIdentity>,
    value: &Value,
) -> Option<RecordId> {
    let project_key = &identity?.project_key;
    let inherited = value.get("forkedFrom")?;
    let session_id = inherited.get("sessionId").and_then(Value::as_str)?;
    let item_id = inherited
        .get("messageUuid")
        .or_else(|| inherited.get("messageUUID"))
        .and_then(Value::as_str)?;
    Some(RecordId::scoped(
        &info.source,
        "item",
        format!("{project_key}:{session_id}:{item_id}"),
    ))
}

fn inherited_tool_call_record_id(
    info: &ProviderInfo,
    identity: Option<&TranscriptIdentity>,
    value: &Value,
    call_id: &str,
) -> Option<RecordId> {
    let project_key = &identity?.project_key;
    let session_id = value
        .get("forkedFrom")?
        .get("sessionId")
        .and_then(Value::as_str)?;
    Some(tool_call_record_id(
        info,
        Some(&format!("{project_key}:{session_id}")),
        call_id,
    ))
}

fn actor_for_entry(entry_type: Option<&str>) -> Actor {
    match entry_type {
        Some("user") => Actor::User,
        Some("assistant") => Actor::Agent,
        Some("system") => Actor::System,
        Some("attachment") => Actor::Environment,
        Some(kind) => Actor::Other(kind.to_owned()),
        None => Actor::System,
    }
}

fn tool_call_record_id(info: &ProviderInfo, item_scope: Option<&str>, call_id: &str) -> RecordId {
    RecordId::scoped(
        &info.source,
        "tool-call",
        item_scope
            .map(|scope| format!("{scope}:{call_id}"))
            .unwrap_or_else(|| call_id.to_owned()),
    )
}

fn normalize_message_content(value: &Value) -> Vec<ContentBlock> {
    match value {
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| {
                !block
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(is_activity_content_type)
            })
            .flat_map(normalize_content)
            .collect(),
        value => normalize_content(value),
    }
}

fn is_activity_content_type(kind: &str) -> bool {
    matches!(
        kind,
        "tool_use" | "server_tool_use" | "mcp_tool_use" | "thinking" | "redacted_thinking"
    ) || is_tool_result_type(kind)
}

fn is_tool_result_type(kind: &str) -> bool {
    kind == "tool_result" || kind.ends_with("_tool_result")
}

fn normalize_content(value: &Value) -> Vec<ContentBlock> {
    match value {
        Value::Null => Vec::new(),
        Value::String(text) => vec![ContentBlock::text(text)],
        Value::Array(values) => values.iter().flat_map(normalize_content).collect(),
        Value::Object(fields) => match fields.get("type").and_then(Value::as_str) {
            Some("text") => fields
                .get("text")
                .and_then(Value::as_str)
                .map(|text| ContentBlock::Text {
                    text: text.to_owned(),
                    annotations: content_annotations(value),
                })
                .into_iter()
                .collect(),
            Some("image") => {
                let source = fields.get("source").unwrap_or(&Value::Null);
                vec![ContentBlock::Image {
                    mime_type: source
                        .get("media_type")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    uri: source.get("url").and_then(Value::as_str).map(str::to_owned),
                    data: source
                        .get("data")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    annotations: content_annotations(value),
                }]
            }
            Some("audio") => {
                let source = fields.get("source").unwrap_or(value);
                vec![ContentBlock::Audio {
                    mime_type: source
                        .get("media_type")
                        .or_else(|| source.get("mimeType"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    uri: source
                        .get("url")
                        .or_else(|| source.get("uri"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    data: source
                        .get("data")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    annotations: content_annotations(value),
                }]
            }
            Some("resource") => {
                let resource = fields
                    .get("resource")
                    .and_then(Value::as_object)
                    .unwrap_or(fields);
                vec![ContentBlock::Resource {
                    uri: resource
                        .get("uri")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    mime_type: resource
                        .get("mime_type")
                        .or_else(|| resource.get("mimeType"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    text: resource
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    data: resource
                        .get("blob")
                        .or_else(|| resource.get("data"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    annotations: content_annotations(value),
                }]
            }
            Some("resource_link") => vec![ContentBlock::ResourceLink {
                uri: fields
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                name: fields
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                title: fields
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                description: fields
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                mime_type: fields
                    .get("mime_type")
                    .or_else(|| fields.get("mimeType"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                size: fields.get("size").and_then(Value::as_u64),
                icons: content_icons(value),
                annotations: content_annotations(value),
            }],
            Some(kind) => vec![ContentBlock::Unknown {
                kind: Some(kind.to_owned()),
                value: value.clone(),
            }],
            None => vec![ContentBlock::Unknown {
                kind: None,
                value: value.clone(),
            }],
        },
        _ => vec![ContentBlock::Unknown {
            kind: None,
            value: value.clone(),
        }],
    }
}

fn content_text(value: &Value) -> String {
    normalize_content(value)
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn transcript_session_identity(
    source: &ClaudeCodeSource,
    path: &Path,
) -> Option<TranscriptIdentity> {
    let relative = path.strip_prefix(source.projects_dir()).ok()?;
    let components: Vec<_> = relative.components().collect();
    match components.as_slice() {
        [project, transcript] => {
            let session = Path::new(transcript.as_os_str())
                .file_stem()?
                .to_string_lossy()
                .into_owned();
            Some(TranscriptIdentity {
                project_key: project.as_os_str().to_string_lossy().into_owned(),
                external_id: session.clone(),
                transcript_key: format!("main:{session}"),
                parent_external_id: None,
                agent_id: None,
                is_main: true,
            })
        }
        [project, session, subagents, _]
            if subagents.as_os_str() == std::ffi::OsStr::new("subagents") =>
        {
            let project_key = project.as_os_str().to_string_lossy().into_owned();
            let parent = session.as_os_str().to_string_lossy().into_owned();
            let agent = Path::new(components[3].as_os_str())
                .file_stem()?
                .to_string_lossy()
                .strip_prefix("agent-")
                .unwrap_or_else(|| {
                    Path::new(components[3].as_os_str())
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                })
                .to_owned();
            Some(TranscriptIdentity {
                project_key,
                external_id: format!("{parent}:{agent}"),
                transcript_key: format!("subagent:{parent}:{agent}"),
                parent_external_id: Some(parent),
                agent_id: (!agent.is_empty()).then_some(agent),
                is_main: false,
            })
        }
        _ => None,
    }
}

fn tool_result_output(entry: &Value, block: &Value, call_id: &str) -> Value {
    correlated_tool_use_result(entry, block, call_id)
        .cloned()
        .or_else(|| block.get("content").cloned())
        .unwrap_or(Value::Null)
}

fn tool_result_status(entry: &Value, block: &Value, call_id: &str) -> ToolStatus {
    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
        return ToolStatus::Failed;
    }

    match correlated_tool_use_result(entry, block, call_id)
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
    {
        Some("failed" | "error") => ToolStatus::Failed,
        Some("cancelled" | "canceled") => ToolStatus::Cancelled,
        Some("declined" | "rejected") => ToolStatus::Declined,
        Some("pending") => ToolStatus::Pending,
        Some("in_progress" | "running") => ToolStatus::InProgress,
        _ => {
            // Anthropic defines `is_error` as optional. A tool_result without
            // `is_error: true` is the successful form.
            ToolStatus::Completed
        }
    }
}

fn tool_result_error(entry: &Value, block: &Value, call_id: &str) -> Option<String> {
    let result = correlated_tool_use_result(entry, block, call_id);
    let status = tool_result_status(entry, block, call_id);
    if !matches!(
        status,
        ToolStatus::Failed | ToolStatus::Cancelled | ToolStatus::Declined
    ) {
        return None;
    }
    result
        .and_then(|value| {
            value
                .get("error")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
        })
        .map(str::to_owned)
        .or_else(|| {
            let text = content_text(block.get("content").unwrap_or(&Value::Null));
            (!text.is_empty()).then_some(text)
        })
}

fn tool_result_duration_ms(entry: &Value, block: &Value, call_id: &str) -> Option<i64> {
    let result = correlated_tool_use_result(entry, block, call_id)?;
    result
        .get("totalDurationMs")
        .or_else(|| result.get("durationMs"))
        .and_then(Value::as_i64)
}

fn correlated_tool_use_result<'a>(
    entry: &'a Value,
    block: &Value,
    call_id: &str,
) -> Option<&'a Value> {
    let result = entry
        .get("toolUseResult")
        .filter(|value| !value.is_null())?;
    let result_call_id = result
        .get("toolUseId")
        .or_else(|| result.get("tool_use_id"))
        .or_else(|| result.get("callId"))
        .and_then(Value::as_str);
    if let Some(result_call_id) = result_call_id {
        return (result_call_id == call_id).then_some(result);
    }

    let result_count = entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|candidate| {
                    candidate
                        .get("type")
                        .and_then(Value::as_str)
                        .is_some_and(is_tool_result_type)
                })
                .count()
        })
        .unwrap_or_default();
    (result_count == 1 && block.get("tool_use_id").and_then(Value::as_str) == Some(call_id))
        .then_some(result)
}

pub(super) fn usage_observations(
    value: &Value,
    position: Position,
    turn: Option<RecordId>,
) -> Vec<(String, UsageSnapshot)> {
    let Some(entry) = assistant_usage_entry(value) else {
        return Vec::new();
    };
    if is_inherited(value) || is_inherited(entry) {
        return Vec::new();
    }
    let Some(message) = entry.get("message") else {
        return Vec::new();
    };
    if message.get("model").and_then(Value::as_str) == Some("<synthetic>") {
        return Vec::new();
    }
    let Some(raw_usage) = message.get("usage") else {
        return Vec::new();
    };
    let Some(usage) = normalize_usage(raw_usage) else {
        return Vec::new();
    };
    let request_id = entry
        .get("requestId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let message_id = message
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && *value != "<synthetic>")
        .map(str::to_owned);
    let key = match (&message_id, &request_id) {
        (Some(message_id), Some(request_id)) => {
            Some(format!("message:{message_id}:request:{request_id}"))
        }
        (None, Some(request_id)) => Some(format!("request:{request_id}")),
        (Some(message_id), None) => Some(format!("message:{message_id}")),
        (None, None) => None,
    }
    .or_else(|| {
        entry
            .get("uuid")
            .or_else(|| value.get("uuid"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(|value| format!("entry:{value}"))
    })
    .unwrap_or_else(|| format!("position:byte:{}", position.byte_start));
    let complete = message
        .get("stop_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.is_empty());
    let sidechain = entry
        .get("isSidechain")
        .or_else(|| message.get("isSidechain"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timestamp = entry
        .get("timestamp")
        .or_else(|| value.get("timestamp"))
        .and_then(Value::as_str)
        .and_then(parse_rfc3339);
    let mut observations = vec![(
        key.clone(),
        UsageSnapshot {
            usage,
            complete,
            message_id: message_id.clone(),
            request_id: request_id.clone(),
            model,
            service_tier: usage_service_tier(entry, message),
            cost: cost_usd(entry),
            sidechain,
            line: position.line,
            byte_start: position.byte_start,
            byte_end: position.byte_end,
            timestamp,
            turn: turn.clone(),
        },
    )];

    let mut advisor_index = 0;
    for iteration in raw_usage
        .get("iterations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if iteration.get("type").and_then(Value::as_str) != Some("advisor_message") {
            continue;
        }
        let Some(model) = iteration
            .get("model")
            .and_then(Value::as_str)
            .filter(|model| !model.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        let Some(usage) = normalize_usage(iteration) else {
            continue;
        };
        let advisor_message_id = message_id
            .as_ref()
            .map(|message_id| format!("{message_id}:advisor:{advisor_index}"));
        observations.push((
            format!("{key}:advisor:{advisor_index}"),
            UsageSnapshot {
                usage,
                complete,
                message_id: advisor_message_id,
                request_id: request_id.clone(),
                model: Some(model),
                service_tier: usage_service_tier(iteration, iteration),
                cost: None,
                sidechain,
                line: position.line,
                byte_start: position.byte_start,
                byte_end: position.byte_end,
                timestamp,
                turn: turn.clone(),
            },
        ));
        advisor_index += 1;
    }
    observations
}

pub(super) fn should_replace_usage(
    previous: Option<&UsageSnapshot>,
    current: &UsageSnapshot,
) -> bool {
    match previous {
        Some(previous) if previous.sidechain != current.sidechain => previous.sidechain,
        Some(previous) if previous.complete != current.complete => current.complete,
        Some(previous) if previous.usage.total != current.usage.total => {
            current.usage.total > previous.usage.total
        }
        Some(previous) if previous.message_id.is_some() != current.message_id.is_some() => {
            current.message_id.is_some()
        }
        Some(previous) if previous.model.is_some() != current.model.is_some() => {
            current.model.is_some()
        }
        Some(previous) if previous.service_tier.is_some() != current.service_tier.is_some() => {
            current.service_tier.is_some()
        }
        Some(previous) if previous.cost.is_some() != current.cost.is_some() => {
            current.cost.is_some()
        }
        Some(previous) if previous.turn != current.turn => true,
        Some(previous) => {
            previous.usage != current.usage
                || previous.service_tier != current.service_tier
                || previous.cost != current.cost
        }
        None => true,
    }
}

pub(super) fn usage_record(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    path: &Path,
    key: &str,
    snapshot: &UsageSnapshot,
) -> Record {
    let identity = transcript_session_identity(source, path);
    let session = identity
        .as_ref()
        .map(|identity| session_record_id(info, &identity.project_key, &identity.external_id));
    let artifact = path
        .strip_prefix(source.config_dir())
        .unwrap_or(path)
        .to_string_lossy();
    Record {
        id: RecordId::scoped(&info.source, "usage", format!("{artifact}:{key}")),
        source: info.source.clone(),
        session,
        turn: snapshot.turn.clone(),
        timestamp: snapshot.timestamp,
        origin: SourceRef {
            source: info.source.clone(),
            path: path.to_path_buf(),
            location: SourceLocation::JsonLine {
                line: snapshot.line,
                byte_start: Some(snapshot.byte_start),
                byte_end: Some(snapshot.byte_end),
            },
        },
        data: RecordData::Usage(Usage {
            model_provider: Some("anthropic".to_owned()),
            model: snapshot.model.clone(),
            service_tier: snapshot.service_tier.clone(),
            request_id: snapshot.request_id.clone(),
            invocation_id: snapshot.message_id.clone(),
            cumulative: None,
            delta: Some(snapshot.usage.clone()),
            cost: snapshot.cost.clone(),
        }),
        original: None,
    }
}

fn normalize_usage(value: &Value) -> Option<TokenUsage> {
    let input = non_negative_i64(value, "input_tokens");
    let flat_cache_creation = non_negative_i64(value, "cache_creation_input_tokens");
    let cache_creation_ephemeral_5m_input = value
        .get("cache_creation")
        .and_then(|value| non_negative_i64(value, "ephemeral_5m_input_tokens"));
    let cache_creation_ephemeral_1h_input = value
        .get("cache_creation")
        .and_then(|value| non_negative_i64(value, "ephemeral_1h_input_tokens"));
    let nested_cache_creation = value
        .get("cache_creation")
        .filter(|value| value.is_object())
        .map(|value| {
            if cache_creation_ephemeral_5m_input.is_some()
                || cache_creation_ephemeral_1h_input.is_some()
            {
                cache_creation_ephemeral_5m_input
                    .unwrap_or_default()
                    .saturating_add(cache_creation_ephemeral_1h_input.unwrap_or_default())
            } else {
                non_negative_i64(value, "input_tokens").unwrap_or_default()
            }
        });
    let cache_creation = nested_cache_creation.or(flat_cache_creation);
    let output = non_negative_i64(value, "output_tokens");
    let cached = non_negative_i64(value, "cache_read_input_tokens");
    let total = non_negative_i64(value, "total_tokens").or_else(|| {
        [input, cache_creation, cached, output]
            .into_iter()
            .flatten()
            .reduce(i64::saturating_add)
    })?;
    Some(TokenUsage {
        total,
        input,
        cache_creation_input: cache_creation,
        cache_creation_ephemeral_5m_input,
        cache_creation_ephemeral_1h_input,
        cached_input: cached,
        output,
        reasoning_output: None,
    })
}

fn assistant_usage_entry(value: &Value) -> Option<&Value> {
    match value.get("type").and_then(Value::as_str) {
        Some("assistant") => Some(value),
        Some("progress") => {
            let entry = value.get("data")?.get("message")?;
            match entry.get("type").and_then(Value::as_str) {
                Some("assistant") | None => entry
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .is_some()
                    .then_some(entry),
                Some(_) => None,
            }
        }
        _ => None,
    }
}

fn usage_service_tier(entry: &Value, message: &Value) -> Option<String> {
    message
        .get("usage")
        .and_then(|usage| usage.get("service_tier"))
        .or_else(|| entry.get("service_tier"))
        .and_then(Value::as_str)
        .filter(|tier| !tier.is_empty())
        .map(str::to_owned)
}

fn cost_usd(entry: &Value) -> Option<Cost> {
    let amount = entry.get("costUSD")?.as_number()?.to_string();
    Some(Cost {
        amount,
        currency: "USD".to_owned(),
    })
}

fn non_negative_i64(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
}

fn is_inherited(value: &Value) -> bool {
    value
        .get("forkedFrom")
        .is_some_and(|value| !value.is_null())
}

fn forked_session_id(value: &Value) -> Option<&str> {
    if value.get("type").and_then(Value::as_str) == Some("fork-context-ref") {
        return value
            .get("forkedSessionId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
    }
    value
        .get("forkedFrom")
        .and_then(|value| value.get("sessionId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}
