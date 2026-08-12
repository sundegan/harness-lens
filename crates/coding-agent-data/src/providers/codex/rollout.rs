use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::providers::shared::jsonl::{
    is_complete_json_value, read_bounded_line, tail_fingerprint, LineRead,
};
use crate::{Change, Diagnostic, Error, ProviderInfo, RecordId, Result, Session, SourceRef};

use super::checkpoint::{
    CodexCheckpoint, FileSignature, RolloutContext, RolloutFileMetadata, RolloutState,
};
use super::inheritance::InheritancePlan;
use super::lineage::LineagePlan;
use super::normalize::{self, Position};
use super::replay::{read_rollout_owner, ReplayPlan};
use super::state_db::IndexSnapshot;
use super::usage_attribution::{UsageAttributionPlan, UsageFingerprint};
use super::CodexSource;

const MAX_DIAGNOSTICS_PER_BATCH: usize = 1_000;

#[derive(Clone, Debug)]
pub(super) struct ScanLimits {
    max_lines_per_batch: usize,
    max_line_bytes: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_lines_per_batch: 100_000,
            max_line_bytes: 16 * 1024 * 1024,
        }
    }
}

struct ScanEnvironment<'a> {
    source: &'a CodexSource,
    info: &'a ProviderInfo,
    replay: &'a ReplayPlan,
    lineage: &'a LineagePlan,
    inheritance: &'a InheritancePlan,
    indexed_sessions: &'a BTreeMap<String, Session>,
    limits: &'a ScanLimits,
}

struct ScanBatch<'a> {
    attribution: &'a mut UsageAttributionPlan,
    pending_usage_rebuilds: &'a mut BTreeMap<PathBuf, BTreeSet<UsageFingerprint>>,
    remaining: &'a mut usize,
    changes: &'a mut Vec<Change>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

