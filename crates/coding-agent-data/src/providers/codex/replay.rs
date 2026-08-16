use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::shared::jsonl::{read_bounded_line, LineRead};
use crate::{Diagnostic, ProviderInfo, SourceRef, Timestamp, TokenUsage};

use super::checkpoint::{CodexCheckpoint, UsageReplayState};
use super::normalize;
use super::state_db::IndexSnapshot;
use super::token_usage::{self, UsageAccountingState};

/// Longest pause tolerated inside a burst whose timestamps Codex rewrote to
/// the fork instant.
const REWRITTEN_BURST_PAUSE_MS: i64 = 1_000;

#[derive(Debug)]
pub(super) struct ReplayPlan {
    entries: BTreeMap<PathBuf, ReplayEntry>,
}

#[derive(Debug)]
struct ReplayEntry {
    parent_prefix: Vec<TokenUsage>,
    structural_prefix_len: Option<usize>,
    rewritten_burst_start: Option<Timestamp>,
}

#[derive(Clone, Copy)]
pub(super) struct HistoryBaseMetadata<'a> {
    pub thread_id: &'a str,
    pub end_ordinal_exclusive: u64,
    pub end_byte_offset: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RolloutMetadata {
    pub session_id: String,
    pub parent_id: Option<String>,
    pub timestamp: Option<Timestamp>,
    pub history_mode: Option<String>,
    pub history_base: Option<OwnedHistoryBaseMetadata>,
    pub own_start_ordinal: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct OwnedHistoryBaseMetadata {
    pub thread_id: String,
    pub end_ordinal_exclusive: u64,
    pub end_byte_offset: u64,
}

struct UsageEvent {
    timestamp: Option<Timestamp>,
    usage: TokenUsage,
}

#[derive(Default)]
struct ChildUsageShape {
    structural_prefix_len: Option<usize>,
    rewritten_burst_start: Option<Timestamp>,
}

impl ReplayPlan {
    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub(super) fn build(
        files: &[PathBuf],
        index: &IndexSnapshot,
        state: &CodexCheckpoint,
        metadata_cache: &BTreeMap<PathBuf, Option<RolloutMetadata>>,
        info: &ProviderInfo,
        max_line_bytes: usize,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let mut paths_by_session = BTreeMap::new();
        for (path, binding) in &index.transcripts {
            paths_by_session
                .entry(binding.external_id.clone())
                .or_insert_with(|| path.clone());
        }
        for (path, rollout) in &state.rollouts {
            if let Some(session_id) = rollout.context().session_external_id.as_ref() {
                paths_by_session
                    .entry(session_id.clone())
                    .or_insert_with(|| path.clone());
            }
        }

        let mut metadata_by_path = BTreeMap::new();
        for path in files {
            if let Some(Some(metadata)) = metadata_cache.get(path) {
                paths_by_session
                    .entry(metadata.session_id.clone())
                    .or_insert_with(|| path.clone());
                metadata_by_path.insert(path.clone(), metadata);
            }
        }

        let mut parent_usage = BTreeMap::<PathBuf, Vec<UsageEvent>>::new();
        let mut entries = BTreeMap::new();
        for (child_path, metadata) in metadata_by_path {
            let Some(parent_id) = metadata.parent_id.as_deref() else {
                continue;
            };
            if parent_id == metadata.session_id {
                continue;
            }
            // Paginated rollouts carry explicit ordinals. `rollout.rs`
            // already rejects every record before this boundary, so applying
            // a second usage-only replay heuristic would drop owned usage.
            if metadata.own_start_ordinal.is_some() {
                continue;
            }

            let child_shape =
                inspect_child_usage_shape(&child_path, max_line_bytes, metadata.timestamp)
                    .unwrap_or_default();

            let parent_path = paths_by_session
                .get(parent_id)
                .filter(|parent_path| *parent_path != &child_path)
                .filter(|parent_path| parent_path.is_file())
                .cloned();
            let parent_events = parent_path.as_ref().map(|parent_path| {
                parent_usage.entry(parent_path.clone()).or_insert_with(|| {
                    read_usage_events(parent_path, max_line_bytes).unwrap_or_default()
                })
            });
            if parent_path.is_none() {
                super::rollout::push_diagnostic(
                    diagnostics,
                    Diagnostic::warning(
                        "codex.usage_replay.parent_missing",
                        "a fork parent rollout is unavailable; rewritten usage timestamps will be used as a fallback",
                    )
                    .with_origin(SourceRef::whole_file(
                        info.source.clone(),
                        child_path.clone(),
                    )),
                );
            }

            let parent_prefix = parent_events
                .map(|events| {
                    let replay_len = metadata.timestamp.map_or(events.len(), |forked_at| {
                        events
                            .iter()
                            .position(|event| {
                                event
                                    .timestamp
                                    .is_some_and(|timestamp| timestamp > forked_at)
                            })
                            .unwrap_or(events.len())
                    });
                    events[..replay_len]
                        .iter()
                        .map(|event| event.usage.clone())
                        .collect()
                })
                .unwrap_or_default();
            entries.insert(
                child_path.clone(),
                ReplayEntry {
                    parent_prefix,
                    structural_prefix_len: child_shape.structural_prefix_len,
                    rewritten_burst_start: child_shape.rewritten_burst_start,
                },
            );
        }
        Self { entries }
    }

    pub(super) fn initialize(&self, path: &Path, state: &mut UsageReplayState) {
        if let Some(structural_prefix_len) = self
            .entries
            .get(path)
            .and_then(|entry| entry.structural_prefix_len)
        {
            let already_skipped = match state {
                UsageReplayState::Uninitialized => 0,
                UsageReplayState::MatchingParent { index } => *index,
                UsageReplayState::SkippingRewrittenBurst { skipped, .. } => *skipped,
                UsageReplayState::SkippingInherited { .. } | UsageReplayState::Done => return,
            };
            *state = match structural_prefix_len.checked_sub(already_skipped) {
                Some(remaining) if remaining > 0 => {
                    UsageReplayState::SkippingInherited { remaining }
                }
                _ => UsageReplayState::Done,
            };
        } else if matches!(state, UsageReplayState::Uninitialized) {
            *state = match self.entries.get(path) {
                None => UsageReplayState::Done,
                Some(_) => UsageReplayState::MatchingParent { index: 0 },
            };
        }
    }

    pub(super) fn filter_delta(
        &self,
        path: &Path,
        state: &mut UsageReplayState,
        timestamp: Option<Timestamp>,
        delta: Option<TokenUsage>,
    ) -> Option<TokenUsage> {
        let delta = delta?;
        let mut current = std::mem::replace(state, UsageReplayState::Done);

        loop {
            match current {
                UsageReplayState::Uninitialized => {
                    current = match self.entries.get(path) {
                        Some(ReplayEntry {
                            structural_prefix_len: Some(remaining),
                            ..
                        }) if *remaining > 0 => UsageReplayState::SkippingInherited {
                            remaining: *remaining,
                        },
                        Some(ReplayEntry {
                            structural_prefix_len: Some(0),
                            ..
                        })
                        | None => UsageReplayState::Done,
                        Some(_) => UsageReplayState::MatchingParent { index: 0 },
                    };
                }
                UsageReplayState::MatchingParent { index } => {
                    let Some(entry) = self.entries.get(path) else {
                        *state = UsageReplayState::Done;
                        return Some(delta);
                    };
                    if entry.parent_prefix.get(index) == Some(&delta) {
                        *state = if index + 1 == entry.parent_prefix.len() {
                            UsageReplayState::Done
                        } else {
                            UsageReplayState::MatchingParent { index: index + 1 }
                        };
                        return None;
                    }
                    current = if index == 0 {
                        entry
                            .rewritten_burst_start
                            .map(|previous| UsageReplayState::SkippingRewrittenBurst {
                                previous,
                                skipped: 0,
                            })
                            .unwrap_or(UsageReplayState::Done)
                    } else {
                        UsageReplayState::Done
                    };
                }
                UsageReplayState::SkippingInherited { remaining } => {
                    if remaining > 1 {
                        *state = UsageReplayState::SkippingInherited {
                            remaining: remaining - 1,
                        };
                    } else {
                        *state = UsageReplayState::Done;
                    }
                    return None;
                }
                UsageReplayState::SkippingRewrittenBurst { previous, skipped } => {
                    if let Some(timestamp) = timestamp.filter(|timestamp| {
                        (0..=REWRITTEN_BURST_PAUSE_MS)
                            .contains(&timestamp.as_millis().saturating_sub(previous.as_millis()))
                    }) {
                        *state = UsageReplayState::SkippingRewrittenBurst {
                            previous: timestamp,
                            skipped: skipped.saturating_add(1),
                        };
                        return None;
                    }
                    *state = UsageReplayState::Done;
                    return Some(delta);
                }
                UsageReplayState::Done => {
                    *state = UsageReplayState::Done;
                    return Some(delta);
                }
            }
        }
    }
}

pub(super) fn read_rollout_metadata(
    path: &Path,
    max_line_bytes: usize,
    expected_owner: Option<&str>,
) -> Option<RolloutMetadata> {
    let mut metadata = Vec::new();
    visit_json_lines(path, max_line_bytes, |value| {
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            return true;
        }
        let Some(payload) = value.get("payload") else {
            return true;
        };
        let Some(session_id) = payload
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            return true;
        };
        let selected_by_index = expected_owner == Some(session_id);
        let selected_by_filename =
            expected_owner.is_none() && rollout_filename_matches_session(path, session_id);
        let parent_id = payload
            .get("forked_from_id")
            .and_then(Value::as_str)
            .or_else(|| payload.get("parent_thread_id").and_then(Value::as_str))
            .or_else(|| {
                payload
                    .pointer("/source/subagent/thread_spawn/parent_thread_id")
                    .and_then(Value::as_str)
            })
            .or_else(|| {
                payload
                    .pointer("/history_base/thread_id")
                    .and_then(Value::as_str)
            })
            .filter(|parent_id| !parent_id.is_empty())
            .map(str::to_owned);
        let history_base = history_base(payload).map(|base| OwnedHistoryBaseMetadata {
            thread_id: base.thread_id.to_owned(),
            end_ordinal_exclusive: base.end_ordinal_exclusive,
            end_byte_offset: base.end_byte_offset,
        });
        metadata.push(RolloutMetadata {
            session_id: session_id.to_owned(),
            parent_id,
            timestamp: normalize::timestamp_value(value.get("timestamp")),
            history_mode: payload
                .get("history_mode")
                .and_then(Value::as_str)
                .map(str::to_owned),
            history_base,
            own_start_ordinal: payload
                .get("subagent_history_start_ordinal")
                .and_then(Value::as_u64),
        });
        // The SQLite binding has the highest priority. Without one, an
        // owner-bearing filename is definitive. Once either matches there is
        // no reason to deserialize the rest of a potentially very large
        // rollout merely to inspect copied legacy metadata.
        !(selected_by_index || selected_by_filename)
    })
    .ok()?;
    if metadata.is_empty() {
        return None;
    }

    let selected = expected_owner
        .and_then(|owner| {
            metadata
                .iter()
                .position(|candidate| candidate.session_id == owner)
        })
        .or_else(|| {
            metadata
                .iter()
                .position(|candidate| rollout_filename_matches_session(path, &candidate.session_id))
        })
        .or_else(|| {
            metadata.iter().position(|candidate| {
                candidate
                    .parent_id
                    .as_deref()
                    .is_some_and(|parent| parent != candidate.session_id)
            })
        })
        // Current paginated rollouts write their owning session metadata at
        // the head. Legacy forks may append copied parent metadata later, so
        // "last session_meta wins" is not a safe compatibility fallback.
        .unwrap_or(0);
    Some(metadata.swap_remove(selected))
}

