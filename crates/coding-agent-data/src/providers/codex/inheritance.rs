use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::providers::shared::jsonl::{read_bounded_line, LineRead};
use crate::{Event, EventData, ProviderInfo, Record, RecordData, RecordId};

use super::checkpoint::RolloutContext;
use super::normalize::{self, Position};
use super::replay::{open_reader, read_rollout_metadata};
use super::state_db::IndexSnapshot;
use super::CodexSource;

pub(super) struct InheritancePlan {
    parent_by_child: BTreeMap<PathBuf, PathBuf>,
    items_by_parent: BTreeMap<PathBuf, BTreeMap<EventKey, RecordId>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EventKey {
    kind: &'static str,
    external_id: String,
}

impl InheritancePlan {
    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            parent_by_child: BTreeMap::new(),
            items_by_parent: BTreeMap::new(),
        }
    }

    pub(super) fn build(
        source: &CodexSource,
        info: &ProviderInfo,
        files: &[PathBuf],
        index: &IndexSnapshot,
        max_line_bytes: usize,
    ) -> Self {
        let mut metadata_by_path = BTreeMap::new();
        let mut paths_by_session = BTreeMap::new();
        for (path, binding) in &index.transcripts {
            paths_by_session.insert(binding.external_id.clone(), path.clone());
        }
        for path in files {
            let expected_owner = index
                .transcripts
                .get(path)
                .map(|binding| binding.external_id.as_str());
            if let Some(metadata) = read_rollout_metadata(path, max_line_bytes, expected_owner) {
                paths_by_session
                    .entry(metadata.session_id.clone())
                    .or_insert_with(|| path.clone());
                metadata_by_path.insert(path.clone(), metadata);
            }
        }

        let mut parent_by_child = BTreeMap::new();
        for (child_path, metadata) in &metadata_by_path {
            let Some(parent_id) = metadata.parent_id.as_deref() else {
                continue;
            };
            if parent_id == metadata.session_id {
                continue;
            }
            if let Some(parent_path) = paths_by_session
                .get(parent_id)
                .filter(|parent_path| *parent_path != child_path)
            {
                parent_by_child.insert(child_path.clone(), parent_path.clone());
            }
        }

        let mut items_by_parent = BTreeMap::new();
        for parent_path in parent_by_child.values() {
            if items_by_parent.contains_key(parent_path) {
                continue;
            }
            let expected_owner = index
                .transcripts
                .get(parent_path)
                .map(|binding| binding.external_id.as_str());
            let Some(metadata) = read_rollout_metadata(parent_path, max_line_bytes, expected_owner)
            else {
                continue;
            };
            if let Ok(items) = read_event_index(
                source,
                info,
                parent_path,
                &metadata.session_id,
                max_line_bytes,
            ) {
                items_by_parent.insert(parent_path.clone(), items);
            }
        }

        Self {
            parent_by_child,
            items_by_parent,
        }
    }

    pub(super) fn apply(&self, child_path: &Path, records: &mut [Record]) {
        let Some(parent_path) = self.parent_by_child.get(child_path) else {
            return;
        };
        let Some(parent_events) = self.items_by_parent.get(parent_path) else {
            return;
        };
        for record in records {
            let RecordData::Event(item) = &mut record.data else {
                continue;
            };
            if item.inherited_from.is_some() {
                continue;
            }
            if let Some(parent) = event_key(item).and_then(|key| parent_events.get(&key)) {
                item.inherited_from = Some(parent.clone());
            }
        }
    }
}

fn read_event_index(
    source: &CodexSource,
    info: &ProviderInfo,
    path: &Path,
    owner: &str,
    max_line_bytes: usize,
) -> std::io::Result<BTreeMap<EventKey, RecordId>> {
    let compressed = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".jsonl.zst"));
    let mut reader = open_reader(path)?;
    let mut bytes = Vec::new();
    let mut line = 0_u64;
    let mut offset = 0_u64;
    let mut context = RolloutContext {
        session_external_id: Some(owner.to_owned()),
        session: Some(normalize::session_id(info, owner)),
        ..RolloutContext::default()
    };
    let indexed_sessions = BTreeMap::new();
    let mut items = BTreeMap::new();

    loop {
        let read = read_bounded_line(&mut reader, &mut bytes, max_line_bytes)?;
        let (count, too_large) = match read {
            LineRead::Eof | LineRead::Partial { .. } => break,
            LineRead::Complete { count, too_large } => (count, too_large),
        };
        let byte_start = offset;
        offset = offset.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        line = line.saturating_add(1);
        if too_large {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let mut records = normalize::rollout_records(
            normalize::RolloutInput {
                source,
                info,
                path,
                value: &value,
                position: Position {
                    line,
                    byte_start: (!compressed).then_some(byte_start),
                    byte_end: (!compressed).then_some(offset),
                    logical_ordinal: value.get("ordinal").and_then(Value::as_u64),
                },
                indexed_sessions: &indexed_sessions,
                lineage: None,
            },
            &mut context,
        );
        for record in records.drain(..) {
            if let RecordData::Event(item) = &record.data {
                if let Some(key) = event_key(item) {
                    items.insert(key, record.id);
                }
            }
        }
    }
    Ok(items)
}

fn event_key(item: &Event) -> Option<EventKey> {
    let external_id = item.external_id.as_ref()?.clone();
    let kind = match &item.data {
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
        EventData::ForkInvocationBoundary(_) => "fork_turn_boundary",
        EventData::InputQueue(_) => "input_queue",
        EventData::ContextCompaction(_) => "context_compaction",
        EventData::ExecutionContext(_) => "execution_context",
        EventData::ModeChange(_) => "mode_change",
        EventData::Notice(_) => "notice",
        EventData::HookResult(_) => "hook_result",
        EventData::Retry(_) => "retry",
        EventData::Rollback(_) => "rollback",
        EventData::Unknown(_) => "unknown",
    };
    Some(EventKey { kind, external_id })
}
