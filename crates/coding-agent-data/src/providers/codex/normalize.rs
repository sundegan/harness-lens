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
    CreditBalance, DataQuality, Event, EventData, EventSequence, ExecutionContext, FileChange,
    FileChangeKind, ForkInvocationBoundary, Goal, GoalStatus, HistoryMode, HistoryPosition,
    HistorySegment, Message, MessagePhase, MessageRole, ModeChange, ModeChangeKind, Notice,
    NoticeLevel, OriginalData, Plan, PlanStep, PlanStepStatus, ProviderInfo, RateLimit,
    RateLimitReason, RateLimitScope, RateLimitWindow, Reasoning, ReasoningVisibility, Record,
    RecordData, RecordId, Rollback, SandboxPolicy, Session, SessionHistory, SessionRelation,
    SessionRelationKind, SourceLocation, SourceRef, SpendLimit, StopReason, Timestamp, ToolCall,
    ToolKind, ToolLocation, ToolResult, ToolSourceKind, ToolStatus, UnknownEvent, UnknownRecord,
    WorldState,
};

use super::checkpoint::RolloutContext;
use super::token_usage;
use super::CodexSource;

pub(super) struct Position {
    pub line: u64,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub logical_ordinal: Option<u64>,
}

pub(super) struct RolloutInput<'a> {
    pub source: &'a CodexSource,
    pub info: &'a ProviderInfo,
    pub path: &'a Path,
    pub value: &'a Value,
    pub position: Position,
    pub indexed_sessions: &'a BTreeMap<String, Session>,
    pub lineage: Option<&'a [HistorySegment]>,
}

pub(super) fn line_too_large_record(
    source: &CodexSource,
    info: &ProviderInfo,
    path: &Path,
    position: Position,
    context: &RolloutContext,
) -> Record {
    let artifact = artifact_identity(source, path);
    let position_id = position
        .byte_start
        .map(|offset| format!("byte:{offset}"))
        .unwrap_or_else(|| format!("line:{}", position.line));
    Record {
        id: RecordId::scoped(&info.source, "unknown", format!("{artifact}:{position_id}")),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp: None,
        origin: SourceRef::json_line(
            info.source.clone(),
            path,
            position.line,
            position.byte_start,
            position.byte_end,
        ),
        data: RecordData::Unknown(UnknownRecord {
            kind: Some("line_too_large".to_owned()),
        }),
        original: None,
    }
}

fn history_mode(value: &str) -> HistoryMode {
    match value {
        "legacy" | "" => HistoryMode::Legacy,
        "paginated" => HistoryMode::Paginated,
        value => HistoryMode::Other(value.to_owned()),
    }
}

fn message_phase(value: &str) -> MessagePhase {
    match value {
        "commentary" => MessagePhase::Commentary,
        "final_answer" | "final" => MessagePhase::FinalAnswer,
        value => MessagePhase::Other(value.to_owned()),
    }
}

fn tool_status(value: &str) -> ToolStatus {
    match value {
        "pending" | "scheduled" | "validating" => ToolStatus::Pending,
        "awaiting_approval" | "awaitingApproval" | "awaiting_confirmation" => {
            ToolStatus::AwaitingApproval
        }
        "in_progress" | "inProgress" | "running" | "executing" => ToolStatus::InProgress,
        "completed" | "success" | "succeeded" => ToolStatus::Completed,
        "failed" | "error" => ToolStatus::Failed,
        "cancelled" | "canceled" => ToolStatus::Cancelled,
        "declined" | "rejected" => ToolStatus::Declined,
        _ => ToolStatus::Unknown,
    }
}

fn sandbox_policy(value: &str) -> SandboxPolicy {
    match value {
        "read_only" | "read-only" | "plan" => SandboxPolicy::ReadOnly,
        "workspace_write" | "workspace-write" | "accept_edits" | "acceptEdits" => {
            SandboxPolicy::WorkspaceWrite
        }
        "danger_full_access" | "danger-full-access" | "full_access" | "full-access"
        | "bypass_permissions" | "bypassPermissions" => SandboxPolicy::FullAccess,
        value => SandboxPolicy::Other(value.to_owned()),
    }
}

fn goal_status(value: &str) -> GoalStatus {
    match value {
        "active" => GoalStatus::Active,
        "paused" => GoalStatus::Paused,
        "blocked" => GoalStatus::Blocked,
        "usage_limited" | "usageLimited" => GoalStatus::UsageLimited,
        "budget_limited" | "budgetLimited" => GoalStatus::BudgetLimited,
        "complete" | "completed" => GoalStatus::Complete,
        value => GoalStatus::Other(value.to_owned()),
    }
}

pub(super) fn session_id(info: &ProviderInfo, external_id: &str) -> RecordId {
    RecordId::scoped(&info.source, "session", external_id)
}

fn session_relations(
    info: &ProviderInfo,
    value: &Value,
    spawn_parent: Option<&str>,
) -> Vec<SessionRelation> {
    let mut relations = Vec::new();
    if let Some(parent) = string(value, "forked_from_id").filter(|value| !value.is_empty()) {
        relations.push(SessionRelation::new(
            SessionRelationKind::Fork,
            session_id(info, parent),
        ));
    }

    let parent = spawn_parent
        .filter(|value| !value.is_empty())
        .or_else(|| string(value, "parent_thread_id").filter(|value| !value.is_empty()))
        .or_else(|| {
            value
                .get("source")
                .and_then(|source| source.get("subagent"))
                .and_then(|subagent| subagent.get("thread_spawn"))
                .and_then(|spawn| string(spawn, "parent_thread_id"))
                .filter(|value| !value.is_empty())
        });
    if let Some(parent) = parent {
        let relation = SessionRelation::new(SessionRelationKind::Child, session_id(info, parent));
        if !relations.contains(&relation) {
            relations.push(relation);
        }
    }
    if let Some(parent) = value
        .pointer("/history_base/thread_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        let relation = SessionRelation::new(SessionRelationKind::Fork, session_id(info, parent));
        if !relations.contains(&relation) {
            relations.push(relation);
        }
    }
    relations
}

fn session_history(
    info: &ProviderInfo,
    value: &Value,
    default_legacy: bool,
) -> Option<SessionHistory> {
    let mode = value
        .get("history_mode")
        .and_then(Value::as_str)
        .map(history_mode)
        .or_else(|| default_legacy.then_some(HistoryMode::Legacy))?;
    let base = value.get("history_base").and_then(|base| {
        let thread_id = string(base, "thread_id").filter(|value| !value.is_empty())?;
        Some(HistoryPosition {
            session: session_id(info, thread_id),
            end_ordinal_exclusive: base.get("end_ordinal_exclusive").and_then(Value::as_u64)?,
            end_byte_offset: base.get("end_byte_offset").and_then(Value::as_u64)?,
        })
    });
    let context_window_id = value
        .get("context_window")
        .and_then(|window| {
            window
                .as_str()
                .or_else(|| string(window, "window_id"))
                .or_else(|| string(window, "id"))
        })
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Some(SessionHistory {
        mode,
        base,
        own_start_ordinal: value
            .get("subagent_history_start_ordinal")
            .and_then(Value::as_u64),
        context_window_id,
        lineage: Vec::new(),
    })
}

fn session_provider_attributes(value: &Value) -> BTreeMap<String, Value> {
    [
        "originator",
        "source",
        "thread_source",
        "agent_path",
        "base_instructions",
        "dynamic_tools",
        "selected_capability_roots",
        "memory_mode",
        "multi_agent_version",
        "sandbox_policy",
        "approval_mode",
        "reasoning_effort",
    ]
    .into_iter()
    .filter_map(|key| {
        value
            .get(key)
            .filter(|value| !value.is_null())
            .cloned()
            .map(|value| (format!("codex.{key}"), value))
    })
    .collect()
}

pub(super) fn thread_session(
    source: &CodexSource,
    info: &ProviderInfo,
    value: &Value,
    spawn_parent: Option<&str>,
) -> Option<(Session, Record)> {
    let external_id = string(value, "id")?.to_owned();
    let transcript = string(value, "rollout_path")
        .filter(|value| !value.is_empty())
        .and_then(|value| source.normalize_rollout_path(PathBuf::from(value)));
    let session = Session {
        external_id: external_id.clone(),
        title: optional_nonempty_string(value, "name")
            .or_else(|| optional_nonempty_string(value, "title"))
            .or_else(|| display_title(value, "first_user_message"))
            .or_else(|| display_title(value, "preview")),
        cwd: string(value, "cwd").map(PathBuf::from),
        transcript,
        created_at: timestamp_from_fields(value, "created_at_ms", "created_at"),
        updated_at: timestamp_from_fields(value, "updated_at_ms", "updated_at"),
        total_tokens: value.get("tokens_used").and_then(Value::as_i64),
        archived: bool_value(value.get("archived"))
            || value
                .get("archived_at")
                .is_some_and(|value| !value.is_null()),
        model: optional_nonempty_string(value, "model"),
        model_provider: optional_nonempty_string(value, "model_provider"),
        agent_version: optional_nonempty_string(value, "cli_version"),
        agent_name: optional_nonempty_string(value, "agent_nickname"),
        agent_role: optional_nonempty_string(value, "agent_role"),
        git_branch: optional_nonempty_string(value, "git_branch"),
        git_commit: optional_nonempty_string(value, "git_sha"),
        git_remote_url: optional_nonempty_string(value, "git_origin_url"),
        relations: session_relations(info, value, spawn_parent),
        history: session_history(info, value, value.get("history_mode").is_some()),
        provider_attributes: session_provider_attributes(value),
        quality: DataQuality::Complete,
    };
    let id = session_id(info, &external_id);
    let record = Record {
        id,
        source: info.source.clone(),
        session: None,
        invocation: None,
        timestamp: session.updated_at,
        origin: SourceRef {
            source: info.source.clone(),
            path: source.state_database(),
            location: SourceLocation::DatabaseRecord { key: external_id },
        },
        data: RecordData::Session(session.clone()),
        original: Some(OriginalData {
            format: "codex.thread-row".to_owned(),
            value: value.clone(),
        }),
    };
    Some((session, record))
}

pub(super) fn merge_session_metadata(indexed: &Session, observed: &Session) -> Session {
    let mut relations = indexed.relations.clone();
    for relation in &observed.relations {
        if !relations.contains(relation) {
            relations.push(relation.clone());
        }
    }
    let title = match (&indexed.title, &observed.title) {
        (_, Some(observed_title))
            if indexed.title.is_none()
                || indexed.updated_at.is_none_or(|indexed_at| {
                    observed
                        .updated_at
                        .is_some_and(|observed_at| observed_at >= indexed_at)
                }) =>
        {
            Some(observed_title.clone())
        }
        (indexed_title, _) => indexed_title.clone(),
    };
    Session {
        external_id: indexed.external_id.clone(),
        title,
        cwd: indexed.cwd.clone().or_else(|| observed.cwd.clone()),
        transcript: indexed
            .transcript
            .clone()
            .or_else(|| observed.transcript.clone()),
        created_at: match (indexed.created_at, observed.created_at) {
            (Some(indexed), Some(observed)) => Some(indexed.min(observed)),
            (indexed, observed) => indexed.or(observed),
        },
        updated_at: match (indexed.updated_at, observed.updated_at) {
            (Some(indexed), Some(observed)) => Some(indexed.max(observed)),
            (indexed, observed) => indexed.or(observed),
        },
        total_tokens: indexed.total_tokens.or(observed.total_tokens),
        archived: indexed.archived || observed.archived,
        model: indexed.model.clone().or_else(|| observed.model.clone()),
        model_provider: indexed
            .model_provider
            .clone()
            .or_else(|| observed.model_provider.clone()),
        agent_version: indexed
            .agent_version
            .clone()
            .or_else(|| observed.agent_version.clone()),
        agent_name: indexed
            .agent_name
            .clone()
            .or_else(|| observed.agent_name.clone()),
        agent_role: indexed
            .agent_role
            .clone()
            .or_else(|| observed.agent_role.clone()),
        git_branch: indexed
            .git_branch
            .clone()
            .or_else(|| observed.git_branch.clone()),
        git_commit: indexed
            .git_commit
            .clone()
            .or_else(|| observed.git_commit.clone()),
        git_remote_url: indexed
            .git_remote_url
            .clone()
            .or_else(|| observed.git_remote_url.clone()),
        relations,
        history: observed.history.clone().or_else(|| indexed.history.clone()),
        provider_attributes: {
            let mut attributes = indexed.provider_attributes.clone();
            attributes.extend(observed.provider_attributes.clone());
            attributes
        },
        quality: if indexed.quality == DataQuality::Complete
            || observed.quality == DataQuality::Complete
        {
            DataQuality::Complete
        } else {
            DataQuality::Partial
        },
    }
}