fn history_base(payload: &Value) -> Option<HistoryBaseMetadata<'_>> {
    let base = payload.get("history_base")?;
    Some(HistoryBaseMetadata {
        thread_id: base
            .get("thread_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())?,
        end_ordinal_exclusive: base.get("end_ordinal_exclusive").and_then(Value::as_u64)?,
        end_byte_offset: base.get("end_byte_offset").and_then(Value::as_u64)?,
    })
}

fn rollout_filename_matches_session(path: &Path, session_id: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name
        .strip_suffix(".zst")
        .unwrap_or(name)
        .strip_suffix(".jsonl")
        .unwrap_or(name);
    name == session_id || name.ends_with(&format!("-{session_id}"))
}

pub(super) fn read_rollout_owner(path: &Path, max_line_bytes: usize) -> Option<String> {
    read_rollout_metadata(path, max_line_bytes, None).map(|metadata| metadata.session_id)
}

fn read_usage_events(path: &Path, max_line_bytes: usize) -> std::io::Result<Vec<UsageEvent>> {
    let mut events = Vec::new();
    let mut accounting = UsageAccountingState::default();
    visit_json_lines(path, max_line_bytes, |value| {
        let payload = value.get("payload").unwrap_or(&Value::Null);
        if value.get("type").and_then(Value::as_str) != Some("event_msg")
            || payload.get("type").and_then(Value::as_str) != Some("token_count")
        {
            return true;
        }
        if let Some(usage) = token_usage::usage_report(payload, &mut accounting) {
            if let Some(delta) = usage.delta {
                events.push(UsageEvent {
                    timestamp: normalize::timestamp_value(value.get("timestamp")),
                    usage: delta,
                });
            }
        }
        true
    })?;
    Ok(events)
}