pub(super) fn scan(
    source: &CodexSource,
    info: &ProviderInfo,
    limits: &ScanLimits,
    index: &IndexSnapshot,
    state: &mut CodexCheckpoint,
    changes: &mut Vec<Change>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool> {
    let catalog = rollout_catalog(source)?;
    let files = catalog.keys().cloned().collect::<Vec<_>>();
    let current_files = files.iter().cloned().collect::<BTreeSet<_>>();
    let tracked_files_match = state.rollouts.keys().eq(catalog.keys());
    if state.usage_attribution_ready
        && state.pending_usage_rebuilds.is_empty()
        && !index.database_changed
        && tracked_files_match
        && state.rollout_catalog.as_ref() == Some(&catalog)
    {
        return Ok(false);
    }

    let index_refresh_pending =
        remove_missing_rollouts(state, &current_files, index, info, changes);
    if !state.usage_attribution_ready {
        let tracked = state.rollouts.keys().cloned().collect::<Vec<_>>();
        begin_usage_rebuilds(state, info, tracked, changes);
        state.usage_attribution_ready = true;
    }
    let mut attribution = UsageAttributionPlan::build(state);
    let inconsistent_attribution = attribution.take_rebuilds();
    begin_usage_rebuilds(state, info, inconsistent_attribution, changes);

    let replay = ReplayPlan::build(
        &files,
        index,
        state,
        info,
        limits.max_line_bytes,
        diagnostics,
    );
    let lineage = LineagePlan::build(&files, index, info, limits.max_line_bytes, diagnostics);
    let inheritance = InheritancePlan::build(source, info, &files, index, limits.max_line_bytes);
    let mut remaining = limits.max_lines_per_batch.saturating_sub(changes.len());
    let mut has_more = index_refresh_pending;
    let environment = ScanEnvironment {
        source,
        info,
        replay: &replay,
        lineage: &lineage,
        inheritance: &inheritance,
        indexed_sessions: &index.sessions,
        limits,
    };

    for path in files {
        if remaining == 0 {
            has_more = true;
            break;
        }
        if state
            .rollouts
            .get(&path)
            .as_ref()
            .is_some_and(|rollout| lineage_changed(rollout, &lineage, &path))
        {
            begin_usage_rebuilds(state, info, [path.clone()], changes);
            push_diagnostic(
                diagnostics,
                Diagnostic::warning(
                    "codex.lineage.changed",
                    "the resolved history lineage changed; the rollout was reparsed",
                )
                .with_origin(SourceRef::whole_file(info.source.clone(), &path)),
            );
        }
        let previous = state.rollouts.get(&path).cloned();
        let binding = index.transcripts.get(&path);
        let (next, file_has_more) = {
            let mut batch = ScanBatch {
                attribution: &mut attribution,
                pending_usage_rebuilds: &mut state.pending_usage_rebuilds,
                remaining: &mut remaining,
                changes,
                diagnostics,
            };
            if is_compressed(&path) {
                scan_compressed(&environment, &mut batch, &path, previous, binding)
            } else {
                scan_plain(&environment, &mut batch, &path, previous, binding)
            }
        }?;
        state.rollouts.insert(path.clone(), next);
        if !file_has_more {
            state.pending_usage_rebuilds.remove(&path);
        }
        has_more |= file_has_more;
    }

    let ownership_changes = attribution.take_rebuilds();
    if begin_usage_rebuilds(state, info, ownership_changes, changes) {
        has_more = true;
    }
    state.rollout_catalog =
        (!has_more && state.pending_usage_rebuilds.is_empty()).then_some(catalog);
    Ok(has_more)
}

fn remove_missing_rollouts(
    state: &mut CodexCheckpoint,
    current_files: &BTreeSet<PathBuf>,
    index: &IndexSnapshot,
    info: &ProviderInfo,
    changes: &mut Vec<Change>,
) -> bool {
    let mut index_refresh_pending = false;
    let removed = state
        .rollouts
        .keys()
        .chain(state.pending_usage_rebuilds.keys())
        .filter(|path| !current_files.contains(*path))
        .cloned()
        .collect::<BTreeSet<_>>();
    for path in removed {
        if let Some(rollout) = state.rollouts.remove(&path) {
            let context = rollout.context();
            if let Some(external_id) = context.session_external_id.as_ref() {
                if index.sessions.contains_key(external_id) {
                    // The rollout projection may currently own the shared
                    // session record ID. Force the unchanged SQLite row to be
                    // emitted again after removing this artifact.
                    state.threads.remove(external_id);
                    state.database = None;
                    index_refresh_pending = true;
                } else if let Some(session) = context.session.clone() {
                    changes.push(Change::Delete(session));
                }
            }
        }
        state.pending_usage_rebuilds.remove(&path);
        changes.push(Change::Remove(whole_file_source(info, path)));
    }
    index_refresh_pending
}

fn begin_usage_rebuilds(
    state: &mut CodexCheckpoint,
    info: &ProviderInfo,
    paths: impl IntoIterator<Item = PathBuf>,
    changes: &mut Vec<Change>,
) -> bool {
    let mut rebuilt = false;
    for path in paths {
        let Some(rollout) = state.rollouts.remove(&path) else {
            continue;
        };
        state
            .pending_usage_rebuilds
            .entry(path.clone())
            .or_default()
            .extend(rollout.context().usage_attribution.keys().cloned());
        changes.push(Change::Reset(whole_file_source(info, path)));
        rebuilt = true;
    }
    rebuilt
}

fn preserve_usage_index(
    path: &Path,
    context: &RolloutContext,
    pending_usage_rebuilds: &mut BTreeMap<PathBuf, BTreeSet<UsageFingerprint>>,
) {
    pending_usage_rebuilds
        .entry(path.to_path_buf())
        .or_default()
        .extend(context.usage_attribution.keys().cloned());
}

fn lineage_changed(previous: &RolloutState, lineage: &LineagePlan, path: &Path) -> bool {
    let Some(expected) = lineage.segments(path) else {
        return false;
    };
    previous
        .context()
        .rollout_session
        .as_ref()
        .and_then(|session| session.history.as_ref())
        .is_some_and(|history| history.lineage != expected)
}

fn scan_plain(
    environment: &ScanEnvironment<'_>,
    batch: &mut ScanBatch<'_>,
    path: &Path,
    previous: Option<RolloutState>,
    binding: Option<&super::state_db::SessionBinding>,
) -> Result<(RolloutState, bool)> {
    let info = environment.info;
    let replay = environment.replay;
    let limits = environment.limits;
    let (mut offset, mut line, expected_tail, mut context, had_previous) = match previous {
        Some(RolloutState::Plain {
            offset,
            line,
            tail_fingerprint,
            context,
        }) => (offset, line, tail_fingerprint, context, true),
        Some(RolloutState::Compressed { context, .. }) => (0, 0, 0, context, true),
        None => (0, 0, 0, RolloutContext::default(), false),
    };
    apply_binding(binding, &mut context);
    apply_owner_hint(
        info,
        path,
        binding.is_some(),
        limits.max_line_bytes,
        &mut context,
    );
    replay.initialize(path, &mut context.usage_replay);
    let metadata =
        fs::metadata(path).map_err(|error| Error::io("inspect a Codex rollout", path, error))?;
    let tail_matches = offset <= metadata.len()
        && (offset == 0
            || tail_fingerprint(path, offset, "read a Codex rollout fingerprint")?
                == expected_tail);
    if had_previous && !tail_matches {
        batch
            .changes
            .push(Change::Reset(whole_file_source(info, path.to_path_buf())));
        preserve_usage_index(path, &context, batch.pending_usage_rebuilds);
        push_diagnostic(
            batch.diagnostics,
            Diagnostic::warning(
                "codex.rollout.reset",
                "a rollout changed before its checkpoint; the artifact was reparsed",
            )
            .with_origin(SourceRef::whole_file(info.source.clone(), path)),
        );
        offset = 0;
        line = 0;
        context = RolloutContext::default();
        apply_binding(binding, &mut context);
        apply_owner_hint(
            info,
            path,
            binding.is_some(),
            limits.max_line_bytes,
            &mut context,
        );
        replay.initialize(path, &mut context.usage_replay);
    }

    let file = File::open(path)
        .map_err(|error| Error::io("open a Codex rollout read-only", path, error))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| Error::io("seek to a Codex rollout checkpoint", path, error))?;
    let mut safe_offset = offset;
    let mut bytes = Vec::new();
    let mut has_more = false;

    loop {
        if *batch.remaining == 0 {
            has_more = true;
            break;
        }
        bytes.clear();
        let (count, too_large) =
            match read_bounded_line(&mut reader, &mut bytes, limits.max_line_bytes)
                .map_err(|error| Error::io("read a Codex rollout line", path, error))?
            {
                LineRead::Eof => break,
                LineRead::Partial { too_large } => {
                    if too_large
                        || bytes.len() > limits.max_line_bytes
                        || !is_complete_json_value(&bytes)
                    {
                        break;
                    }
                    (bytes.len(), false)
                }
                LineRead::Complete { count, too_large } => (count, too_large),
            };
        let byte_start = safe_offset;
        safe_offset = safe_offset.saturating_add(count as u64);
        line = line.saturating_add(1);
        *batch.remaining -= 1;
        if too_large {
            push_line_too_large(
                info,
                path,
                line,
                Some(byte_start),
                Some(safe_offset),
                limits,
                batch.diagnostics,
            );
            continue;
        }
        parse_line(
            environment,
            batch,
            path,
            &bytes,
            Position {
                line,
                byte_start: Some(byte_start),
                byte_end: Some(safe_offset),
                logical_ordinal: None,
            },
            &mut context,
        );
    }

    let tail = tail_fingerprint(path, safe_offset, "read a Codex rollout fingerprint")?;
    Ok((
        RolloutState::Plain {
            offset: safe_offset,
            line,
            tail_fingerprint: tail,
            context,
        },
        has_more,
    ))
}