pub(super) fn rollout_records(
    input: RolloutInput<'_>,
    context: &mut RolloutContext,
) -> Vec<Record> {
    let RolloutInput {
        source,
        info,
        path,
        value,
        position,
        indexed_sessions,
        lineage,
    } = input;
    let timestamp = timestamp_value(value.get("timestamp"));
    let kind = value.get("type").and_then(Value::as_str);
    let payload = value.get("payload").unwrap_or(&Value::Null);
    let artifact = artifact_identity(source, path);
    let position_id = position
        .byte_start
        .map(|offset| format!("byte:{offset}"))
        .unwrap_or_else(|| format!("line:{}", position.line));
    let original = OriginalData {
        format: "codex.rollout".to_owned(),
        value: value.clone(),
    };
    let origin = SourceRef {
        source: info.source.clone(),
        path: path.to_path_buf(),
        location: SourceLocation::JsonLine {
            line: position.line,
            byte_start: position.byte_start,
            byte_end: position.byte_end,
        },
    };
    let sequence = EventSequence::with_logical_ordinal(position.line, 0, position.logical_ordinal);

    match kind {
        Some("session_meta") => {
            let Some(external_id) = string(payload, "id") else {
                return vec![unknown_record(
                    info,
                    &artifact,
                    &position_id,
                    timestamp,
                    context,
                    origin,
                    original,
                    kind,
                )];
            };
            if context
                .session_external_id
                .as_deref()
                .is_some_and(|session_id| session_id != external_id)
            {
                // Legacy forks can contain copied parent metadata either
                // before or after the owning metadata. The index binding,
                // filename hint, or selected head metadata identifies the
                // physical owner. Copied metadata is context, not an unknown
                // child event.
                return Vec::new();
            }
            let record_id = session_id(info, external_id);
            let mut history = session_history(info, payload, true);
            if let (Some(history), Some(lineage)) = (&mut history, lineage) {
                history.lineage = lineage.to_vec();
            }
            let observed = Session {
                external_id: external_id.to_owned(),
                title: None,
                cwd: string(payload, "cwd").map(PathBuf::from),
                transcript: Some(path.to_path_buf()),
                created_at: string(payload, "timestamp")
                    .and_then(parse_rfc3339)
                    .or(timestamp),
                updated_at: timestamp,
                total_tokens: None,
                archived: path.starts_with(source.archived_sessions()),
                model: optional_nonempty_string(payload, "model"),
                model_provider: optional_nonempty_string(payload, "model_provider"),
                agent_version: optional_nonempty_string(payload, "cli_version"),
                agent_name: optional_nonempty_string(payload, "agent_nickname").or_else(|| {
                    payload
                        .get("agent")
                        .and_then(|agent| optional_nonempty_string(agent, "nickname"))
                }),
                agent_role: optional_nonempty_string(payload, "agent_role").or_else(|| {
                    payload
                        .get("agent")
                        .and_then(|agent| optional_nonempty_string(agent, "role"))
                }),
                git_branch: payload
                    .get("git")
                    .and_then(|git| string(git, "branch"))
                    .map(str::to_owned),
                git_commit: payload
                    .get("git")
                    .and_then(|git| optional_nonempty_string(git, "commit_hash")),
                git_remote_url: payload
                    .get("git")
                    .and_then(|git| optional_nonempty_string(git, "repository_url")),
                relations: session_relations(info, payload, None),
                history,
                provider_attributes: session_provider_attributes(payload),
                quality: DataQuality::Partial,
            };
            context.session_external_id = Some(external_id.to_owned());
            context.session = Some(record_id.clone());
            context.rollout_session = Some(observed.clone());
            let session = indexed_sessions
                .get(external_id)
                .map(|indexed| merge_session_metadata(indexed, &observed))
                .unwrap_or(observed);
            context.session_snapshot = Some(session.clone());
            if context.current_model.is_none() {
                context.current_model = session.model.clone();
            }
            if context.current_model_provider.is_none() {
                context.current_model_provider = session.model_provider.clone();
            }
            vec![Record {
                id: record_id,
                source: info.source.clone(),
                session: None,
                invocation: None,
                timestamp: session.updated_at.or(timestamp),
                origin,
                data: RecordData::Session(session),
                original: Some(original),
            }]
        }
        Some("event_msg") => normalize_event_msg(
            info,
            &artifact,
            &position_id,
            timestamp,
            context,
            sequence,
            origin,
            original,
            payload,
        ),
        Some("response_item") => normalize_response_item(
            info,
            &artifact,
            &position_id,
            sequence,
            timestamp,
            context,
            origin,
            original,
            payload,
        ),
        Some("turn_context") => normalize_invocation_context(
            info,
            &artifact,
            &position_id,
            sequence,
            timestamp,
            context,
            origin,
            original,
            payload,
        ),
        Some("inter_agent_communication") => normalize_inter_agent_communication(
            info,
            &artifact,
            &position_id,
            sequence,
            timestamp,
            context,
            origin,
            original,
            payload,
        ),
        Some("inter_agent_communication_metadata") => vec![Record {
            id: RecordId::scoped(
                &info.source,
                "event",
                format!("{artifact}:{position_id}:fork-turn-boundary"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: None,
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::ForkInvocationBoundary(ForkInvocationBoundary {
                    trigger_invocation: payload
                        .get("trigger_turn")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                }),
            }),
            original: Some(original),
        }],
        Some("world_state") => vec![Record {
            id: RecordId::scoped(
                &info.source,
                "event",
                format!("{artifact}:{position_id}:world-state"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: None,
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::WorldState(WorldState {
                    full: payload
                        .get("full")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    state: payload.get("state").cloned().unwrap_or(Value::Null),
                }),
            }),
            original: Some(original),
        }],
        Some("compacted") => vec![Record {
            id: RecordId::scoped(
                &info.source,
                "event",
                format!("{artifact}:{position_id}:compaction"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: string(payload, "window_id").map(str::to_owned),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::ContextCompaction(normalize_compaction(payload)),
            }),
            original: Some(original),
        }],
        _ => vec![unknown_record(
            info,
            &artifact,
            &position_id,
            timestamp,
            context,
            origin,
            original,
            kind,
        )],
    }
}

#[allow(clippy::too_many_arguments)]
fn normalize_invocation_context(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    update_execution_context_state(context, payload);
    let external_id = string(payload, "turn_id").filter(|value| !value.is_empty());
    let invocation = external_id
        .map(|external_id| invocation_id(info, artifact, external_id))
        .or_else(|| context.current_invocation.clone());
    if let (Some(external_id), Some(invocation)) = (external_id, invocation.as_ref()) {
        if context.current_invocation.as_ref() != Some(invocation) {
            context.current_invocation = Some(invocation.clone());
            context.current_invocation_external_id = Some(external_id.to_owned());
            context.current_invocation_inferred = false;
            context.current_invocation_started_at = None;
        }
    }
    let model_context_window = payload
        .get("model_context_window")
        .or_else(|| payload.get("context_window"))
        .and_then(Value::as_i64);
    if model_context_window.is_some() {
        context.current_invocation_model_context_window = model_context_window;
    }

    let mut records = Vec::new();
    if let Some(invocation) = invocation.clone() {
        records.push(Record {
            id: invocation,
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: None,
            timestamp,
            origin: origin.clone(),
            data: RecordData::AgentInvocation(AgentInvocation {
                invocation_id: context
                    .current_invocation_external_id
                    .clone()
                    .unwrap_or_default(),
                context_id: None,
                task_id: None,
                operation: AgentOperation::Invoke,
                sender_id: None,
                receiver_ids: Vec::new(),
                child_session: None,
                status: AgentInvocationStatus::InProgress,
                started_at: context.current_invocation_started_at,
                completed_at: None,
                duration_ms: None,
                input: None,
                output: None,
                artifacts: Vec::new(),
                error: None,
                stop_reason: None,
                trace_id: context.current_invocation_trace_id.clone(),
                model_context_window: context.current_invocation_model_context_window,
                time_to_first_token_ms: context.current_invocation_time_to_first_token_ms,
            }),
            original: Some(original.clone()),
        });
    }

    records.push(execution_context_record(
        info, artifact, position, sequence, timestamp, context, origin, original, payload,
    ));
    records
}

fn update_execution_context_state(context: &mut RolloutContext, payload: &Value) {
    if let Some(model) = optional_nonempty_string(payload, "model") {
        context.current_model = Some(model);
    }
    if let Some(provider) = optional_nonempty_string(payload, "model_provider")
        .or_else(|| optional_nonempty_string(payload, "model_provider_id"))
    {
        context.current_model_provider = Some(provider);
    }
    if payload.get("service_tier").is_some() {
        context.current_service_tier = optional_nonempty_string(payload, "service_tier");
    }
}

#[allow(clippy::too_many_arguments)]
fn execution_context_record(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Record {
    let workspace_roots = payload
        .get("workspace_roots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|root| {
            root.as_str()
                .or_else(|| root.get("path").and_then(Value::as_str))
                .map(PathBuf::from)
        })
        .collect();
    let approval_policy = payload
        .get("approval_policy")
        .and_then(provider_value_label)
        .map(approval_policy);
    let sandbox_value = payload.get("sandbox_policy");
    let sandbox_label = sandbox_value.and_then(provider_value_label);
    let collaboration_mode = payload
        .get("collaboration_mode")
        .and_then(provider_value_label);
    let permission_profile = payload
        .get("permission_profile")
        .filter(|value| !value.is_null())
        .cloned();
    let active_permission_profile = payload
        .get("active_permission_profile")
        .filter(|value| !value.is_null())
        .cloned();
    let provider_attributes = [
        "network",
        "file_system_sandbox_policy",
        "multi_agent_mode",
        "realtime_active",
        "comp_hash",
        // Compatibility aliases written by older Codex builds.
        "network_access",
        "network_policy",
        "filesystem_sandbox",
        "multi_agent_version",
        "realtime",
    ]
    .into_iter()
    .filter_map(|key| {
        payload
            .get(key)
            .filter(|value| !value.is_null())
            .cloned()
            .map(|value| (format!("codex.{key}"), value))
    })
    .collect();
    Record {
        id: RecordId::scoped(
            &info.source,
            "execution-context",
            format!("{artifact}:{position}"),
        ),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: string(payload, "turn_id").map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::System,
            agent_id: None,
            data: EventData::ExecutionContext(ExecutionContext {
                cwd: string(payload, "cwd").map(PathBuf::from),
                workspace_roots,
                model: optional_nonempty_string(payload, "model").or_else(|| {
                    context
                        .session_snapshot
                        .as_ref()
                        .and_then(|session| session.model.clone())
                }),
                model_provider: optional_nonempty_string(payload, "model_provider")
                    .or_else(|| optional_nonempty_string(payload, "model_provider_id"))
                    .or_else(|| {
                        context
                            .session_snapshot
                            .as_ref()
                            .and_then(|session| session.model_provider.clone())
                    }),
                service_tier: optional_nonempty_string(payload, "service_tier"),
                model_context_window: payload
                    .get("model_context_window")
                    .or_else(|| payload.get("context_window"))
                    .and_then(Value::as_i64),
                reasoning_effort: payload
                    .get("effort")
                    .or_else(|| payload.get("reasoning_effort"))
                    .and_then(provider_value_label)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                reasoning_summary: payload
                    .get("reasoning_summary")
                    .or_else(|| payload.get("summary"))
                    .and_then(provider_value_label)
                    .map(str::to_owned),
                personality: payload
                    .get("personality")
                    .and_then(provider_value_label)
                    .map(str::to_owned),
                current_date: optional_nonempty_string(payload, "current_date"),
                timezone: optional_nonempty_string(payload, "timezone"),
                approval_policy,
                approvals_reviewer: payload
                    .get("approvals_reviewer")
                    .and_then(provider_value_label)
                    .map(str::to_owned),
                sandbox_policy: sandbox_label.map(sandbox_policy),
                permission_profile,
                active_permission_profile,
                collaboration_mode: collaboration_mode.map(str::to_owned),
                provider_attributes,
            }),
        }),
        original: Some(original),
    }
}

#[allow(clippy::too_many_arguments)]
fn normalize_event_msg(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    sequence: EventSequence,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    match payload.get("type").and_then(Value::as_str) {
        Some("task_started" | "turn_started") => {
            let Some(external_id) = string(payload, "turn_id") else {
                return vec![unknown_record(
                    info,
                    artifact,
                    position,
                    timestamp,
                    context,
                    origin,
                    original,
                    Some("task_started"),
                )];
            };
            let id = invocation_id(info, artifact, external_id);
            let started_at = integer_timestamp(payload.get("started_at")).or(timestamp);
            context.terminal_tool_results.clear();
            context.tool_calls.clear();
            context.current_invocation = Some(id.clone());
            context.current_invocation_external_id = Some(external_id.to_owned());
            context.current_invocation_inferred = false;
            context.current_invocation_started_at = started_at;
            context.current_invocation_trace_id = optional_nonempty_string(payload, "trace_id");
            context.current_invocation_model_context_window = payload
                .get("model_context_window")
                .or_else(|| payload.get("context_window"))
                .and_then(Value::as_i64);
            context.current_invocation_time_to_first_token_ms = payload
                .get("time_to_first_token_ms")
                .or_else(|| payload.get("ttft_ms"))
                .and_then(Value::as_i64);
            vec![Record {
                id,
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: None,
                timestamp,
                origin,
                data: RecordData::AgentInvocation(AgentInvocation {
                    invocation_id: external_id.to_owned(),
                    context_id: None,
                    task_id: None,
                    operation: AgentOperation::Invoke,
                    sender_id: None,
                    receiver_ids: Vec::new(),
                    child_session: None,
                    status: AgentInvocationStatus::InProgress,
                    started_at,
                    completed_at: None,
                    duration_ms: None,
                    input: None,
                    output: None,
                    artifacts: Vec::new(),
                    error: None,
                    stop_reason: None,
                    trace_id: context.current_invocation_trace_id.clone(),
                    model_context_window: context.current_invocation_model_context_window,
                    time_to_first_token_ms: context.current_invocation_time_to_first_token_ms,
                }),
                original: Some(original),
            }]
        }
        Some("task_complete" | "turn_complete") => normalize_terminal_invocation(
            info, artifact, position, timestamp, context, origin, original, payload, false,
        ),
        Some("turn_aborted") => normalize_terminal_invocation(
            info, artifact, position, timestamp, context, origin, original, payload, true,
        ),
        Some("context_compacted") => vec![Record {
            id: RecordId::scoped(
                &info.source,
                "event",
                format!("{artifact}:{position}:compaction"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: None,
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::ContextCompaction(normalize_compaction(payload)),
            }),
            original: Some(original),
        }],
        Some("thread_goal_updated") => normalize_goal_event(
            info, artifact, position, timestamp, context, sequence, origin, original, payload,
        ),
        Some("thread_rolled_back") => vec![Record {
            id: RecordId::scoped(
                &info.source,
                "event",
                format!("{artifact}:{position}:rollback"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: None,
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::Rollback(Rollback {
                    user_inputs_removed: payload.get("num_turns").and_then(Value::as_u64),
                }),
            }),
            original: Some(original),
        }],
        Some(
            "user_message" | "agent_message" | "agent_reasoning" | "agent_reasoning_raw_content",
        ) => normalize_legacy_presentation(
            info, artifact, position, timestamp, context, sequence, origin, original, payload,
        ),
        Some("item_started" | "item_completed") => normalize_materialized_event(
            info, artifact, position, timestamp, context, sequence, origin, original, payload,
        ),
        Some("entered_review_mode" | "exited_review_mode") => normalize_review_mode_event(
            info, artifact, position, timestamp, context, sequence, origin, original, payload,
        ),
        Some("error" | "warning" | "guardian_warning" | "deprecation_notice") => {
            normalize_notice_event(
                info, artifact, position, timestamp, context, sequence, origin, original, payload,
            )
        }
        Some("patch_apply_begin" | "patch_apply_updated" | "patch_apply_end") => {
            normalize_patch_event(
                info, artifact, position, sequence, timestamp, context, origin, original, payload,
            )
        }
        Some("mcp_tool_call_begin" | "mcp_tool_call_end") => normalize_mcp_event(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("exec_command_end") => normalize_exec_command_end(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("web_search_end") => normalize_web_search_end(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("image_generation_end") => normalize_image_generation_end(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("view_image_tool_call") => normalize_view_image(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("sub_agent_activity") => normalize_subagent_event(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("plan_update") => normalize_plan_event(
            info, artifact, position, sequence, timestamp, context, origin, original, payload,
        ),
        Some("thread_settings_applied") => {
            let settings = payload.get("thread_settings").unwrap_or(payload);
            update_execution_context_state(context, settings);
            vec![execution_context_record(
                info, artifact, position, sequence, timestamp, context, origin, original, settings,
            )]
        }
        Some("thread_name_updated") => normalize_thread_name_update(
            info, artifact, position, timestamp, context, origin, original,
        ),
        Some("token_count") => {
            let usage = token_usage::usage_report(payload, &mut context.usage_accounting);
            let rate_limit = payload.get("rate_limits").and_then(normalize_rate_limit);
            let mut records = Vec::with_capacity(
                usize::from(usage.is_some()) + usize::from(rate_limit.is_some()),
            );
            if let Some(mut usage) = usage {
                usage.model = optional_nonempty_string(payload, "model")
                    .or_else(|| {
                        payload
                            .get("info")
                            .and_then(|info| optional_nonempty_string(info, "model"))
                    })
                    .or_else(|| context.current_model.clone())
                    .or_else(|| {
                        context
                            .session_snapshot
                            .as_ref()
                            .and_then(|session| session.model.clone())
                    });
                usage.model_provider = optional_nonempty_string(payload, "model_provider")
                    .or_else(|| optional_nonempty_string(payload, "model_provider_id"))
                    .or_else(|| context.current_model_provider.clone())
                    .or_else(|| {
                        context
                            .session_snapshot
                            .as_ref()
                            .and_then(|session| session.model_provider.clone())
                    });
                usage.service_tier = optional_nonempty_string(payload, "service_tier")
                    .or_else(|| context.current_service_tier.clone());
                usage.request_id = optional_nonempty_string(payload, "request_id")
                    .or_else(|| optional_nonempty_string(payload, "response_id"));
                usage.invocation_id = optional_nonempty_string(payload, "invocation_id");
                records.push(Record {
                    id: RecordId::scoped(&info.source, "usage", format!("{artifact}:{position}")),
                    source: info.source.clone(),
                    session: context.session.clone(),
                    invocation: context.current_invocation.clone(),
                    timestamp,
                    origin: origin.clone(),
                    data: RecordData::UsageReport(usage),
                    original: Some(original.clone()),
                });
            }
            if let Some(rate_limit) = rate_limit {
                records.push(Record {
                    id: RecordId::scoped(
                        &info.source,
                        "rate-limit",
                        format!("{artifact}:{position}"),
                    ),
                    source: info.source.clone(),
                    session: context.session.clone(),
                    invocation: context.current_invocation.clone(),
                    timestamp,
                    origin,
                    data: RecordData::RateLimit(rate_limit),
                    original: Some(original),
                });
            }
            records
        }
        event_kind => vec![unknown_record(
            info, artifact, position, timestamp, context, origin, original, event_kind,
        )],
    }
}

#[allow(clippy::too_many_arguments)]
fn normalize_goal_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    sequence: EventSequence,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let goal = payload.get("goal").unwrap_or(&Value::Null);
    let Some(objective) = string(goal, "objective").filter(|value| !value.is_empty()) else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("thread_goal_updated"),
        )];
    };
    let Some(status) = string(goal, "status").filter(|value| !value.is_empty()) else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("thread_goal_updated"),
        )];
    };
    let record_invocation = string(payload, "turn_id")
        .filter(|value| !value.is_empty())
        .map(|external_id| invocation_id(info, artifact, external_id))
        .or_else(|| context.current_invocation.clone());
    vec![Record {
        id: RecordId::scoped(&info.source, "event", format!("{artifact}:{position}:goal")),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: record_invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: string(goal, "thread_id")
                .or_else(|| string(payload, "thread_id"))
                .map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::System,
            agent_id: None,
            data: EventData::Goal(Goal {
                objective: objective.to_owned(),
                status: goal_status(status),
                token_budget: goal.get("token_budget").and_then(Value::as_i64),
                tokens_used: goal
                    .get("tokens_used")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                time_used_seconds: goal
                    .get("time_used_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                created_at: integer_timestamp(goal.get("created_at")),
                updated_at: integer_timestamp(goal.get("updated_at")),
            }),
        }),
        original: Some(original),
    }]
}