fn inspect_child_usage_shape(
    path: &Path,
    max_line_bytes: usize,
    forked_at: Option<Timestamp>,
) -> std::io::Result<ChildUsageShape> {
    let mut timestamps = Vec::with_capacity(2);
    let mut accounting = UsageAccountingState::default();
    let mut record_index = 0_usize;
    let mut usage_count = 0_usize;
    let mut last_task_started = None;
    let mut last_invocation_context = None;
    let mut saw_trigger_marker = false;
    let mut structural_prefix_len = None;
    visit_json_lines(path, max_line_bytes, |value| {
        let current_index = record_index;
        record_index = record_index.saturating_add(1);
        let payload = value.get("payload").unwrap_or(&Value::Null);
        let record_type = value.get("type").and_then(Value::as_str);
        if record_type == Some("event_msg")
            && payload.get("type").and_then(Value::as_str) == Some("token_count")
        {
            if token_usage::usage_report(payload, &mut accounting)
                .and_then(|usage| usage.delta)
                .is_some()
            {
                usage_count = usage_count.saturating_add(1);
                if timestamps.len() < 2 {
                    timestamps.push(normalize::timestamp_value(value.get("timestamp")));
                }
            }
        } else if record_type == Some("event_msg")
            && payload.get("type").and_then(Value::as_str) == Some("task_started")
        {
            last_task_started = Some((current_index, usage_count));
        } else if record_type == Some("turn_context") {
            last_invocation_context = Some((current_index, usage_count));
        }

        let is_trigger_marker = matches!(
            record_type,
            Some("inter_agent_communication_metadata" | "inter_agent_communication")
        ) && payload.get("trigger_turn").and_then(Value::as_bool)
            == Some(true);
        if is_trigger_marker && !saw_trigger_marker {
            saw_trigger_marker = true;
            structural_prefix_len = last_task_started.map(|(_, count)| count).or_else(|| {
                last_invocation_context
                    .filter(|(index, _)| index.saturating_add(1) == current_index)
                    .map(|(_, count)| count)
            });
        }
        true
    })?;
    let rewritten_burst_start = match (forked_at, timestamps.as_slice()) {
        (Some(forked_at), [Some(first), Some(second), ..])
            if first.as_millis().abs_diff(forked_at.as_millis())
                <= REWRITTEN_BURST_PAUSE_MS as u64
                && (0..=REWRITTEN_BURST_PAUSE_MS)
                    .contains(&second.as_millis().saturating_sub(first.as_millis())) =>
        {
            Some(*first)
        }
        _ => None,
    };
    Ok(ChildUsageShape {
        structural_prefix_len,
        rewritten_burst_start,
    })
}