fn scan_compressed(
    environment: &ScanEnvironment<'_>,
    batch: &mut ScanBatch<'_>,
    path: &Path,
    previous: Option<RolloutState>,
    binding: Option<&super::state_db::SessionBinding>,
) -> Result<(RolloutState, bool)> {
    let info = environment.info;
    let replay = environment.replay;
    let limits = environment.limits;
    let signature = file_signature(path)?;
    let (previous_signature, processed_lines, complete, mut context, had_previous) = match previous
    {
        Some(RolloutState::Compressed {
            signature,
            line,
            complete,
            context,
        }) => (Some(signature), line, complete, context, true),
        Some(RolloutState::Plain { context, .. }) => (None, 0, false, context, true),
        None => (None, 0, false, RolloutContext::default(), false),
    };
    apply_binding(binding, &mut context);
    apply_owner_hint(
        info,
        path,
        binding.is_some(),
        limits.max_line_bytes,
        &mut context,
    );
    replay.initialize(path, &mut context.usage_replay);
    if previous_signature == Some(signature) && complete {
        return Ok((
            RolloutState::Compressed {
                signature,
                line: processed_lines,
                complete: true,
                context,
            },
            false,
        ));
    }

    let mut skip_lines = if previous_signature == Some(signature) {
        processed_lines
    } else {
        0
    };
    if had_previous && previous_signature != Some(signature) {
        batch
            .changes
            .push(Change::Reset(whole_file_source(info, path.to_path_buf())));
        preserve_usage_index(path, &context, batch.pending_usage_rebuilds);
        push_diagnostic(
            batch.diagnostics,
            Diagnostic::warning(
                "codex.rollout.compressed_reset",
                "a compressed rollout changed and was reparsed",
            )
            .with_origin(SourceRef::whole_file(info.source.clone(), path)),
        );
        context = RolloutContext::default();
        apply_binding(binding, &mut context);
        apply_owner_hint(
            info,
            path,
            binding.is_some(),
            limits.max_line_bytes,
            &mut context,
        );
        replay.initialize(path, &mut context.usage_replay);
    }

    let file = File::open(path)
        .map_err(|error| Error::io("open a compressed Codex rollout", path, error))?;
    let decoder = zstd::stream::read::Decoder::new(file)
        .map_err(|error| Error::io("open the Codex zstd stream", path, error))?;
    let mut reader = BufReader::new(decoder);
    let mut bytes = Vec::new();
    let mut line = 0_u64;
    let mut complete = true;
    let mut has_more = false;

    loop {
        bytes.clear();
        let (too_large, final_line) =
            match read_bounded_line(&mut reader, &mut bytes, limits.max_line_bytes)
                .map_err(|error| Error::io("read a compressed Codex rollout line", path, error))?
            {
                LineRead::Eof => break,
                LineRead::Partial { too_large } => (too_large, true),
                LineRead::Complete { too_large, .. } => (too_large, false),
            };
        line = line.saturating_add(1);
        if skip_lines > 0 {
            skip_lines -= 1;
            if final_line {
                break;
            }
            continue;
        }
        if *batch.remaining == 0 {
            line = line.saturating_sub(1);
            complete = false;
            has_more = true;
            break;
        }
        *batch.remaining -= 1;
        if too_large {
            push_line_too_large(info, path, line, None, None, limits, batch.diagnostics);
        } else {
            parse_line(
                environment,
                batch,
                path,
                &bytes,
                Position {
                    line,
                    byte_start: None,
                    byte_end: None,
                    logical_ordinal: None,
                },
                &mut context,
            );
        }
        if final_line {
            break;
        }
    }
    Ok((
        RolloutState::Compressed {
            signature,
            line,
            complete,
            context,
        },
        has_more,
    ))
}