fn normalize_thread_name_update(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
) -> Vec<Record> {
    let payload = original.value.get("payload").unwrap_or(&Value::Null);
    let Some(external_id) = string(payload, "thread_id")
        .or(context.session_external_id.as_deref())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
    else {
        return vec![unknown_record(
            info,
            artifact,
            &format!("{position}:missing-thread-id"),
            timestamp,
            context,
            origin,
            original,
            Some("thread_name_updated"),
        )];
    };
    if context
        .session_external_id
        .as_deref()
        .is_some_and(|current| current != external_id.as_str())
    {
        return vec![unknown_record(
            info,
            artifact,
            &format!("{position}:foreign-thread"),
            timestamp,
            context,
            origin,
            original,
            Some("thread_name_updated"),
        )];
    }
    let Some(title) = optional_nonempty_string(payload, "thread_name")
        .or_else(|| optional_nonempty_string(payload, "name"))
    else {
        return vec![unknown_record(
            info,
            artifact,
            &format!("{position}:missing-title"),
            timestamp,
            context,
            origin,
            original,
            Some("thread_name_updated"),
        )];
    };

    let mut observed = context
        .rollout_session
        .clone()
        .unwrap_or_else(|| Session::new(&external_id));
    observed.title = Some(title.clone());
    observed.updated_at = match (observed.updated_at, timestamp) {
        (Some(current), Some(timestamp)) => Some(current.max(timestamp)),
        (current, timestamp) => current.or(timestamp),
    };
    context.rollout_session = Some(observed.clone());

    let mut session = context.session_snapshot.clone().unwrap_or(observed);
    session.title = Some(title);
    session.updated_at = match (session.updated_at, timestamp) {
        (Some(current), Some(observed)) => Some(current.max(observed)),
        (current, observed) => current.or(observed),
    };
    context.session_external_id = Some(external_id.clone());
    context.session = Some(session_id(info, &external_id));
    context.session_snapshot = Some(session.clone());

    vec![Record {
        id: session_id(info, &external_id),
        source: info.source.clone(),
        session: None,
        invocation: None,
        timestamp: session.updated_at.or(timestamp),
        origin,
        data: RecordData::Session(session),
        original: Some(original),
    }]
}