fn visit_json_lines(
    path: &Path,
    max_line_bytes: usize,
    mut visit: impl FnMut(Value) -> bool,
) -> std::io::Result<()> {
    let mut reader = open_reader(path)?;
    let mut bytes = Vec::new();
    loop {
        let line = read_bounded_line(&mut reader, &mut bytes, max_line_bytes)?;
        let final_line = matches!(line, LineRead::Partial { .. });
        match line {
            LineRead::Eof => break,
            LineRead::Complete {
                too_large: true, ..
            }
            | LineRead::Partial { too_large: true } => {}
            LineRead::Complete {
                too_large: false, ..
            }
            | LineRead::Partial { too_large: false } => {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    if !visit(value) {
                        break;
                    }
                }
            }
        }
        if final_line {
            break;
        }
    }
    Ok(())
}

pub(super) fn open_reader(path: &Path) -> std::io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".jsonl.zst"))
    {
        let decoder = zstd::stream::read::Decoder::new(file)?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Read};

    use tempfile::tempdir;

    use super::{open_reader, read_rollout_metadata};

    #[test]
    fn metadata_owner_match_does_not_read_a_corrupt_compressed_tail() {
        let directory = tempdir().unwrap();
        let path = directory
            .path()
            .join("rollout-2026-01-02T03-04-05-thread-1.jsonl.zst");
        let line = concat!(
            "{\"timestamp\":\"2026-01-02T03:04:05Z\",",
            "\"type\":\"session_meta\",",
            "\"payload\":{\"id\":\"thread-1\"}}\n"
        );
        let mut encoded = zstd::stream::encode_all(Cursor::new(line), 0).unwrap();
        // A truncated second frame proves that a reader which continues past
        // the definitive metadata line observes an error.
        encoded.extend_from_slice(&[0x28, 0xb5, 0x2f, 0xfd]);
        fs::write(&path, encoded).unwrap();

        let mut reader = open_reader(&path).unwrap();
        let mut decoded = Vec::new();
        assert!(reader.read_to_end(&mut decoded).is_err());

        let metadata = read_rollout_metadata(&path, 1024, None).unwrap();
        assert_eq!(metadata.session_id, "thread-1");
    }
}