fn parse_line(
    environment: &ScanEnvironment<'_>,
    batch: &mut ScanBatch<'_>,
    path: &Path,
    bytes: &[u8],
    position: Position,
    context: &mut RolloutContext,
) {
    let source = environment.source;
    let info = environment.info;
    let replay = environment.replay;
    let lineage = environment.lineage;
    let inheritance = environment.inheritance;
    let indexed_sessions = environment.indexed_sessions;
    let contents = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let contents = contents.strip_suffix(b"\r").unwrap_or(contents);
    if contents.is_empty() {
        return;
    }
    let value: Value = match serde_json::from_slice(contents) {
        Ok(value) => value,
        Err(error) => {
            push_diagnostic(
                batch.diagnostics,
                Diagnostic::warning(
                    "codex.rollout.invalid_json",
                    format!("rollout line {} is invalid JSON: {error}", position.line),
                )
                .with_origin(SourceRef::json_line(
                    info.source.clone(),
                    path,
                    position.line,
                    position.byte_start,
                    position.byte_end,
                )),
            );
            return;
        }
    };
    let position = Position {
        logical_ordinal: value.get("ordinal").and_then(Value::as_u64),
        ..position
    };
    if value.get("type").and_then(Value::as_str) != Some("session_meta")
        && lineage.own_start_ordinal(path).is_some_and(|start| {
            position
                .logical_ordinal
                .is_none_or(|ordinal| ordinal < start)
        })
    {
        return;
    }
    let artifact = path
        .strip_prefix(source.codex_home())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    hydrate_session_snapshot(context, indexed_sessions);
    let mut records = normalize::rollout_records(
        normalize::RolloutInput {
            source,
            info,
            path,
            value: &value,
            position,
            indexed_sessions,
            lineage: lineage.segments(path),
        },
        context,
    );
    inheritance.apply(path, &mut records);
    for record in &mut records {
        if let crate::RecordData::UsageReport(usage) = &mut record.data {
            let replayed_delta = replay.filter_delta(
                path,
                &mut context.usage_replay,
                record.timestamp,
                usage.delta.take(),
            );
            usage.delta = batch.attribution.attribute(
                path,
                context,
                record.timestamp,
                usage.model.as_deref(),
                replayed_delta,
            );
        }
    }
    let (emit_records, superseded) =
        reconcile_presentation(info, &artifact, &value, context, &records);
    for id in superseded {
        if let Some(index) = batch
            .changes
            .iter()
            .rposition(|change| matches!(change, Change::Upsert(record) if record.id == id))
        {
            batch.changes.remove(index);
        } else {
            batch.changes.push(Change::Delete(id));
        }
    }
    if emit_records {
        batch
            .changes
            .extend(records.into_iter().map(Change::upsert));
    } else {
        batch.changes.extend(
            records
                .into_iter()
                .filter(|record| matches!(&record.data, crate::RecordData::AgentInvocation(_)))
                .map(Change::upsert),
        );
    }
}

