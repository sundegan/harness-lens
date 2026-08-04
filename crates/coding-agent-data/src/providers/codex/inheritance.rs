use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::providers::shared::jsonl::{read_bounded_line, LineRead};
use crate::{Item, ItemData, ProviderInfo, Record, RecordData, RecordId};

use super::checkpoint::RolloutContext;
use super::normalize::{self, Position};
use super::replay::{open_reader, read_rollout_metadata};
use super::state_db::IndexSnapshot;
use super::CodexSource;

pub(super) struct InheritancePlan {
    parent_by_child: BTreeMap<PathBuf, PathBuf>,
    items_by_parent: BTreeMap<PathBuf, BTreeMap<ItemKey, RecordId>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ItemKey {
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
            if let Ok(items) = read_item_index(
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
        let Some(parent_items) = self.items_by_parent.get(parent_path) else {
            return;
        };
        for record in records {
            let RecordData::Item(item) = &mut record.data else {
                continue;
            };
            if item.inherited_from.is_some() {
                continue;
            }
            if let Some(parent) = item_key(item).and_then(|key| parent_items.get(&key)) {
                item.inherited_from = Some(parent.clone());
            }
        }
    }
}

fn read_item_index(
    source: &CodexSource,
    info: &ProviderInfo,
    path: &Path,
    owner: &str,
    max_line_bytes: usize,
) -> std::io::Result<BTreeMap<ItemKey, RecordId>> {
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
            if let RecordData::Item(item) = &record.data {
                if let Some(key) = item_key(item) {
                    items.insert(key, record.id);
                }
            }
        }
    }
    Ok(items)
}

fn item_key(item: &Item) -> Option<ItemKey> {
    let external_id = item.external_id.as_ref()?.clone();
    let kind = match &item.data {
        ItemData::Message(_) => "message",
        ItemData::Reasoning(_) => "reasoning",
        ItemData::Plan(_) => "plan",
        ItemData::ToolCall(_) => "tool_call",
        ItemData::ToolResult(_) => "tool_result",
        ItemData::ApprovalRequest(_) => "approval_request",
        ItemData::ApprovalDecision(_) => "approval_decision",
        ItemData::ModelInvocation(_) => "model_invocation",
        ItemData::AgentInvocation(_) => "agent_invocation",
        ItemData::FileChange(_) => "file_change",
        ItemData::WorldState(_) => "world_state",
        ItemData::Goal(_) => "goal",
        ItemData::ForkTurnBoundary(_) => "fork_turn_boundary",
        ItemData::InputQueue(_) => "input_queue",
        ItemData::ContextCompaction(_) => "context_compaction",
        ItemData::ExecutionContext(_) => "execution_context",
        ItemData::ModeChange(_) => "mode_change",
        ItemData::Notice(_) => "notice",
        ItemData::HookResult(_) => "hook_result",
        ItemData::Retry(_) => "retry",
        ItemData::Rollback(_) => "rollback",
        ItemData::Unknown(_) => "unknown",
    };
    Some(ItemKey { kind, external_id })
}