/// Normalizes legacy Codex presentation events that predate the structured
/// response-event stream. They remain separate records because the provider
/// does not persist a reliable identity linking them to a response event. A
/// content-only comparison could otherwise discard two real, identical
/// messages from one invocation.
#[allow(clippy::too_many_arguments)]
fn normalize_legacy_presentation(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    sequence: EventSequence,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(kind) = payload.get("type").and_then(Value::as_str) else {
        return vec![unknown_record(
            info, artifact, position, timestamp, context, origin, original, None,
        )];
    };
    let mut records = Vec::new();
    if kind == "user_message" && context.current_invocation.is_none() {
        let external_id = format!("legacy:{position}");
        let id = invocation_id(info, artifact, &external_id);
        context.current_invocation = Some(id.clone());
        context.current_invocation_external_id = Some(external_id.clone());
        context.current_invocation_inferred = true;
        context.current_invocation_started_at = timestamp;
        records.push(Record {
            id,
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: None,
            timestamp,
            origin: origin.clone(),
            data: RecordData::AgentInvocation(AgentInvocation {
                invocation_id: external_id,
                context_id: None,
                task_id: None,
                operation: AgentOperation::Invoke,
                sender_id: None,
                receiver_ids: Vec::new(),
                child_session: None,
                status: AgentInvocationStatus::InProgress,
                started_at: timestamp,
                completed_at: None,
                duration_ms: None,
                input: None,
                output: None,
                artifacts: Vec::new(),
                error: None,
                stop_reason: None,
                trace_id: None,
                model_context_window: None,
                time_to_first_token_ms: None,
            }),
            original: Some(original.clone()),
        });
    }
    let (event_data, role) = match kind {
        "user_message" => {
            let value = payload
                .get("message")
                .or_else(|| payload.get("content"))
                .unwrap_or(&Value::Null);
            (
                EventData::Message(Message {
                    role: MessageRole::User,
                    phase: None,
                    content: normalize_content(value),
                }),
                "user",
            )
        }
        "agent_message" => {
            let value = payload
                .get("message")
                .or_else(|| payload.get("content"))
                .unwrap_or(&Value::Null);
            (
                EventData::Message(Message {
                    role: MessageRole::Assistant,
                    phase: optional_nonempty_string(payload, "phase")
                        .as_deref()
                        .map(message_phase),
                    content: normalize_content(value),
                }),
                "assistant",
            )
        }
        "agent_reasoning" | "agent_reasoning_raw_content" => {
            let value = payload
                .get("text")
                .or_else(|| payload.get("summary"))
                .or_else(|| payload.get("raw_content"))
                .or_else(|| payload.get("content"))
                .unwrap_or(&Value::Null);
            (
                EventData::Reasoning(Reasoning {
                    summary: if kind == "agent_reasoning" {
                        normalize_reasoning_summary(value)
                    } else {
                        Vec::new()
                    },
                    content: if kind == "agent_reasoning_raw_content" {
                        normalize_content(value)
                    } else {
                        Vec::new()
                    },
                    visibility: ReasoningVisibility::Visible,
                }),
                "assistant",
            )
        }
        _ => unreachable!(),
    };
    records.push(Record {
        id: RecordId::scoped(
            &info.source,
            if kind == "agent_reasoning" || kind == "agent_reasoning_raw_content" {
                "reasoning"
            } else {
                "message"
            },
            format!("{artifact}:legacy:{position}"),
        ),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin: origin.clone(),
        data: RecordData::Event(Event {
            external_id: string(payload, "client_id")
                .or_else(|| string(payload, "id"))
                .map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: if kind == "agent_reasoning" || kind == "agent_reasoning_raw_content" {
                Actor::Agent
            } else if role == "user" {
                Actor::User
            } else {
                Actor::Agent
            },
            agent_id: None,
            data: event_data,
        }),
        original: Some(original),
    });
    if kind == "agent_message" && context.current_invocation_inferred {
        let id = context.current_invocation.take();
        let external_id = context.current_invocation_external_id.take();
        let started_at = context.current_invocation_started_at.take();
        context.current_invocation_inferred = false;
        if let Some(id) = id {
            records.push(Record {
                id,
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: None,
                timestamp,
                origin,
                data: RecordData::AgentInvocation(AgentInvocation {
                    invocation_id: external_id.unwrap_or_default(),
                    context_id: None,
                    task_id: None,
                    operation: AgentOperation::Invoke,
                    sender_id: None,
                    receiver_ids: Vec::new(),
                    child_session: None,
                    status: AgentInvocationStatus::Completed,
                    started_at,
                    completed_at: timestamp,
                    duration_ms: match (started_at, timestamp) {
                        (Some(started), Some(completed)) => Some(
                            completed
                                .as_millis()
                                .saturating_sub(started.as_millis())
                                .max(0),
                        ),
                        _ => None,
                    },
                    input: None,
                    output: None,
                    artifacts: Vec::new(),
                    error: None,
                    stop_reason: Some(StopReason::EndInvocation),
                    trace_id: None,
                    model_context_window: None,
                    time_to_first_token_ms: None,
                }),
                original: None,
            });
        }
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn normalize_inter_agent_communication(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let external_id = string(payload, "id").filter(|value| !value.is_empty());
    let identity = external_id.unwrap_or(position);
    let sender = optional_nonempty_string(payload, "author");
    let mut receiver_ids = optional_nonempty_string(payload, "recipient")
        .into_iter()
        .collect::<Vec<_>>();
    receiver_ids.extend(
        payload
            .get("other_recipients")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    );
    let task_id = payload
        .get("internal_chat_message_metadata_passthrough")
        .and_then(|metadata| string(metadata, "turn_id"))
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let record_invocation = task_id
        .as_deref()
        .map(|external_id| invocation_id(info, artifact, external_id))
        .or_else(|| context.current_invocation.clone());
    let content = string(payload, "content")
        .filter(|value| !value.is_empty())
        .map(|value| serde_json::json!({ "content": value }));

    vec![Record {
        id: RecordId::scoped(
            &info.source,
            "agent-invocation",
            format!("{artifact}:{identity}"),
        ),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: record_invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: external_id.map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: sender.clone(),
            data: EventData::AgentInvocation(AgentInvocation {
                invocation_id: identity.to_owned(),
                context_id: context.session_external_id.clone(),
                task_id,
                operation: AgentOperation::SendInput,
                sender_id: sender,
                receiver_ids,
                child_session: None,
                status: AgentInvocationStatus::Completed,
                started_at: None,
                completed_at: None,
                duration_ms: None,
                stop_reason: None,
                trace_id: None,
                model_context_window: None,
                time_to_first_token_ms: None,
                input: content,
                output: None,
                artifacts: Vec::new(),
                error: None,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_review_mode_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    sequence: EventSequence,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let entered = payload.get("type").and_then(Value::as_str) == Some("entered_review_mode");
    let external_id = optional_nonempty_string(payload, "item_id");
    let identity = external_id.as_deref().unwrap_or(position);
    let record_invocation = string(payload, "turn_id")
        .filter(|value| !value.is_empty())
        .map(|external_id| invocation_id(info, artifact, external_id))
        .or_else(|| context.current_invocation.clone());
    let description = if entered {
        optional_nonempty_string(payload, "user_facing_hint")
    } else {
        payload
            .get("review_output")
            .and_then(|review| optional_nonempty_string(review, "overall_explanation"))
    };
    vec![Record {
        id: RecordId::scoped(
            &info.source,
            "mode-change",
            format!("{artifact}:{identity}"),
        ),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: record_invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id,
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::System,
            agent_id: None,
            data: EventData::ModeChange(ModeChange {
                mode: "review".to_owned(),
                kind: if entered {
                    ModeChangeKind::Entered
                } else {
                    ModeChangeKind::Exited
                },
                description,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_notice_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    sequence: EventSequence,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(message) = string(payload, "message").filter(|message| !message.is_empty()) else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            payload.get("type").and_then(Value::as_str),
        )];
    };
    let event_type = payload.get("type").and_then(Value::as_str);
    let level = if event_type == Some("error") {
        NoticeLevel::Error
    } else {
        NoticeLevel::Warning
    };
    let code = payload
        .get("codex_error_info")
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .or_else(|| value.get("type").and_then(Value::as_str).map(str::to_owned))
        })
        .or_else(|| event_type.map(str::to_owned));
    vec![Record {
        id: RecordId::scoped(&info.source, "notice", format!("{artifact}:{position}")),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: None,
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::System,
            agent_id: None,
            data: EventData::Notice(Notice {
                level,
                code,
                message: message.to_owned(),
            }),
        }),
        original: Some(original),
    }]
}

struct MaterializedTool {
    call_id: String,
    observed: ObservedTool,
    title: Option<String>,
    status: ToolStatus,
    output: Value,
    content: Vec<ContentBlock>,
    error: Option<String>,
    duration_ms: Option<i64>,
    terminal: bool,
    derive_file_changes: bool,
}

#[allow(clippy::too_many_arguments)]
fn materialized_tool_records(
    info: &ProviderInfo,
    artifact: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    tool: MaterializedTool,
) -> Vec<Record> {
    let call_record_id = RecordId::scoped(
        &info.source,
        "tool-call",
        format!("{artifact}:{}", tool.call_id),
    );
    if tool.terminal {
        context.tool_calls.remove(&tool.call_id);
        remember_terminal_tool(context, &tool.call_id, tool.status);
    } else {
        context
            .tool_calls
            .insert(tool.call_id.clone(), tool.observed.clone());
    }

    let mut records = vec![Record {
        id: call_record_id.clone(),
        source: info.source.clone(),
        session: session.clone(),
        invocation: invocation.clone(),
        timestamp,
        origin: origin.clone(),
        data: RecordData::Event(Event {
            external_id: Some(tool.call_id.clone()),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: None,
            data: EventData::ToolCall(ToolCall {
                call_id: tool.call_id.clone(),
                name: tool.observed.name.clone(),
                namespace: tool.observed.namespace.clone(),
                source_kind: tool.observed.source_kind,
                server_name: tool.observed.server_name.clone(),
                title: tool.title,
                kind: tool.observed.kind,
                status: tool.status,
                input: tool.observed.input.clone(),
                locations: tool.observed.locations.clone(),
            }),
        }),
        original: Some(original),
    }];
    if !tool.terminal {
        return records;
    }

    records.push(Record {
        id: RecordId::scoped(
            &info.source,
            "tool-result",
            format!("{artifact}:{}", tool.call_id),
        ),
        source: info.source.clone(),
        session: session.clone(),
        invocation: invocation.clone(),
        timestamp,
        origin: origin.clone(),
        data: RecordData::Event(Event {
            external_id: Some(tool.call_id.clone()),
            sequence: sequence.with_part(sequence.part.saturating_add(1)),
            parent: Some(call_record_id.clone()),
            inherited_from: None,
            actor: Actor::Tool,
            agent_id: None,
            data: EventData::ToolResult(ToolResult {
                call_id: tool.call_id.clone(),
                name: Some(tool.observed.name.clone()),
                output: tool.output.clone(),
                content: tool.content,
                status: tool.status,
                error: tool.error,
                duration_ms: tool.duration_ms,
            }),
        }),
        original: None,
    });

    if tool.derive_file_changes {
        for (index, change) in normalized_file_changes(&tool.observed, &tool.output, tool.status)
            .into_iter()
            .enumerate()
        {
            records.push(Record {
                id: RecordId::scoped(
                    &info.source,
                    "file-change",
                    format!(
                        "{artifact}:{}:{}",
                        tool.call_id,
                        change.path.to_string_lossy()
                    ),
                ),
                source: info.source.clone(),
                session: session.clone(),
                invocation: invocation.clone(),
                timestamp,
                origin: origin.clone(),
                data: RecordData::Event(Event {
                    external_id: None,
                    sequence: sequence.with_part(
                        sequence
                            .part
                            .saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
                            .saturating_add(2),
                    ),
                    parent: Some(call_record_id.clone()),
                    inherited_from: None,
                    actor: Actor::Tool,
                    agent_id: None,
                    data: EventData::FileChange(change),
                }),
                original: None,
            });
        }
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    sequence: EventSequence,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(event) = payload.get("item").filter(|event| event.is_object()) else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            payload.get("type").and_then(Value::as_str),
        )];
    };
    let event_kind = event.get("type").and_then(Value::as_str);
    let event_id = string(event, "id").filter(|value| !value.is_empty());
    let identity = event_id.unwrap_or(position);
    let terminal = payload.get("type").and_then(Value::as_str) == Some("item_completed");
    let event_timestamp = materialized_event_timestamp(payload, terminal).or(timestamp);
    let record_invocation = string(payload, "turn_id")
        .filter(|value| !value.is_empty())
        .map(|external_id| invocation_id(info, artifact, external_id))
        .or_else(|| context.current_invocation.clone());
    let session = context.session.clone();

    match event_kind {
        Some("UserMessage" | "user_message") => {
            vec![Record {
                id: RecordId::scoped(&info.source, "message", format!("{artifact}:{identity}")),
                source: info.source.clone(),
                session,
                invocation: record_invocation,
                timestamp: event_timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: event_id.map(str::to_owned),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::User,
                    agent_id: None,
                    data: EventData::Message(Message {
                        role: MessageRole::User,
                        phase: None,
                        content: normalize_content(event.get("content").unwrap_or(&Value::Null)),
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("HookPrompt" | "hook_prompt") => {
            let content = event
                .get("fragments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|fragment| string(fragment, "text"))
                .filter(|text| !text.is_empty())
                .map(ContentBlock::text)
                .collect();
            vec![Record {
                id: RecordId::scoped(&info.source, "message", format!("{artifact}:{identity}")),
                source: info.source.clone(),
                session,
                invocation: record_invocation,
                timestamp: event_timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: event_id.map(str::to_owned),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::System,
                    agent_id: None,
                    data: EventData::Message(Message {
                        role: MessageRole::Developer,
                        phase: None,
                        content,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("AgentMessage" | "agent_message") => {
            vec![Record {
                id: RecordId::scoped(&info.source, "message", format!("{artifact}:{identity}")),
                source: info.source.clone(),
                session,
                invocation: record_invocation,
                timestamp: event_timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: event_id.map(str::to_owned),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::Message(Message {
                        role: MessageRole::Assistant,
                        phase: string(event, "phase").map(message_phase),
                        content: normalize_content(event.get("content").unwrap_or(&Value::Null)),
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("Plan" | "plan") => vec![Record {
            id: RecordId::scoped(&info.source, "event", format!("{artifact}:{identity}")),
            source: info.source.clone(),
            session,
            invocation: record_invocation,
            timestamp: event_timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: event_id.map(str::to_owned),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::Agent,
                agent_id: None,
                data: EventData::Plan(Plan {
                    text: optional_nonempty_string(event, "text"),
                    steps: Vec::new(),
                }),
            }),
            original: Some(original),
        }],
        Some("Reasoning" | "reasoning") => {
            vec![Record {
                id: RecordId::scoped(&info.source, "reasoning", format!("{artifact}:{identity}")),
                source: info.source.clone(),
                session,
                invocation: record_invocation,
                timestamp: event_timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: event_id.map(str::to_owned),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::Reasoning(Reasoning {
                        summary: normalize_reasoning_summary(
                            event.get("summary_text").unwrap_or(&Value::Null),
                        ),
                        content: event
                            .get("raw_content")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(ContentBlock::text)
                            .collect(),
                        visibility: ReasoningVisibility::Visible,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("CommandExecution" | "command_execution") => {
            let input = serde_json::json!({
                "command": event.get("command").cloned().unwrap_or(Value::Null),
                "cwd": event.get("cwd").cloned().unwrap_or(Value::Null),
                "source": event.get("source").cloned().unwrap_or(Value::Null),
                "interaction_input": event
                    .get("interaction_input")
                    .cloned()
                    .unwrap_or(Value::Null),
                "plugin_id": event.get("plugin_id").cloned().unwrap_or(Value::Null),
                "script_path": event.get("script_path").cloned().unwrap_or(Value::Null),
            });
            let observed = ObservedTool::new(
                "exec_command",
                string(event, "plugin_id").filter(|value| !value.is_empty()),
                &input,
            );
            let status = string(event, "status")
                .map(tool_status)
                .unwrap_or(if terminal {
                    ToolStatus::Completed
                } else {
                    ToolStatus::InProgress
                });
            let output = serde_json::json!({
                "stdout": event.get("stdout").cloned().unwrap_or(Value::Null),
                "stderr": event.get("stderr").cloned().unwrap_or(Value::Null),
                "aggregated_output": event
                    .get("aggregated_output")
                    .cloned()
                    .unwrap_or(Value::Null),
                "formatted_output": event
                    .get("formatted_output")
                    .cloned()
                    .unwrap_or(Value::Null),
                "exit_code": event.get("exit_code").cloned().unwrap_or(Value::Null),
            });
            let content = ["aggregated_output", "formatted_output", "stdout", "stderr"]
                .into_iter()
                .find_map(|key| {
                    string(event, key)
                        .filter(|value| !value.is_empty())
                        .map(ContentBlock::text)
                })
                .into_iter()
                .collect();
            materialized_tool_records(
                info,
                artifact,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                MaterializedTool {
                    call_id: identity.to_owned(),
                    observed,
                    title: None,
                    status,
                    error: tool_output_error(event, status),
                    duration_ms: duration_millis(event.get("duration")),
                    output,
                    content,
                    terminal,
                    derive_file_changes: false,
                },
            )
        }
        Some("DynamicToolCall" | "dynamic_tool_call") => {
            let input = event.get("arguments").cloned().unwrap_or(Value::Null);
            let observed = ObservedTool::new(
                string(event, "tool").unwrap_or_default(),
                string(event, "namespace"),
                &input,
            );
            let status = string(event, "status")
                .map(tool_status)
                .unwrap_or(if terminal {
                    ToolStatus::Completed
                } else {
                    ToolStatus::InProgress
                });
            let output = event.get("content_items").cloned().unwrap_or(Value::Null);
            materialized_tool_records(
                info,
                artifact,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                MaterializedTool {
                    call_id: identity.to_owned(),
                    observed,
                    title: None,
                    status,
                    content: normalize_content(&output),
                    output,
                    error: optional_nonempty_string(event, "error"),
                    duration_ms: duration_millis(event.get("duration")),
                    terminal,
                    derive_file_changes: true,
                },
            )
        }
        Some("CollabAgentToolCall" | "collab_agent_tool_call") => {
            normalize_materialized_agent_call(
                info,
                artifact,
                identity,
                event_id,
                sequence,
                event_timestamp,
                session,
                record_invocation,
                origin,
                original,
                event,
                terminal,
            )
        }
        Some("SubAgentActivity" | "sub_agent_activity") => {
            normalize_materialized_subagent_activity(
                info,
                artifact,
                identity,
                event_id,
                sequence,
                event_timestamp,
                session,
                record_invocation,
                origin,
                original,
                event,
            )
        }
        Some("WebSearch" | "web_search") => {
            let input = serde_json::json!({
                "query": event.get("query").cloned().unwrap_or(Value::Null),
                "action": event.get("action").cloned().unwrap_or(Value::Null),
            });
            let observed = ObservedTool::new("web_search", None, &input);
            let output = serde_json::json!({
                "results": event.get("results").cloned().unwrap_or(Value::Null),
                "action": event.get("action").cloned().unwrap_or(Value::Null),
            });
            materialized_tool_records(
                info,
                artifact,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                MaterializedTool {
                    call_id: identity.to_owned(),
                    observed,
                    title: None,
                    status: if terminal {
                        ToolStatus::Completed
                    } else {
                        ToolStatus::InProgress
                    },
                    output,
                    content: Vec::new(),
                    error: None,
                    duration_ms: None,
                    terminal,
                    derive_file_changes: false,
                },
            )
        }
        Some("ImageView" | "image_view") => {
            let input =
                serde_json::json!({ "path": event.get("path").cloned().unwrap_or(Value::Null) });
            let observed = ObservedTool::new("view_image", None, &input);
            let path = string(event, "path").filter(|value| !value.is_empty());
            materialized_tool_records(
                info,
                artifact,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                MaterializedTool {
                    call_id: identity.to_owned(),
                    observed,
                    title: None,
                    status: if terminal {
                        ToolStatus::Completed
                    } else {
                        ToolStatus::InProgress
                    },
                    output: input,
                    content: path
                        .map(|path| ContentBlock::Image {
                            mime_type: None,
                            uri: Some(path.to_owned()),
                            data: None,
                            annotations: None,
                        })
                        .into_iter()
                        .collect(),
                    error: None,
                    duration_ms: None,
                    terminal,
                    derive_file_changes: false,
                },
            )
        }
        Some("Extension" | "extension")
            if string(event, "kind") == Some("image_gen.generation") =>
        {
            normalize_materialized_image_generation(
                info,
                artifact,
                identity,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                event,
                terminal,
            )
        }
        Some("Extension" | "extension") if string(event, "kind") == Some("web.search") => {
            let input = serde_json::json!({
                "query": event.get("query").cloned().unwrap_or(Value::Null),
                "action": event.get("action").cloned().unwrap_or(Value::Null),
            });
            let observed = ObservedTool::new("web_search", Some("web"), &input);
            let output = serde_json::json!({
                "results": event.get("results").cloned().unwrap_or(Value::Null),
                "action": event.get("action").cloned().unwrap_or(Value::Null),
            });
            materialized_tool_records(
                info,
                artifact,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                MaterializedTool {
                    call_id: identity.to_owned(),
                    observed,
                    title: None,
                    status: if terminal {
                        ToolStatus::Completed
                    } else {
                        ToolStatus::InProgress
                    },
                    output,
                    content: Vec::new(),
                    error: None,
                    duration_ms: None,
                    terminal,
                    derive_file_changes: false,
                },
            )
        }
        Some("Extension" | "extension") if string(event, "kind") == Some("clock.sleep") => {
            let input = serde_json::json!({
                "duration_ms": event
                    .get("durationMs")
                    .or_else(|| event.get("duration_ms"))
                    .cloned()
                    .unwrap_or(Value::Null)
            });
            materialized_tool_records(
                info,
                artifact,
                sequence,
                event_timestamp,
                context,
                session,
                record_invocation,
                origin,
                original,
                MaterializedTool {
                    call_id: identity.to_owned(),
                    observed: ObservedTool::new("sleep", Some("clock"), &input),
                    title: None,
                    status: if terminal {
                        ToolStatus::Completed
                    } else {
                        ToolStatus::InProgress
                    },
                    output: Value::Null,
                    content: Vec::new(),
                    error: None,
                    duration_ms: event
                        .get("durationMs")
                        .or_else(|| event.get("duration_ms"))
                        .and_then(Value::as_i64),
                    terminal,
                    derive_file_changes: false,
                },
            )
        }
        Some("ImageGeneration" | "image_generation") => normalize_materialized_image_generation(
            info,
            artifact,
            identity,
            sequence,
            event_timestamp,
            context,
            session,
            record_invocation,
            origin,
            original,
            event,
            terminal,
        ),
        Some("EnteredReviewMode" | "entered_review_mode")
        | Some("ExitedReviewMode" | "exited_review_mode") => normalize_materialized_mode_change(
            info,
            artifact,
            identity,
            event_id,
            sequence,
            event_timestamp,
            session,
            record_invocation,
            origin,
            original,
            event,
        ),
        Some("FileChange" | "file_change") => normalize_materialized_file_change(
            info,
            artifact,
            identity,
            sequence,
            event_timestamp,
            context,
            session,
            record_invocation,
            origin,
            original,
            event,
            terminal,
        ),
        Some("McpToolCall" | "mcp_tool_call") => normalize_materialized_mcp_call(
            info,
            artifact,
            identity,
            sequence,
            event_timestamp,
            context,
            session,
            record_invocation,
            origin,
            original,
            event,
            terminal,
        ),
        Some("ContextCompaction" | "context_compaction") => vec![Record {
            id: RecordId::scoped(&info.source, "event", format!("{artifact}:{identity}")),
            source: info.source.clone(),
            session,
            invocation: record_invocation,
            timestamp: event_timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: event_id.map(str::to_owned),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::ContextCompaction(normalize_compaction(event)),
            }),
            original: Some(original),
        }],
        _ => vec![unknown_event_record(
            info,
            artifact,
            position,
            sequence,
            event_timestamp,
            session,
            record_invocation,
            origin,
            original,
            event,
            event_kind,
        )],
    }
}

fn materialized_event_timestamp(payload: &Value, terminal: bool) -> Option<Timestamp> {
    let key = if terminal {
        "completed_at_ms"
    } else {
        "started_at_ms"
    };
    payload
        .get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .map(Timestamp::from_millis)
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_agent_call(
    info: &ProviderInfo,
    artifact: &str,
    identity: &str,
    event_id: Option<&str>,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    event: &Value,
    terminal: bool,
) -> Vec<Record> {
    let tool = string(event, "tool").unwrap_or_default();
    let operation = match tool {
        "spawn_agent" => AgentOperation::Spawn,
        "send_input" => AgentOperation::SendInput,
        "resume_agent" => AgentOperation::Resume,
        "wait" => AgentOperation::Wait,
        "close_agent" => AgentOperation::Close,
        value => AgentOperation::Other(value.to_owned()),
    };
    let status = match string(event, "status") {
        Some("in_progress" | "inProgress") => AgentInvocationStatus::InProgress,
        Some("completed") => AgentInvocationStatus::Completed,
        Some("failed") => AgentInvocationStatus::Failed,
        Some(_) => AgentInvocationStatus::Unknown,
        None if terminal => AgentInvocationStatus::Completed,
        None => AgentInvocationStatus::InProgress,
    };
    let receiver_ids = event
        .get("receiver_thread_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let input = serde_json::json!({
        "prompt": event.get("prompt").cloned().unwrap_or(Value::Null),
        "model": event.get("model").cloned().unwrap_or(Value::Null),
        "reasoning_effort": event
            .get("reasoning_effort")
            .cloned()
            .unwrap_or(Value::Null),
    });
    let output = event
        .get("agents_states")
        .filter(|value| !value.is_null())
        .cloned();
    vec![Record {
        id: RecordId::scoped(
            &info.source,
            "agent-invocation",
            format!("{artifact}:{identity}"),
        ),
        source: info.source.clone(),
        session,
        invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: event_id.map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: optional_nonempty_string(event, "sender_thread_id"),
            data: EventData::AgentInvocation(AgentInvocation {
                invocation_id: identity.to_owned(),
                context_id: None,
                task_id: None,
                operation: operation.clone(),
                sender_id: optional_nonempty_string(event, "sender_thread_id"),
                child_session: matches!(operation, AgentOperation::Spawn)
                    .then(|| receiver_ids.first().map(|id| session_id(info, id)))
                    .flatten(),
                receiver_ids,
                status,
                started_at: None,
                completed_at: None,
                duration_ms: None,
                stop_reason: None,
                trace_id: None,
                model_context_window: None,
                time_to_first_token_ms: None,
                input: Some(input),
                output,
                artifacts: Vec::new(),
                error: None,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_subagent_activity(
    info: &ProviderInfo,
    artifact: &str,
    identity: &str,
    event_id: Option<&str>,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    event: &Value,
) -> Vec<Record> {
    let agent_thread_id = string(event, "agent_thread_id")
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let (operation, status) = match string(event, "kind").unwrap_or_default() {
        "started" => (AgentOperation::Spawn, AgentInvocationStatus::InProgress),
        "interacted" => (
            AgentOperation::Other("interacted".to_owned()),
            AgentInvocationStatus::Completed,
        ),
        "interrupted" => (
            AgentOperation::Other("interrupted".to_owned()),
            AgentInvocationStatus::Interrupted,
        ),
        value => (
            AgentOperation::Other(value.to_owned()),
            AgentInvocationStatus::Unknown,
        ),
    };
    vec![Record {
        id: RecordId::scoped(
            &info.source,
            "agent-invocation",
            format!("{artifact}:{identity}"),
        ),
        source: info.source.clone(),
        session,
        invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: event_id.map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: optional_nonempty_string(event, "agent_path"),
            data: EventData::AgentInvocation(AgentInvocation {
                invocation_id: identity.to_owned(),
                context_id: None,
                task_id: None,
                operation,
                sender_id: None,
                receiver_ids: agent_thread_id.clone().into_iter().collect(),
                child_session: agent_thread_id.map(|id| session_id(info, &id)),
                status,
                started_at: None,
                completed_at: None,
                duration_ms: None,
                stop_reason: None,
                trace_id: None,
                model_context_window: None,
                time_to_first_token_ms: None,
                input: None,
                output: None,
                artifacts: Vec::new(),
                error: None,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_mode_change(
    info: &ProviderInfo,
    artifact: &str,
    identity: &str,
    event_id: Option<&str>,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    event: &Value,
) -> Vec<Record> {
    let entered = matches!(
        event.get("type").and_then(Value::as_str),
        Some("EnteredReviewMode" | "entered_review_mode")
    );
    let description = if entered {
        optional_nonempty_string(event, "user_facing_hint")
    } else {
        event
            .get("review_output")
            .and_then(|review| optional_nonempty_string(review, "overall_explanation"))
    };
    vec![Record {
        id: RecordId::scoped(
            &info.source,
            "mode-change",
            format!("{artifact}:{identity}"),
        ),
        source: info.source.clone(),
        session,
        invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: event_id.map(str::to_owned),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::System,
            agent_id: None,
            data: EventData::ModeChange(ModeChange {
                mode: "review".to_owned(),
                kind: if entered {
                    ModeChangeKind::Entered
                } else {
                    ModeChangeKind::Exited
                },
                description,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_image_generation(
    info: &ProviderInfo,
    artifact: &str,
    identity: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    event: &Value,
    terminal: bool,
) -> Vec<Record> {
    let status = string(event, "status")
        .map(tool_status)
        .unwrap_or(if terminal {
            ToolStatus::Completed
        } else {
            ToolStatus::InProgress
        });
    let saved_path = string(event, "saved_path")
        .or_else(|| string(event, "savedPath"))
        .filter(|value| !value.is_empty());
    let result = string(event, "result").filter(|value| !value.is_empty());
    let input = serde_json::json!({
        "prompt": event
            .get("revised_prompt")
            .or_else(|| event.get("revisedPrompt"))
            .cloned()
            .unwrap_or(Value::Null),
        "saved_path": event
            .get("saved_path")
            .or_else(|| event.get("savedPath"))
            .cloned()
            .unwrap_or(Value::Null),
    });
    let output = serde_json::json!({
        "result": event.get("result").cloned().unwrap_or(Value::Null),
        "saved_path": event
            .get("saved_path")
            .or_else(|| event.get("savedPath"))
            .cloned()
            .unwrap_or(Value::Null),
        "status": event.get("status").cloned().unwrap_or(Value::Null),
    });
    let mut records = materialized_tool_records(
        info,
        artifact,
        sequence,
        timestamp,
        context,
        session.clone(),
        invocation.clone(),
        origin.clone(),
        original,
        MaterializedTool {
            call_id: identity.to_owned(),
            observed: ObservedTool::new("image_generation", None, &input),
            title: None,
            status,
            output,
            content: vec![ContentBlock::Image {
                mime_type: None,
                uri: saved_path.map(str::to_owned),
                data: result.map(str::to_owned),
                annotations: None,
            }],
            error: None,
            duration_ms: None,
            terminal,
            derive_file_changes: false,
        },
    );
    if terminal {
        if let Some(saved_path) = saved_path {
            records.push(Record {
                id: RecordId::scoped(
                    &info.source,
                    "file-change",
                    format!("{artifact}:{identity}:{saved_path}"),
                ),
                source: info.source.clone(),
                session,
                invocation,
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: None,
                    sequence: sequence.with_part(sequence.part.saturating_add(2)),
                    parent: Some(RecordId::scoped(
                        &info.source,
                        "tool-call",
                        format!("{artifact}:{identity}"),
                    )),
                    inherited_from: None,
                    actor: Actor::Tool,
                    agent_id: None,
                    data: EventData::FileChange(FileChange {
                        path: PathBuf::from(saved_path),
                        old_path: None,
                        kind: FileChangeKind::Create,
                        diff: None,
                        status,
                    }),
                }),
                original: None,
            });
        }
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_file_change(
    info: &ProviderInfo,
    artifact: &str,
    identity: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    event: &Value,
    terminal: bool,
) -> Vec<Record> {
    let status = string(event, "status")
        .map(tool_status)
        .unwrap_or(if terminal {
            ToolStatus::Completed
        } else {
            ToolStatus::InProgress
        });
    let input = serde_json::json!({
        "changes": event.get("changes").cloned().unwrap_or(Value::Null)
    });
    let output = serde_json::json!({
        "stdout": event.get("stdout").cloned().unwrap_or(Value::Null),
        "stderr": event.get("stderr").cloned().unwrap_or(Value::Null),
        "changes": event.get("changes").cloned().unwrap_or(Value::Null),
    });
    let content = ["stdout", "stderr"]
        .into_iter()
        .filter_map(|key| string(event, key))
        .filter(|value| !value.is_empty())
        .map(ContentBlock::text)
        .collect();
    let mut records = materialized_tool_records(
        info,
        artifact,
        sequence,
        timestamp,
        context,
        session.clone(),
        invocation.clone(),
        origin.clone(),
        original,
        MaterializedTool {
            call_id: identity.to_owned(),
            observed: ObservedTool::new("apply_patch", None, &input),
            title: None,
            status,
            output,
            content,
            error: tool_output_error(event, status),
            duration_ms: None,
            terminal,
            derive_file_changes: false,
        },
    );

    let mut changes = event
        .get("changes")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|changes| changes.iter())
        .collect::<Vec<_>>();
    changes.sort_by_key(|(path, _)| *path);
    for (index, (path, change)) in changes.into_iter().enumerate() {
        let Some(file_change) = normalize_file_change(path, change, status) else {
            continue;
        };
        records.push(Record {
            id: RecordId::scoped(
                &info.source,
                "file-change",
                format!("{artifact}:{identity}:{path}"),
            ),
            source: info.source.clone(),
            session: session.clone(),
            invocation: invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: None,
                sequence: sequence.with_part(
                    sequence
                        .part
                        .saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
                        .saturating_add(2),
                ),
                parent: Some(RecordId::scoped(
                    &info.source,
                    "tool-call",
                    format!("{artifact}:{identity}"),
                )),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::FileChange(file_change),
            }),
            original: None,
        });
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn normalize_materialized_mcp_call(
    info: &ProviderInfo,
    artifact: &str,
    identity: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    event: &Value,
    terminal: bool,
) -> Vec<Record> {
    let input = event.get("arguments").cloned().unwrap_or(Value::Null);
    let server = string(event, "server");
    let observed = ObservedTool::new(string(event, "tool").unwrap_or_default(), server, &input)
        .with_source(ToolSourceKind::Mcp, server);
    let status = string(event, "status")
        .map(tool_status)
        .unwrap_or(if terminal {
            ToolStatus::Completed
        } else {
            ToolStatus::InProgress
        });
    let output = event
        .get("result")
        .cloned()
        .or_else(|| event.get("error").cloned())
        .unwrap_or(Value::Null);
    let error = event.get("error").and_then(|error| {
        error
            .as_str()
            .map(str::to_owned)
            .or_else(|| optional_nonempty_string(error, "message"))
    });
    let terminal = terminal
        || matches!(
            status,
            ToolStatus::Completed
                | ToolStatus::Failed
                | ToolStatus::Cancelled
                | ToolStatus::Declined
        );
    materialized_tool_records(
        info,
        artifact,
        sequence,
        timestamp,
        context,
        session,
        invocation,
        origin,
        original,
        MaterializedTool {
            call_id: identity.to_owned(),
            observed,
            title: optional_nonempty_string(event, "appName")
                .or_else(|| optional_nonempty_string(event, "app_name")),
            status,
            content: event
                .get("result")
                .and_then(|result| result.get("content"))
                .map(normalize_content)
                .unwrap_or_default(),
            output,
            error,
            duration_ms: duration_millis(event.get("duration")),
            terminal,
            derive_file_changes: true,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn normalize_patch_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(call_id) = string(payload, "call_id") else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            payload.get("type").and_then(Value::as_str),
        )];
    };
    let event_kind = payload.get("type").and_then(Value::as_str);
    let terminal = event_kind == Some("patch_apply_end");
    let status = if terminal {
        string(payload, "status")
            .map(tool_status)
            .or_else(|| {
                payload
                    .get("success")
                    .and_then(Value::as_bool)
                    .map(|success| {
                        if success {
                            ToolStatus::Completed
                        } else {
                            ToolStatus::Failed
                        }
                    })
            })
            .unwrap_or(ToolStatus::Unknown)
    } else {
        ToolStatus::InProgress
    };
    let call_record_id =
        RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
    let input = serde_json::json!({
        "changes": payload.get("changes").cloned().unwrap_or(Value::Null)
    });
    let observed =
        ObservedTool::new("apply_patch", None, &input).with_source(ToolSourceKind::BuiltIn, None);
    context
        .tool_calls
        .insert(call_id.to_owned(), observed.clone());
    if terminal {
        remember_terminal_tool(context, call_id, status);
    }
    let mut records = vec![Record {
        id: call_record_id.clone(),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin: origin.clone(),
        data: RecordData::Event(Event {
            external_id: Some(call_id.to_owned()),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: None,
            data: EventData::ToolCall(ToolCall {
                call_id: call_id.to_owned(),
                name: observed.name,
                namespace: observed.namespace,
                source_kind: observed.source_kind,
                server_name: observed.server_name,
                title: None,
                kind: observed.kind,
                status,
                input,
                locations: observed.locations,
            }),
        }),
        original: Some(original),
    }];

    if terminal {
        let stdout = string(payload, "stdout").unwrap_or_default();
        let stderr = string(payload, "stderr").unwrap_or_default();
        let output = serde_json::json!({
            "stdout": stdout,
            "stderr": stderr,
            "changes": payload.get("changes").cloned().unwrap_or(Value::Null),
        });
        let content = [stdout, stderr]
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(ContentBlock::text)
            .collect();
        let error = matches!(
            status,
            ToolStatus::Failed | ToolStatus::Declined | ToolStatus::Cancelled
        )
        .then(|| {
            if stderr.is_empty() {
                stdout.to_owned()
            } else {
                stderr.to_owned()
            }
        })
        .filter(|value| !value.is_empty());
        records.push(Record {
            id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence: sequence.with_part(1),
                parent: Some(call_record_id.clone()),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::ToolResult(ToolResult {
                    call_id: call_id.to_owned(),
                    name: Some("apply_patch".to_owned()),
                    content,
                    output,
                    status,
                    error,
                    duration_ms: None,
                }),
            }),
            original: None,
        });
    }

    let mut changes: Vec<_> = payload
        .get("changes")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|changes| changes.iter())
        .collect();
    changes.sort_by_key(|(path, _)| *path);
    for (index, (path, change)) in changes.into_iter().enumerate() {
        let Some(file_change) = normalize_file_change(path, change, status) else {
            continue;
        };
        records.push(Record {
            id: RecordId::scoped(
                &info.source,
                "file-change",
                format!("{artifact}:{call_id}:{path}"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: None,
                sequence: sequence
                    .with_part(u32::try_from(index).unwrap_or(u32::MAX).saturating_add(2)),
                parent: Some(call_record_id.clone()),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::FileChange(file_change),
            }),
            original: None,
        });
    }
    records
}

fn normalize_file_change(path: &str, value: &Value, status: ToolStatus) -> Option<FileChange> {
    match value.get("type").and_then(Value::as_str)? {
        "add" => Some(FileChange {
            path: PathBuf::from(path),
            old_path: None,
            kind: FileChangeKind::Create,
            diff: optional_nonempty_string(value, "content"),
            status,
        }),
        "delete" => Some(FileChange {
            path: PathBuf::from(path),
            old_path: None,
            kind: FileChangeKind::Delete,
            diff: optional_nonempty_string(value, "content"),
            status,
        }),
        "update" => {
            let move_path = optional_nonempty_string(value, "move_path").map(PathBuf::from);
            Some(FileChange {
                path: move_path.clone().unwrap_or_else(|| PathBuf::from(path)),
                old_path: move_path.map(|_| PathBuf::from(path)),
                kind: if value
                    .get("move_path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !path.is_empty())
                {
                    FileChangeKind::Move
                } else {
                    FileChangeKind::Update
                },
                diff: optional_nonempty_string(value, "unified_diff"),
                status,
            })
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn normalize_mcp_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(call_id) = string(payload, "call_id") else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            payload.get("type").and_then(Value::as_str),
        )];
    };
    let invocation = payload.get("invocation").unwrap_or(&Value::Null);
    let server = string(invocation, "server").unwrap_or_default();
    let tool = string(invocation, "tool").unwrap_or_default();
    let input = invocation.get("arguments").cloned().unwrap_or(Value::Null);
    let observed = ObservedTool::new(tool, (!server.is_empty()).then_some(server), &input)
        .with_source(ToolSourceKind::Mcp, (!server.is_empty()).then_some(server));
    let terminal = payload.get("type").and_then(Value::as_str) == Some("mcp_tool_call_end");
    let result = payload.get("result").unwrap_or(&Value::Null);
    let ok = result.get("Ok");
    let provider_error = result.get("Err");
    let failed = provider_error.is_some()
        || ok
            .and_then(|value| value.get("is_error"))
            .and_then(Value::as_bool)
            == Some(true);
    let status = if terminal {
        if failed {
            ToolStatus::Failed
        } else {
            ToolStatus::Completed
        }
    } else {
        ToolStatus::InProgress
    };
    let call_record_id =
        RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
    context
        .tool_calls
        .insert(call_id.to_owned(), observed.clone());
    if terminal {
        remember_terminal_tool(context, call_id, status);
    }
    let mut records = vec![Record {
        id: call_record_id.clone(),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin: origin.clone(),
        data: RecordData::Event(Event {
            external_id: Some(call_id.to_owned()),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: None,
            data: EventData::ToolCall(ToolCall {
                call_id: call_id.to_owned(),
                name: observed.name.clone(),
                namespace: observed.namespace.clone(),
                source_kind: observed.source_kind,
                server_name: observed.server_name.clone(),
                title: (!server.is_empty()).then(|| server.to_owned()),
                kind: observed.kind,
                status,
                input,
                locations: observed.locations,
            }),
        }),
        original: Some(original),
    }];
    if terminal {
        let output = ok
            .cloned()
            .or_else(|| provider_error.cloned())
            .unwrap_or(Value::Null);
        let content_value = ok
            .and_then(|value| value.get("content"))
            .unwrap_or(&Value::Null);
        let content = normalize_content(content_value);
        let error = provider_error
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                failed
                    .then(|| content_text(content_value))
                    .filter(|value| !value.is_empty())
            });
        records.push(Record {
            id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence: sequence.with_part(1),
                parent: Some(call_record_id),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::ToolResult(ToolResult {
                    call_id: call_id.to_owned(),
                    name: Some(observed.name),
                    output,
                    content,
                    status,
                    error,
                    duration_ms: duration_millis(payload.get("duration")),
                }),
            }),
            original: None,
        });
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn normalize_exec_command_end(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(call_id) = string(payload, "call_id") else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("exec_command_end"),
        )];
    };
    let status = string(payload, "status")
        .map(tool_status)
        .unwrap_or_else(|| {
            if payload.get("exit_code").and_then(Value::as_i64) == Some(0) {
                ToolStatus::Completed
            } else {
                ToolStatus::Failed
            }
        });
    let call_record_id =
        RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
    let duration_ms = duration_millis(payload.get("duration"));
    remember_terminal_tool(context, call_id, status);
    let input = serde_json::json!({
        "command": payload.get("command").cloned().unwrap_or(Value::Null),
        "cwd": payload.get("cwd").cloned().unwrap_or(Value::Null),
        "source": payload.get("source").cloned().unwrap_or(Value::Null),
        "interaction_input": payload
            .get("interaction_input")
            .cloned()
            .unwrap_or(Value::Null),
    });
    let observed = ObservedTool::new("exec_command", None, &input);
    context
        .tool_calls
        .insert(call_id.to_owned(), observed.clone());
    let output = serde_json::json!({
        "stdout": payload.get("stdout").cloned().unwrap_or(Value::Null),
        "stderr": payload.get("stderr").cloned().unwrap_or(Value::Null),
        "aggregated_output": payload
            .get("aggregated_output")
            .cloned()
            .unwrap_or(Value::Null),
        "formatted_output": payload
            .get("formatted_output")
            .cloned()
            .unwrap_or(Value::Null),
        "exit_code": payload.get("exit_code").cloned().unwrap_or(Value::Null),
        "status": payload.get("status").cloned().unwrap_or(Value::Null),
    });
    let display_output = ["aggregated_output", "formatted_output", "stdout", "stderr"]
        .into_iter()
        .find_map(|key| {
            string(payload, key)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    let error = matches!(
        status,
        ToolStatus::Failed | ToolStatus::Cancelled | ToolStatus::Declined
    )
    .then(|| {
        ["stderr", "aggregated_output", "formatted_output", "stdout"]
            .into_iter()
            .find_map(|key| {
                string(payload, key)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("command exited with status {status:?}"))
    });

    vec![
        Record {
            id: call_record_id.clone(),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::Agent,
                agent_id: None,
                data: EventData::ToolCall(ToolCall {
                    call_id: call_id.to_owned(),
                    name: observed.name.clone(),
                    namespace: observed.namespace.clone(),
                    source_kind: observed.source_kind,
                    server_name: observed.server_name.clone(),
                    title: None,
                    kind: observed.kind,
                    status,
                    input,
                    locations: observed.locations,
                }),
            }),
            original: Some(original),
        },
        Record {
            id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence: sequence.with_part(1),
                parent: Some(call_record_id),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::ToolResult(ToolResult {
                    call_id: call_id.to_owned(),
                    name: Some(observed.name),
                    content: display_output.map(ContentBlock::text).into_iter().collect(),
                    output,
                    status,
                    error,
                    duration_ms,
                }),
            }),
            original: None,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn normalize_web_search_end(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(call_id) = string(payload, "call_id") else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("web_search_end"),
        )];
    };
    let call_record_id =
        RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
    remember_terminal_tool(context, call_id, ToolStatus::Completed);
    let input = payload
        .get("action")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({ "query": string(payload, "query") }));
    let observed = ObservedTool::new("web_search", None, &input)
        .with_source(ToolSourceKind::ProviderHosted, None);
    context
        .tool_calls
        .insert(call_id.to_owned(), observed.clone());
    let output = serde_json::json!({
        "query": payload.get("query").cloned().unwrap_or(Value::Null),
        "action": payload.get("action").cloned().unwrap_or(Value::Null),
        "results": payload.get("results").cloned().unwrap_or(Value::Null),
    });

    vec![
        Record {
            id: call_record_id.clone(),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::Agent,
                agent_id: None,
                data: EventData::ToolCall(ToolCall {
                    call_id: call_id.to_owned(),
                    name: observed.name.clone(),
                    namespace: observed.namespace.clone(),
                    source_kind: observed.source_kind,
                    server_name: observed.server_name.clone(),
                    title: None,
                    kind: observed.kind,
                    status: ToolStatus::Completed,
                    input,
                    locations: observed.locations,
                }),
            }),
            original: Some(original),
        },
        Record {
            id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence: sequence.with_part(1),
                parent: Some(call_record_id),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::ToolResult(ToolResult {
                    call_id: call_id.to_owned(),
                    name: Some(observed.name),
                    content: Vec::new(),
                    output,
                    status: ToolStatus::Completed,
                    error: None,
                    duration_ms: None,
                }),
            }),
            original: None,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn normalize_image_generation_end(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(call_id) = string(payload, "call_id") else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("image_generation_end"),
        )];
    };
    let status = string(payload, "status")
        .map(tool_status)
        .unwrap_or(ToolStatus::Unknown);
    remember_terminal_tool(context, call_id, status);
    let result = string(payload, "result").filter(|value| !value.is_empty());
    let saved_path = string(payload, "saved_path").filter(|value| !value.is_empty());
    let output = serde_json::json!({
        "result": payload.get("result").cloned().unwrap_or(Value::Null),
        "revised_prompt": payload
            .get("revised_prompt")
            .cloned()
            .unwrap_or(Value::Null),
        "saved_path": payload.get("saved_path").cloned().unwrap_or(Value::Null),
        "status": payload.get("status").cloned().unwrap_or(Value::Null),
    });
    let input = serde_json::json!({
        "prompt": payload.get("revised_prompt").cloned().unwrap_or(Value::Null),
        "saved_path": payload.get("saved_path").cloned().unwrap_or(Value::Null),
    });
    let observed = ObservedTool::new("image_generation", None, &input)
        .with_source(ToolSourceKind::ProviderHosted, None);
    context
        .tool_calls
        .insert(call_id.to_owned(), observed.clone());
    let call_record_id =
        RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
    let mut records = vec![
        Record {
            id: call_record_id.clone(),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::Agent,
                agent_id: None,
                data: EventData::ToolCall(ToolCall {
                    call_id: call_id.to_owned(),
                    name: observed.name.clone(),
                    namespace: observed.namespace.clone(),
                    source_kind: observed.source_kind,
                    server_name: observed.server_name.clone(),
                    title: None,
                    kind: observed.kind,
                    status,
                    input,
                    locations: observed.locations,
                }),
            }),
            original: Some(original),
        },
        Record {
            id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence: sequence.with_part(sequence.part.saturating_add(1)),
                parent: Some(call_record_id.clone()),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::ToolResult(ToolResult {
                    call_id: call_id.to_owned(),
                    name: Some(observed.name),
                    output,
                    content: vec![ContentBlock::Image {
                        mime_type: None,
                        uri: saved_path.map(str::to_owned),
                        data: result.map(str::to_owned),
                        annotations: None,
                    }],
                    status,
                    error: None,
                    duration_ms: None,
                }),
            }),
            original: None,
        },
    ];
    if let Some(saved_path) = saved_path {
        records.push(Record {
            id: RecordId::scoped(
                &info.source,
                "file-change",
                format!("{artifact}:{call_id}:{saved_path}"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: None,
                sequence: sequence.with_part(sequence.part.saturating_add(2)),
                parent: Some(call_record_id),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::FileChange(FileChange {
                    path: PathBuf::from(saved_path),
                    old_path: None,
                    kind: FileChangeKind::Create,
                    diff: None,
                    status,
                }),
            }),
            original: None,
        });
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn normalize_view_image(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let (Some(call_id), Some(path)) = (string(payload, "call_id"), string(payload, "path")) else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("view_image_tool_call"),
        )];
    };
    let call_record_id =
        RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
    let value = serde_json::json!({ "path": path });
    vec![
        Record {
            id: call_record_id.clone(),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin: origin.clone(),
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::Agent,
                agent_id: None,
                data: EventData::ToolCall(ToolCall {
                    call_id: call_id.to_owned(),
                    name: "view_image".to_owned(),
                    namespace: None,
                    source_kind: ToolSourceKind::BuiltIn,
                    server_name: None,
                    title: None,
                    kind: ToolKind::Read,
                    status: ToolStatus::Completed,
                    input: value.clone(),
                    locations: vec![ToolLocation {
                        path: PathBuf::from(path),
                        line: None,
                    }],
                }),
            }),
            original: Some(original),
        },
        Record {
            id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: context.current_invocation.clone(),
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: Some(call_id.to_owned()),
                sequence: sequence.with_part(1),
                parent: Some(call_record_id),
                inherited_from: None,
                actor: Actor::Tool,
                agent_id: None,
                data: EventData::ToolResult(ToolResult {
                    call_id: call_id.to_owned(),
                    name: Some("view_image".to_owned()),
                    output: value,
                    content: vec![ContentBlock::Image {
                        mime_type: None,
                        uri: Some(path.to_owned()),
                        data: None,
                        annotations: None,
                    }],
                    status: ToolStatus::Completed,
                    error: None,
                    duration_ms: None,
                }),
            }),
            original: None,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn normalize_subagent_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let Some(event_id) = string(payload, "event_id") else {
        return vec![unknown_record(
            info,
            artifact,
            position,
            timestamp,
            context,
            origin,
            original,
            Some("sub_agent_activity"),
        )];
    };
    let agent_thread_id = string(payload, "agent_thread_id").unwrap_or_default();
    let agent_path = string(payload, "agent_path").map(str::to_owned);
    let kind = string(payload, "kind").unwrap_or_default();
    let (operation, status) = match kind {
        "started" => (AgentOperation::Spawn, AgentInvocationStatus::InProgress),
        "interacted" => (
            AgentOperation::Other("interacted".to_owned()),
            AgentInvocationStatus::Completed,
        ),
        "interrupted" => (
            AgentOperation::Other("interrupted".to_owned()),
            AgentInvocationStatus::Interrupted,
        ),
        other => (
            AgentOperation::Other(other.to_owned()),
            AgentInvocationStatus::Unknown,
        ),
    };
    let occurred_at = payload
        .get("occurred_at_ms")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .map(Timestamp::from_millis)
        .or(timestamp);
    vec![Record {
        id: RecordId::scoped(
            &info.source,
            "agent-invocation",
            format!("{artifact}:{event_id}"),
        ),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp: occurred_at,
        origin,
        data: RecordData::Event(Event {
            external_id: Some(event_id.to_owned()),
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: agent_path,
            data: EventData::AgentInvocation(AgentInvocation {
                invocation_id: event_id.to_owned(),
                context_id: context.session_external_id.clone(),
                task_id: None,
                operation,
                sender_id: context.session_external_id.clone(),
                receiver_ids: if agent_thread_id.is_empty() {
                    Vec::new()
                } else {
                    vec![agent_thread_id.to_owned()]
                },
                child_session: (!agent_thread_id.is_empty())
                    .then(|| session_id(info, agent_thread_id)),
                status,
                started_at: None,
                completed_at: None,
                duration_ms: None,
                stop_reason: None,
                trace_id: None,
                model_context_window: None,
                time_to_first_token_ms: None,
                input: None,
                output: None,
                artifacts: Vec::new(),
                error: None,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_plan_event(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let steps = payload
        .get("plan")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let text = string(step, "step")?.to_owned();
            let status = match string(step, "status").unwrap_or_default() {
                "pending" => PlanStepStatus::Pending,
                "in_progress" => PlanStepStatus::InProgress,
                "completed" => PlanStepStatus::Completed,
                "cancelled" | "canceled" => PlanStepStatus::Cancelled,
                "failed" => PlanStepStatus::Failed,
                "skipped" => PlanStepStatus::Skipped,
                _ => PlanStepStatus::Unknown,
            };
            Some(PlanStep {
                text,
                status,
                priority: None,
            })
        })
        .collect();
    vec![Record {
        id: RecordId::scoped(&info.source, "event", format!("{artifact}:{position}:plan")),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id: None,
            sequence,
            parent: None,
            inherited_from: None,
            actor: Actor::Agent,
            agent_id: None,
            data: EventData::Plan(Plan {
                text: optional_nonempty_string(payload, "explanation"),
                steps,
            }),
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_terminal_invocation(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
    aborted: bool,
) -> Vec<Record> {
    let external_id = string(payload, "turn_id")
        .map(str::to_owned)
        .or_else(|| context.current_invocation_external_id.clone());
    let id = external_id
        .as_deref()
        .map(|value| invocation_id(info, artifact, value))
        .or_else(|| context.current_invocation.clone())
        .unwrap_or_else(|| {
            RecordId::scoped(
                &info.source,
                "agent-invocation",
                format!("{artifact}:{position}"),
            )
        });
    let error = payload
        .get("error")
        .filter(|value| !value.is_null())
        .map(error_message);
    let abort_reason = string(payload, "reason");
    let status = if aborted {
        match abort_reason {
            Some("cancelled" | "canceled") => AgentInvocationStatus::Cancelled,
            _ => AgentInvocationStatus::Interrupted,
        }
    } else if error.is_some() {
        AgentInvocationStatus::Failed
    } else {
        AgentInvocationStatus::Completed
    };
    let stop_reason = if aborted {
        match abort_reason {
            Some("cancelled" | "canceled") => StopReason::Cancelled,
            Some("interrupted") | None => StopReason::Interrupted,
            Some(reason) => StopReason::Other(reason.to_owned()),
        }
    } else if error.is_some() {
        StopReason::Failed
    } else {
        StopReason::EndInvocation
    };
    let started_at =
        integer_timestamp(payload.get("started_at")).or(context.current_invocation_started_at);
    let completed_at = integer_timestamp(payload.get("completed_at")).or(timestamp);
    let duration_ms = payload
        .get("duration_ms")
        .and_then(Value::as_i64)
        .or_else(|| match (started_at, completed_at) {
            (Some(started), Some(completed)) => Some(
                completed
                    .as_millis()
                    .saturating_sub(started.as_millis())
                    .max(0),
            ),
            _ => None,
        });
    let trace_id = optional_nonempty_string(payload, "trace_id")
        .or_else(|| context.current_invocation_trace_id.clone());
    let model_context_window = payload
        .get("model_context_window")
        .or_else(|| payload.get("context_window"))
        .and_then(Value::as_i64)
        .or(context.current_invocation_model_context_window);
    let time_to_first_token_ms = payload
        .get("time_to_first_token_ms")
        .or_else(|| payload.get("ttft_ms"))
        .and_then(Value::as_i64)
        .or(context.current_invocation_time_to_first_token_ms);
    context.current_invocation = None;
    context.current_invocation_external_id = None;
    context.current_invocation_inferred = false;
    context.current_invocation_started_at = None;
    context.current_invocation_trace_id = None;
    context.current_invocation_model_context_window = None;
    context.current_invocation_time_to_first_token_ms = None;
    context.tool_calls.clear();
    vec![Record {
        id,
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: None,
        timestamp,
        origin,
        data: RecordData::AgentInvocation(AgentInvocation {
            invocation_id: external_id.unwrap_or_default(),
            context_id: None,
            task_id: None,
            operation: AgentOperation::Invoke,
            sender_id: None,
            receiver_ids: Vec::new(),
            child_session: None,
            status,
            started_at,
            completed_at,
            duration_ms,
            input: None,
            output: None,
            artifacts: Vec::new(),
            error,
            stop_reason: Some(stop_reason),
            trace_id,
            model_context_window,
            time_to_first_token_ms,
        }),
        original: Some(original),
    }]
}

#[allow(clippy::too_many_arguments)]
fn normalize_response_item(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    context: &mut RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
) -> Vec<Record> {
    let record_invocation = response_item_invocation(info, artifact, payload, context);
    match payload.get("type").and_then(Value::as_str) {
        Some("agent_message") => {
            let event_id = string(payload, "id").unwrap_or(position).to_owned();
            let sender = optional_nonempty_string(payload, "author");
            let mut receiver_ids = optional_nonempty_string(payload, "recipient")
                .into_iter()
                .collect::<Vec<_>>();
            receiver_ids.extend(
                payload
                    .get("other_recipients")
                    .or_else(|| payload.get("otherRecipients"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            );
            let task_id = payload
                .get("internal_chat_message_metadata_passthrough")
                .and_then(|metadata| string(metadata, "turn_id"))
                .map(str::to_owned);
            vec![Record {
                id: RecordId::scoped(
                    &info.source,
                    "agent-invocation",
                    format!("{artifact}:{event_id}"),
                ),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: Some(event_id.clone()),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: sender.clone(),
                    data: EventData::AgentInvocation(AgentInvocation {
                        invocation_id: event_id,
                        context_id: context.session_external_id.clone(),
                        task_id,
                        operation: AgentOperation::SendInput,
                        sender_id: sender,
                        receiver_ids,
                        child_session: None,
                        status: AgentInvocationStatus::Completed,
                        started_at: None,
                        completed_at: None,
                        duration_ms: None,
                        stop_reason: None,
                        trace_id: None,
                        model_context_window: None,
                        time_to_first_token_ms: None,
                        input: Some(payload.get("content").cloned().unwrap_or(Value::Null)),
                        output: None,
                        artifacts: Vec::new(),
                        error: None,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("local_shell_call") => {
            let call_id = string(payload, "call_id")
                .or_else(|| string(payload, "id"))
                .filter(|value| !value.is_empty())
                .unwrap_or(position);
            let input = payload.get("action").cloned().unwrap_or(Value::Null);
            let observed = ObservedTool::new("local_shell", Some("codex"), &input)
                .with_source(ToolSourceKind::BuiltIn, None);
            let status = string(payload, "status")
                .map(tool_status)
                .unwrap_or(ToolStatus::Unknown);
            if matches!(status, ToolStatus::InProgress | ToolStatus::Pending) {
                context
                    .tool_calls
                    .insert(call_id.to_owned(), observed.clone());
            } else {
                context.tool_calls.remove(call_id);
                remember_terminal_tool(context, call_id, status);
            }
            vec![Record {
                id: RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation,
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: Some(call_id.to_owned()),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::ToolCall(ToolCall {
                        call_id: call_id.to_owned(),
                        name: observed.name,
                        namespace: observed.namespace,
                        source_kind: observed.source_kind,
                        server_name: observed.server_name,
                        title: None,
                        kind: observed.kind,
                        status,
                        input,
                        locations: observed.locations,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("function_call" | "custom_tool_call") => {
            let Some(call_id) = string(payload, "call_id") else {
                return vec![unknown_record(
                    info,
                    artifact,
                    position,
                    timestamp,
                    context,
                    origin,
                    original,
                    payload.get("type").and_then(Value::as_str),
                )];
            };
            let input = payload
                .get("arguments")
                .or_else(|| payload.get("input"))
                .map(normalize_tool_value)
                .unwrap_or(Value::Null);
            let provider_name = string(payload, "name").unwrap_or_default();
            let namespace = string(payload, "namespace");
            let source_kind =
                if payload.get("type").and_then(Value::as_str) == Some("custom_tool_call") {
                    ToolSourceKind::Custom
                } else {
                    ToolSourceKind::Unknown
                };
            let observed =
                ObservedTool::new(provider_name, namespace, &input).with_source(source_kind, None);
            context
                .tool_calls
                .insert(call_id.to_owned(), observed.clone());
            let status = payload
                .get("status")
                .and_then(Value::as_str)
                .map(tool_status)
                .unwrap_or(ToolStatus::Pending);
            vec![Record {
                id: RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: Some(call_id.to_owned()),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::ToolCall(ToolCall {
                        call_id: call_id.to_owned(),
                        title: None,
                        kind: observed.kind,
                        name: observed.name,
                        namespace: observed.namespace,
                        source_kind: observed.source_kind,
                        server_name: observed.server_name,
                        status,
                        input,
                        locations: observed.locations,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("function_call_output" | "custom_tool_call_output") => {
            let Some(call_id) = string(payload, "call_id") else {
                return vec![unknown_record(
                    info,
                    artifact,
                    position,
                    timestamp,
                    context,
                    origin,
                    original,
                    payload.get("type").and_then(Value::as_str),
                )];
            };
            if context.terminal_tool_results.contains(call_id) {
                // Codex commonly writes a richer terminal event first and
                // then repeats its output as a response event. Keep the
                // terminal record instead of replacing it with a less
                // informative projection.
                return Vec::new();
            }
            let output = payload.get("output").cloned().unwrap_or(Value::Null);
            let observed = context.tool_calls.remove(call_id);
            let status = terminal_tool_status(payload, &output);
            vec![Record {
                id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: Some(call_id.to_owned()),
                    sequence,
                    parent: Some(RecordId::scoped(
                        &info.source,
                        "tool-call",
                        format!("{artifact}:{call_id}"),
                    )),
                    inherited_from: None,
                    actor: Actor::Tool,
                    agent_id: None,
                    data: EventData::ToolResult(ToolResult {
                        call_id: call_id.to_owned(),
                        name: optional_nonempty_string(payload, "name")
                            .or_else(|| observed.as_ref().map(|tool| tool.name.clone())),
                        content: normalize_tool_output_content(&output),
                        output,
                        status,
                        error: tool_output_error(payload, status),
                        duration_ms: None,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("message") => {
            let role = message_role(string(payload, "role").unwrap_or_default());
            let identity = string(payload, "id")
                .filter(|value| !value.is_empty())
                .unwrap_or(position);
            vec![Record {
                id: RecordId::scoped(&info.source, "message", format!("{artifact}:{identity}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: string(payload, "id").map(str::to_owned),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: actor_for_message(&role),
                    agent_id: None,
                    data: EventData::Message(Message {
                        role,
                        phase: string(payload, "phase").map(message_phase),
                        content: normalize_content(payload.get("content").unwrap_or(&Value::Null)),
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("reasoning") => {
            let identity = string(payload, "id")
                .filter(|value| !value.is_empty())
                .unwrap_or(position);
            vec![Record {
                id: RecordId::scoped(&info.source, "reasoning", format!("{artifact}:{identity}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: string(payload, "id").map(str::to_owned),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::Reasoning(Reasoning {
                        summary: normalize_reasoning_summary(
                            payload.get("summary").unwrap_or(&Value::Null),
                        ),
                        content: normalize_content(payload.get("content").unwrap_or(&Value::Null)),
                        visibility: if payload
                            .get("encrypted_content")
                            .and_then(Value::as_str)
                            .is_some_and(|content| !content.is_empty())
                        {
                            ReasoningVisibility::Encrypted
                        } else {
                            ReasoningVisibility::Visible
                        },
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("tool_search_output") => {
            let call_id = string(payload, "call_id")
                .or_else(|| string(payload, "id"))
                .unwrap_or(position);
            let output = payload.clone();
            context.tool_calls.remove(call_id);
            vec![Record {
                id: RecordId::scoped(&info.source, "tool-result", format!("{artifact}:{call_id}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: Some(call_id.to_owned()),
                    sequence,
                    parent: Some(RecordId::scoped(
                        &info.source,
                        "tool-call",
                        format!("{artifact}:{call_id}"),
                    )),
                    inherited_from: None,
                    actor: Actor::Tool,
                    agent_id: None,
                    data: EventData::ToolResult(ToolResult {
                        call_id: call_id.to_owned(),
                        name: Some("tool_search".to_owned()),
                        content: Vec::new(),
                        output,
                        status: string(payload, "status")
                            .map(tool_status)
                            .unwrap_or(ToolStatus::Unknown),
                        error: None,
                        duration_ms: None,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("web_search_call" | "tool_search_call") => {
            let call_id = string(payload, "call_id")
                .or_else(|| string(payload, "id"))
                .unwrap_or(position);
            let name = if payload.get("type").and_then(Value::as_str) == Some("web_search_call") {
                "web_search"
            } else {
                "tool_search"
            };
            let input = payload
                .get("action")
                .or_else(|| payload.get("arguments"))
                .cloned()
                .unwrap_or(Value::Null);
            let observed = ObservedTool::new(name, None, &input);
            context
                .tool_calls
                .insert(call_id.to_owned(), observed.clone());
            vec![Record {
                id: RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}")),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin,
                data: RecordData::Event(Event {
                    external_id: Some(call_id.to_owned()),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::ToolCall(ToolCall {
                        call_id: call_id.to_owned(),
                        name: observed.name,
                        namespace: observed.namespace,
                        source_kind: observed.source_kind,
                        server_name: observed.server_name,
                        title: None,
                        kind: observed.kind,
                        status: payload
                            .get("status")
                            .and_then(Value::as_str)
                            .map(tool_status)
                            .unwrap_or(ToolStatus::Pending),
                        input,
                        locations: observed.locations,
                    }),
                }),
                original: Some(original),
            }]
        }
        Some("image_generation_call") => {
            let call_id = string(payload, "call_id")
                .or_else(|| string(payload, "id"))
                .unwrap_or(position);
            if context.terminal_tool_results.contains(call_id) {
                return Vec::new();
            }
            let input = serde_json::json!({
                "prompt": payload
                    .get("revised_prompt")
                    .or_else(|| payload.get("prompt"))
                    .cloned()
                    .unwrap_or(Value::Null)
            });
            let observed = ObservedTool::new("image_generation", None, &input);
            context
                .tool_calls
                .insert(call_id.to_owned(), observed.clone());
            let status = string(payload, "status")
                .map(tool_status)
                .unwrap_or(ToolStatus::Unknown);
            let call_record_id =
                RecordId::scoped(&info.source, "tool-call", format!("{artifact}:{call_id}"));
            let result = payload.get("result").cloned().unwrap_or(Value::Null);
            let mut records = vec![Record {
                id: call_record_id.clone(),
                source: info.source.clone(),
                session: context.session.clone(),
                invocation: record_invocation.clone(),
                timestamp,
                origin: origin.clone(),
                data: RecordData::Event(Event {
                    external_id: Some(call_id.to_owned()),
                    sequence,
                    parent: None,
                    inherited_from: None,
                    actor: Actor::Agent,
                    agent_id: None,
                    data: EventData::ToolCall(ToolCall {
                        call_id: call_id.to_owned(),
                        name: observed.name.clone(),
                        namespace: observed.namespace,
                        source_kind: observed.source_kind,
                        server_name: observed.server_name,
                        title: None,
                        kind: observed.kind,
                        status,
                        input,
                        locations: observed.locations,
                    }),
                }),
                original: Some(original),
            }];
            if !result.is_null()
                || matches!(
                    status,
                    ToolStatus::Completed
                        | ToolStatus::Failed
                        | ToolStatus::Cancelled
                        | ToolStatus::Declined
                )
            {
                records.push(Record {
                    id: RecordId::scoped(
                        &info.source,
                        "tool-result",
                        format!("{artifact}:{call_id}"),
                    ),
                    source: info.source.clone(),
                    session: context.session.clone(),
                    invocation: record_invocation,
                    timestamp,
                    origin,
                    data: RecordData::Event(Event {
                        external_id: Some(call_id.to_owned()),
                        sequence: sequence.with_part(sequence.part.saturating_add(1)),
                        parent: Some(call_record_id),
                        inherited_from: None,
                        actor: Actor::Tool,
                        agent_id: None,
                        data: EventData::ToolResult(ToolResult {
                            call_id: call_id.to_owned(),
                            name: Some(observed.name),
                            output: result.clone(),
                            content: result
                                .as_str()
                                .map(|data| ContentBlock::Image {
                                    mime_type: None,
                                    uri: None,
                                    data: Some(data.to_owned()),
                                    annotations: None,
                                })
                                .into_iter()
                                .collect(),
                            status,
                            error: tool_output_error(payload, status),
                            duration_ms: None,
                        }),
                    }),
                    original: None,
                });
            }
            records
        }
        Some("compaction" | "compaction_summary" | "context_compaction") => vec![Record {
            id: RecordId::scoped(
                &info.source,
                "event",
                format!("{artifact}:{position}:compaction"),
            ),
            source: info.source.clone(),
            session: context.session.clone(),
            invocation: record_invocation,
            timestamp,
            origin,
            data: RecordData::Event(Event {
                external_id: string(payload, "id").map(str::to_owned),
                sequence,
                parent: None,
                inherited_from: None,
                actor: Actor::System,
                agent_id: None,
                data: EventData::ContextCompaction(normalize_compaction(payload)),
            }),
            original: Some(original),
        }],
        event_kind => vec![unknown_event_record(
            info,
            artifact,
            position,
            sequence,
            timestamp,
            context.session.clone(),
            record_invocation,
            origin,
            original,
            payload,
            event_kind,
        )],
    }
}

fn response_item_invocation(
    info: &ProviderInfo,
    artifact: &str,
    payload: &Value,
    context: &RolloutContext,
) -> Option<RecordId> {
    payload
        .get("internal_chat_message_metadata_passthrough")
        .and_then(|metadata| string(metadata, "turn_id"))
        .filter(|external_id| !external_id.is_empty())
        .map(|external_id| invocation_id(info, artifact, external_id))
        .or_else(|| context.current_invocation.clone())
}

#[allow(clippy::too_many_arguments)]
fn unknown_event_record(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    sequence: EventSequence,
    timestamp: Option<Timestamp>,
    session: Option<RecordId>,
    invocation: Option<RecordId>,
    origin: SourceRef,
    original: OriginalData,
    payload: &Value,
    kind: Option<&str>,
) -> Record {
    let external_id = string(payload, "id")
        .or_else(|| string(payload, "call_id"))
        .map(str::to_owned);
    let identity = external_id.as_deref().unwrap_or(position);
    Record {
        id: RecordId::scoped(&info.source, "event", format!("{artifact}:{identity}")),
        source: info.source.clone(),
        session,
        invocation,
        timestamp,
        origin,
        data: RecordData::Event(Event {
            external_id,
            sequence,
            parent: None,
            inherited_from: None,
            actor: if kind.is_some_and(|kind| kind.ends_with("_output")) {
                Actor::Tool
            } else {
                Actor::Agent
            },
            agent_id: None,
            data: EventData::Unknown(UnknownEvent {
                kind: kind.map(str::to_owned),
            }),
        }),
        original: Some(original),
    }
}

#[allow(clippy::too_many_arguments)]
fn unknown_record(
    info: &ProviderInfo,
    artifact: &str,
    position: &str,
    timestamp: Option<Timestamp>,
    context: &RolloutContext,
    origin: SourceRef,
    original: OriginalData,
    kind: Option<&str>,
) -> Record {
    Record {
        id: RecordId::scoped(&info.source, "unknown", format!("{artifact}:{position}")),
        source: info.source.clone(),
        session: context.session.clone(),
        invocation: context.current_invocation.clone(),
        timestamp,
        origin,
        data: RecordData::Unknown(UnknownRecord {
            kind: kind.map(str::to_owned),
        }),
        original: Some(original),
    }
}

fn invocation_id(info: &ProviderInfo, artifact: &str, external_id: &str) -> RecordId {
    RecordId::scoped(
        &info.source,
        "agent-invocation",
        format!("{artifact}:{external_id}"),
    )
}

fn artifact_identity(source: &CodexSource, path: &Path) -> String {
    path.strip_prefix(source.codex_home())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn timestamp_from_fields(
    value: &Value,
    millisecond_key: &str,
    second_key: &str,
) -> Option<Timestamp> {
    value
        .get(millisecond_key)
        .and_then(Value::as_i64)
        .map(Timestamp::from_millis)
        .or_else(|| integer_timestamp(value.get(second_key)))
}

fn integer_timestamp(value: Option<&Value>) -> Option<Timestamp> {
    value.and_then(Value::as_i64).map(|value| {
        if value.unsigned_abs() >= 1_000_000_000_000 {
            Timestamp::from_millis(value)
        } else {
            Timestamp::from_seconds(value)
        }
    })
}

fn normalize_rate_limit(value: &Value) -> Option<RateLimit> {
    let fields = value.as_object()?;
    let windows = ["primary", "secondary"]
        .into_iter()
        .filter_map(|name| {
            let window = fields.get(name)?.as_object()?;
            Some(RateLimitWindow {
                name: name.to_owned(),
                used_percent: window.get("used_percent").and_then(Value::as_f64),
                window_minutes: window.get("window_minutes").and_then(Value::as_i64),
                resets_at: integer_timestamp(window.get("resets_at")),
            })
        })
        .collect::<Vec<_>>();
    let credits = fields
        .get("credits")
        .and_then(Value::as_object)
        .map(|credits| CreditBalance {
            has_credits: credits.get("has_credits").and_then(Value::as_bool),
            unlimited: credits.get("unlimited").and_then(Value::as_bool),
            balance: credits
                .get("balance")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        });
    let spend_limit = fields
        .get("individual_limit")
        .and_then(Value::as_object)
        .map(|limit| SpendLimit {
            limit: limit
                .get("limit")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            used: limit
                .get("used")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            remaining_percent: limit
                .get("remaining_percent")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok()),
            resets_at: integer_timestamp(limit.get("resets_at")),
        });
    let reached = fields
        .get("rate_limit_reached_type")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let (reached_reason, reached_scope) = match reached {
        Some("rate_limit_reached") => (Some(RateLimitReason::RateLimit), None),
        Some("workspace_owner_credits_depleted") => (
            Some(RateLimitReason::CreditsDepleted),
            Some(RateLimitScope::WorkspaceOwner),
        ),
        Some("workspace_member_credits_depleted") => (
            Some(RateLimitReason::CreditsDepleted),
            Some(RateLimitScope::WorkspaceMember),
        ),
        Some("workspace_owner_usage_limit_reached") => (
            Some(RateLimitReason::UsageLimit),
            Some(RateLimitScope::WorkspaceOwner),
        ),
        Some("workspace_member_usage_limit_reached") => (
            Some(RateLimitReason::UsageLimit),
            Some(RateLimitScope::WorkspaceMember),
        ),
        Some(value) => (Some(RateLimitReason::Other(value.to_owned())), None),
        None => (None, None),
    };
    let snapshot = RateLimit {
        external_id: optional_nonempty_string(value, "limit_id"),
        name: optional_nonempty_string(value, "limit_name"),
        windows,
        credits,
        spend_limit,
        spend_control_reached: fields.get("spend_control_reached").and_then(Value::as_bool),
        plan: optional_nonempty_string(value, "plan_type"),
        reached_reason,
        reached_scope,
    };
    (!fields.is_empty()).then_some(snapshot)
}

pub(super) fn timestamp_value(value: Option<&Value>) -> Option<Timestamp> {
    let value = value?;
    value
        .as_str()
        .and_then(|value| parse_rfc3339(value.trim()))
        .or_else(|| integer_timestamp(Some(value)))
}

fn normalize_tool_value(value: &Value) -> Value {
    value
        .as_str()
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_else(|| value.clone())
}

fn normalize_compaction(value: &Value) -> ContextCompaction {
    ContextCompaction {
        summary: compaction_summary(value),
        automatic: compaction_bool(value, "automatic"),
        tokens_before: compaction_i64(value, "tokens_before"),
        tokens_after: compaction_i64(value, "tokens_after"),
        replacement_history: value
            .get("replacement_history")
            .or_else(|| value.get("replacementHistory"))
            .and_then(Value::as_array)
            .cloned(),
        window_number: value
            .get("window_number")
            .or_else(|| value.get("windowNumber"))
            .and_then(Value::as_u64),
        first_window_id: compaction_string(value, "first_window_id"),
        previous_window_id: compaction_string(value, "previous_window_id"),
        window_id: compaction_string(value, "window_id"),
    }
}

fn compaction_summary(value: &Value) -> Option<String> {
    optional_nonempty_string(value, "message")
        .or_else(|| optional_nonempty_string(value, "summary"))
        .or_else(|| optional_nonempty_string(value, "text"))
}

fn compaction_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .or_else(|| value.get(camel_case(key)))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn compaction_bool(value: &Value, key: &str) -> Option<bool> {
    value
        .get(key)
        .or_else(|| value.get(camel_case(key)))
        .and_then(Value::as_bool)
}

fn compaction_i64(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .or_else(|| value.get(camel_case(key)))
        .and_then(Value::as_i64)
}

fn camel_case(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
        } else if uppercase {
            result.extend(character.to_uppercase());
            uppercase = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn normalize_tool_output_content(value: &Value) -> Vec<ContentBlock> {
    match value {
        Value::Null => Vec::new(),
        Value::String(text) => vec![ContentBlock::text(text)],
        Value::Array(values) => values
            .iter()
            .flat_map(normalize_tool_output_content)
            .collect(),
        Value::Object(fields) if fields.get("type").and_then(Value::as_str).is_some() => {
            normalize_content(value)
        }
        _ => Vec::new(),
    }
}

fn normalize_content(value: &Value) -> Vec<ContentBlock> {
    match value {
        Value::Null => Vec::new(),
        Value::String(text) => vec![ContentBlock::text(text)],
        Value::Array(values) => values.iter().flat_map(normalize_content).collect(),
        Value::Object(fields) => match fields.get("type").and_then(Value::as_str) {
            Some(
                "text" | "Text" | "input_text" | "inputText" | "output_text" | "outputText"
                | "summary_text" | "summaryText" | "reasoning_text" | "reasoningText",
            ) => fields
                .get("text")
                .and_then(Value::as_str)
                .map(|text| ContentBlock::Text {
                    text: text.to_owned(),
                    annotations: content_annotations(value),
                })
                .into_iter()
                .collect(),
            Some(
                "image" | "Image" | "input_image" | "inputImage" | "local_image" | "localImage",
            ) => vec![ContentBlock::Image {
                mime_type: fields
                    .get("mime_type")
                    .or_else(|| fields.get("mimeType"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                uri: fields
                    .get("image_url")
                    .or_else(|| fields.get("imageUrl"))
                    .or_else(|| fields.get("url"))
                    .or_else(|| fields.get("uri"))
                    .or_else(|| fields.get("path"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                data: fields
                    .get("data")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                annotations: content_annotations(value),
            }],
            Some(
                "audio" | "Audio" | "input_audio" | "inputAudio" | "local_audio" | "localAudio",
            ) => vec![ContentBlock::Audio {
                mime_type: fields
                    .get("mime_type")
                    .or_else(|| fields.get("mimeType"))
                    .or_else(|| fields.get("format"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                uri: fields
                    .get("audio_url")
                    .or_else(|| fields.get("audioUrl"))
                    .or_else(|| fields.get("url"))
                    .or_else(|| fields.get("uri"))
                    .or_else(|| fields.get("path"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                data: fields
                    .get("data")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                annotations: content_annotations(value),
            }],
            Some("skill" | "mention") => vec![ContentBlock::ResourceLink {
                uri: fields
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                name: fields
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                title: None,
                description: None,
                mime_type: None,
                size: None,
                icons: Vec::new(),
                annotations: content_annotations(value),
            }],
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
        .filter_map(|content| match content {
            ContentBlock::Text { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn terminal_tool_status(payload: &Value, output: &Value) -> ToolStatus {
    if let Some(status) = string(payload, "status") {
        return tool_status(status);
    }
    if payload.get("is_error").and_then(Value::as_bool) == Some(true)
        || output.get("is_error").and_then(Value::as_bool) == Some(true)
        || payload.get("success").and_then(Value::as_bool) == Some(false)
    {
        ToolStatus::Failed
    } else {
        ToolStatus::Completed
    }
}

fn tool_output_error(payload: &Value, status: ToolStatus) -> Option<String> {
    if !matches!(
        status,
        ToolStatus::Failed | ToolStatus::Cancelled | ToolStatus::Declined
    ) {
        return None;
    }
    payload
        .get("error")
        .and_then(|error| {
            error.as_str().map(str::to_owned).or_else(|| {
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
        })
        .or_else(|| {
            payload
                .get("output")
                .and_then(Value::as_str)
                .filter(|output| !output.is_empty())
                .map(str::to_owned)
        })
}

fn remember_terminal_tool(context: &mut RolloutContext, call_id: &str, status: ToolStatus) {
    if matches!(
        status,
        ToolStatus::Completed | ToolStatus::Failed | ToolStatus::Cancelled | ToolStatus::Declined
    ) {
        context.terminal_tool_results.insert(call_id.to_owned());
    }
}

fn duration_millis(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(milliseconds) = value.as_i64() {
        return Some(milliseconds);
    }
    let seconds = value.get("secs").and_then(Value::as_u64);
    let nanoseconds = value.get("nanos").and_then(Value::as_u64);
    if seconds.is_none() && nanoseconds.is_none() {
        return None;
    }
    let milliseconds = u128::from(seconds.unwrap_or_default())
        .saturating_mul(1_000)
        .saturating_add(u128::from(nanoseconds.unwrap_or_default()) / 1_000_000);
    Some(i64::try_from(milliseconds).unwrap_or(i64::MAX))
}

fn normalize_reasoning_summary(value: &Value) -> Vec<String> {
    match value {
        Value::String(value) => vec![value.clone()],
        Value::Array(values) => values
            .iter()
            .filter_map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.get("text").and_then(Value::as_str).map(str::to_owned))
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn provider_value_label(value: &Value) -> Option<&str> {
    value.as_str().or_else(|| {
        let object = value.as_object()?;
        ["type", "mode", "name", "id", "kind"]
            .into_iter()
            .find_map(|key| object.get(key).and_then(Value::as_str))
            .or_else(|| {
                (object.len() == 1)
                    .then(|| object.keys().next().map(String::as_str))
                    .flatten()
            })
    })
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn optional_nonempty_string(value: &Value, key: &str) -> Option<String> {
    string(value, key)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn display_title(value: &Value, key: &str) -> Option<String> {
    let title = string(value, key)?.trim();
    if title.is_empty() {
        return None;
    }
    const MAX_CHARS: usize = 160;
    let mut chars = title.chars();
    let shortened: String = chars.by_ref().take(MAX_CHARS).collect();
    Some(if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    })
}

fn bool_value(value: Option<&Value>) -> bool {
    value.is_some_and(|value| match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_i64().is_some_and(|value| value != 0),
        _ => false,
    })
}

fn error_message(value: &Value) -> String {
    value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