fn reconcile_presentation(
    info: &ProviderInfo,
    artifact: &str,
    value: &Value,
    context: &mut RolloutContext,
    records: &[crate::Record],
) -> (bool, Vec<RecordId>) {
    match presentation_observation(value) {
        PresentationObservation::Legacy(kind) => {
            let mut remove_credit = false;
            if let Some(credit) = context.unmatched_canonical_presentations.get_mut(kind) {
                if *credit > 0 {
                    *credit -= 1;
                    remove_credit = *credit == 0;
                    if remove_credit {
                        context.unmatched_canonical_presentations.remove(kind);
                    }
                    return (false, Vec::new());
                }
            }
            if remove_credit {
                context.unmatched_canonical_presentations.remove(kind);
            }
            context
                .pending_legacy_presentations
                .entry(kind.to_owned())
                .or_default()
                .extend(
                    records
                        .iter()
                        .filter(|record| matches!(&record.data, crate::RecordData::Event(_)))
                        .map(|record| record.id.as_str().to_owned()),
                );
            (true, Vec::new())
        }
        PresentationObservation::Canonical(kinds) => {
            let mut superseded = Vec::new();
            for kind in kinds {
                let (pending, remove_pending) =
                    if let Some(records) = context.pending_legacy_presentations.get_mut(kind) {
                        let pending = (!records.is_empty()).then(|| records.remove(0));
                        (pending, records.is_empty())
                    } else {
                        (None, false)
                    };
                if remove_pending {
                    context.pending_legacy_presentations.remove(kind);
                }
                if let Some(record) = pending {
                    superseded.push(pending_legacy_record_id(info, artifact, kind, &record));
                } else {
                    *context
                        .unmatched_canonical_presentations
                        .entry(kind.to_owned())
                        .or_default() += 1;
                }
            }
            (true, superseded)
        }
        PresentationObservation::Other => (true, Vec::new()),
    }
}

fn hydrate_session_snapshot(
    context: &mut RolloutContext,
    indexed_sessions: &BTreeMap<String, Session>,
) {
    if context.session_snapshot.is_some() {
        return;
    }
    let Some(external_id) = context.session_external_id.as_ref() else {
        return;
    };
    context.session_snapshot = match (
        indexed_sessions.get(external_id),
        context.rollout_session.as_ref(),
    ) {
        (Some(indexed), Some(observed)) => {
            Some(normalize::merge_session_metadata(indexed, observed))
        }
        (Some(indexed), None) => Some(indexed.clone()),
        (None, Some(observed)) => Some(observed.clone()),
        (None, None) => None,
    };
}

fn pending_legacy_record_id(
    info: &ProviderInfo,
    artifact: &str,
    presentation_kind: &str,
    stored: &str,
) -> RecordId {
    let source_prefix = format!("{}:", info.source.as_str());
    if stored.starts_with(&source_prefix) {
        return RecordId::new(stored);
    }
    let record_kind = if presentation_kind.starts_with("reasoning:") {
        "reasoning"
    } else {
        "message"
    };
    RecordId::scoped(
        &info.source,
        record_kind,
        format!("{artifact}:legacy:{stored}"),
    )
}

enum PresentationObservation {
    Legacy(&'static str),
    Canonical(Vec<&'static str>),
    Other,
}

fn presentation_observation(value: &Value) -> PresentationObservation {
    let payload = value.get("payload").unwrap_or(&Value::Null);
    match (
        value.get("type").and_then(Value::as_str),
        payload.get("type").and_then(Value::as_str),
    ) {
        (Some("event_msg"), Some("user_message")) => {
            PresentationObservation::Legacy("message:user")
        }
        (Some("event_msg"), Some("agent_message")) => {
            PresentationObservation::Legacy("message:assistant")
        }
        (Some("event_msg"), Some("agent_reasoning")) => {
            PresentationObservation::Legacy("reasoning:summary")
        }
        (Some("event_msg"), Some("agent_reasoning_raw_content")) => {
            PresentationObservation::Legacy("reasoning:content")
        }
        (Some("response_item"), Some("message")) => {
            match payload.get("role").and_then(Value::as_str) {
                Some("user") => PresentationObservation::Canonical(vec!["message:user"]),
                Some("assistant" | "agent") => {
                    PresentationObservation::Canonical(vec!["message:assistant"])
                }
                _ => PresentationObservation::Other,
            }
        }
        (Some("response_item"), Some("reasoning")) => {
            let mut kinds = Vec::with_capacity(2);
            if nonempty_json(payload.get("summary")) {
                kinds.push("reasoning:summary");
            }
            if nonempty_json(payload.get("content")) {
                kinds.push("reasoning:content");
            }
            if kinds.is_empty() {
                PresentationObservation::Other
            } else {
                PresentationObservation::Canonical(kinds)
            }
        }
        _ => PresentationObservation::Other,
    }
}

fn nonempty_json(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Null) | None => false,
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(value)) => !value.is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
        Some(Value::Bool(_) | Value::Number(_)) => true,
    }
}

fn apply_binding(binding: Option<&super::state_db::SessionBinding>, context: &mut RolloutContext) {
    if let Some(binding) = binding {
        context.session_external_id = Some(binding.external_id.clone());
        context.session = Some(binding.record.clone());
    }
}

fn apply_owner_hint(
    info: &ProviderInfo,
    path: &Path,
    has_index_binding: bool,
    max_line_bytes: usize,
    context: &mut RolloutContext,
) {
    if has_index_binding || context.session_external_id.is_some() {
        return;
    }
    let Some(external_id) = read_rollout_owner(path, max_line_bytes) else {
        return;
    };
    context.session = Some(normalize::session_id(info, &external_id));
    context.session_external_id = Some(external_id);
}

fn rollout_catalog(source: &CodexSource) -> Result<BTreeMap<PathBuf, RolloutFileMetadata>> {
    let mut files = BTreeMap::new();
    collect_rollout_files(&source.active_sessions(), &mut files)?;
    collect_rollout_files(&source.archived_sessions(), &mut files)?;
    Ok(files)
}

fn collect_rollout_files(
    directory: &Path,
    files: &mut BTreeMap<PathBuf, RolloutFileMetadata>,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(directory)
        .map_err(|error| Error::io("read a Codex rollout directory", directory, error))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| Error::io("read a Codex rollout directory entry", directory, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| Error::io("inspect a Codex rollout entry", entry.path(), error))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_rollout_files(&entry.path(), files)?;
        } else if file_type.is_file() && is_rollout(&entry.path()) {
            let metadata = entry
                .metadata()
                .map_err(|error| Error::io("inspect a Codex rollout entry", entry.path(), error))?;
            files.insert(
                entry.path(),
                RolloutFileMetadata {
                    len: metadata.len(),
                    modified_nanos: modified_nanos(&metadata),
                },
            );
        }
    }
    Ok(())
}

fn is_rollout(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    name.ends_with(".jsonl") || name.ends_with(".jsonl.zst")
}

fn is_compressed(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".jsonl.zst"))
}

fn whole_file_source(info: &ProviderInfo, path: PathBuf) -> SourceRef {
    SourceRef::whole_file(info.source.clone(), path)
}

fn push_line_too_large(
    info: &ProviderInfo,
    path: &Path,
    line: u64,
    byte_start: Option<u64>,
    byte_end: Option<u64>,
    limits: &ScanLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    push_diagnostic(
        diagnostics,
        Diagnostic::warning(
            "codex.rollout.line_too_large",
            format!(
                "rollout line {line} exceeds the {} byte limit and was skipped",
                limits.max_line_bytes
            ),
        )
        .with_origin(SourceRef::json_line(
            info.source.clone(),
            path,
            line,
            byte_start,
            byte_end,
        )),
    );
}

pub(super) fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if diagnostics.len() + 1 < MAX_DIAGNOSTICS_PER_BATCH {
        diagnostics.push(diagnostic);
    } else if diagnostics.len() + 1 == MAX_DIAGNOSTICS_PER_BATCH {
        diagnostics.push(Diagnostic::warning(
            "codex.diagnostics.truncated",
            "additional Codex diagnostics were omitted from this batch",
        ));
    }
}

fn file_signature(path: &Path) -> Result<FileSignature> {
    let metadata =
        fs::metadata(path).map_err(|error| Error::io("inspect a Codex rollout", path, error))?;
    Ok(FileSignature {
        len: metadata.len(),
        modified_nanos: modified_nanos(&metadata),
        tail_fingerprint: tail_fingerprint(path, metadata.len(), "read a Codex rollout signature")?,
    })
}

fn modified_nanos(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use crate::providers::codex::{CodexProvider, CodexSource};
    use crate::Provider;

    use super::{
        scan_plain, CodexCheckpoint, InheritancePlan, LineagePlan, ReplayPlan, RolloutState,
        ScanBatch, ScanEnvironment, ScanLimits, UsageAttributionPlan,
    };

    #[test]
    fn malformed_lines_advance_in_bounded_batches() {
        let directory = tempfile::tempdir().unwrap();
        let codex_home = directory.path().join(".codex");
        let sessions = codex_home.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("rollout-test.jsonl");
        fs::write(&path, b"not-json\nalso-not-json\nstill-not-json\n").unwrap();
        let source = CodexSource::new(&codex_home, &codex_home);
        let provider = CodexProvider::new(source.clone());
        let limits = ScanLimits {
            max_lines_per_batch: 2,
            max_line_bytes: 1024,
        };
        let indexed_sessions = BTreeMap::new();
        let replay = ReplayPlan::empty();
        let lineage = LineagePlan::empty();
        let inheritance = InheritancePlan::empty();
        let mut remaining = 2;
        let mut changes = Vec::new();
        let mut diagnostics = Vec::new();
        let mut attribution = UsageAttributionPlan::build(&CodexCheckpoint::default());
        let mut pending_usage_rebuilds = BTreeMap::new();
        let environment = ScanEnvironment {
            source: &source,
            info: provider.info(),
            replay: &replay,
            lineage: &lineage,
            inheritance: &inheritance,
            indexed_sessions: &indexed_sessions,
            limits: &limits,
        };

        let (first_state, first_has_more) = {
            let mut batch = ScanBatch {
                attribution: &mut attribution,
                pending_usage_rebuilds: &mut pending_usage_rebuilds,
                remaining: &mut remaining,
                changes: &mut changes,
                diagnostics: &mut diagnostics,
            };
            scan_plain(&environment, &mut batch, &path, None, None)
        }
        .unwrap();

        assert!(first_has_more);
        assert!(changes.is_empty());
        assert_eq!(diagnostics.len(), 2);
        assert!(matches!(first_state, RolloutState::Plain { line: 2, .. }));

        let mut remaining = 2;
        let mut diagnostics = Vec::new();
        let (second_state, second_has_more) = {
            let mut batch = ScanBatch {
                attribution: &mut attribution,
                pending_usage_rebuilds: &mut pending_usage_rebuilds,
                remaining: &mut remaining,
                changes: &mut changes,
                diagnostics: &mut diagnostics,
            };
            scan_plain(&environment, &mut batch, &path, Some(first_state), None)
        }
        .unwrap();

        assert!(!second_has_more);
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(second_state, RolloutState::Plain { line: 3, .. }));
    }
}
