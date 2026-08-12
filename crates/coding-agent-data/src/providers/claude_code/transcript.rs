use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::providers::shared::jsonl::{
    fingerprint_json, is_complete_json_value, read_bounded_line, tail_fingerprint, LineRead,
};
use crate::{
    Batch, Change, Checkpoint, Diagnostic, Error, ProviderInfo, Record, RecordId, Result, SourceRef,
};

use super::checkpoint::{self, EmittedUsage, TranscriptContext, TranscriptState, UsageSnapshot};
use super::normalize::{self, Position};
use super::ClaudeCodeSource;

const MAX_DIAGNOSTICS_PER_BATCH: usize = 1_000;
// Child-agent linkage is metadata, not the transcript body. Keep this index
// bounded so a large or malformed child file cannot turn every scan into a
// full-file read. The path remains the stable child-session identity. An
// agent ID is used for linkage only when it is explicitly persisted.
const MAX_CHILD_SESSION_INDEX_LINES: usize = 64;
const MAX_CHILD_SESSION_INDEX_BYTES: usize = 4 * 1024 * 1024;
const MAX_CHILD_SESSION_INDEX_LINE_BYTES: usize = 512 * 1024;

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

pub(super) fn scan(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    limits: &ScanLimits,
    checkpoint: Option<&Checkpoint>,
) -> Result<Batch> {
    let mut state = checkpoint::decode(info, checkpoint)?;
    let mut diagnostics = Vec::new();
    let mut changes = Vec::new();
    let files = transcript_files(source)?;
    let current_files: BTreeSet<PathBuf> = files.iter().cloned().collect();
    let child_sessions = child_session_index(source, info, &files);
    let mut invalidated_paths = BTreeSet::new();
    let mut remaining = limits.max_lines_per_batch;
    let mut has_more = false;

    for path in files {
        if remaining == 0 {
            has_more = true;
            break;
        }
        let previous = state.transcripts.get(&path).cloned();
        let (next, file_has_more) = scan_transcript(
            source,
            info,
            &path,
            previous,
            &child_sessions,
            limits,
            &mut remaining,
            &mut changes,
            &mut diagnostics,
            &mut invalidated_paths,
        )?;
        state.transcripts.insert(path, next);
        has_more |= file_has_more;
    }

    let removed: Vec<PathBuf> = state
        .transcripts
        .keys()
        .filter(|path| !current_files.contains(*path))
        .cloned()
        .collect();
    let mut deleted_sessions = BTreeSet::new();
    for path in removed {
        let previous = state.transcripts.remove(&path);
        let identity = normalize::transcript_session_identity(source, &path)
            .map(|identity| (identity.project_key, identity.external_id))
            .or_else(|| {
                previous.as_ref().and_then(|previous| {
                    previous.summary.as_ref().map(|summary| {
                        (
                            summary.project_key.clone(),
                            summary.session.external_id.clone(),
                        )
                    })
                })
            });
        if let Some((project, session)) = identity {
            if deleted_sessions.insert((project.clone(), session.clone())) {
                changes.push(Change::Delete(normalize::session_record_id(
                    info, &project, &session,
                )));
            }
        }
        changes.push(Change::Remove(whole_file_source(info, path)));
    }
    reconcile_usage(source, info, &mut state, &invalidated_paths, &mut changes)?;

    Ok(Batch::new(
        changes,
        checkpoint::encode(info, state)?,
        diagnostics,
        has_more,
    ))
}

#[allow(clippy::too_many_arguments)]
fn scan_transcript(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    path: &Path,
    previous: Option<TranscriptState>,
    child_sessions: &BTreeMap<String, RecordId>,
    limits: &ScanLimits,
    remaining: &mut usize,
    changes: &mut Vec<Change>,
    diagnostics: &mut Vec<Diagnostic>,
    invalidated_paths: &mut BTreeSet<PathBuf>,
) -> Result<(TranscriptState, bool)> {
    let default_summary = normalize::default_summary(source, info, path);
    let (
        mut offset,
        mut line,
        expected_tail,
        mut summary,
        mut emitted_summary_fingerprint,
        mut usage,
        mut context,
        had_previous,
    ) = match previous {
        Some(previous) => (
            previous.offset,
            previous.line,
            previous.tail_fingerprint,
            previous.summary,
            previous.emitted_summary_fingerprint,
            previous.usage,
            previous.context,
            true,
        ),
        None => (
            0,
            0,
            0,
            default_summary.clone(),
            None,
            Default::default(),
            TranscriptContext::default(),
            false,
        ),
    };
    if summary.is_none() {
        summary = default_summary.clone();
        emitted_summary_fingerprint = None;
    }
    let metadata = fs::metadata(path)
        .map_err(|error| Error::io("inspect a Claude Code transcript", path, error))?;
    let tail_matches = offset <= metadata.len()
        && (offset == 0
            || tail_fingerprint(path, offset, "read a Claude Code transcript fingerprint")?
                == expected_tail);
    if had_previous && !tail_matches {
        changes.push(Change::Reset(whole_file_source(info, path.to_path_buf())));
        push_diagnostic(
            diagnostics,
            Diagnostic::warning(
                "claude_code.transcript.reset",
                "a transcript changed before its checkpoint; the artifact was reparsed",
            )
            .with_origin(SourceRef::whole_file(info.source.clone(), path)),
        );
        offset = 0;
        line = 0;
        summary = default_summary;
        emitted_summary_fingerprint = None;
        usage.clear();
        context = TranscriptContext::default();
        invalidated_paths.insert(path.to_path_buf());
    }

    let file = File::open(path)
        .map_err(|error| Error::io("open a Claude Code transcript read-only", path, error))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| Error::io("seek to a Claude Code transcript checkpoint", path, error))?;
    let mut safe_offset = offset;
    let mut bytes = Vec::new();
    let mut has_more = false;

    loop {
        if *remaining == 0 {
            has_more = true;
            break;
        }
        bytes.clear();
        let (count, too_large) =
            match read_bounded_line(&mut reader, &mut bytes, limits.max_line_bytes)
                .map_err(|error| Error::io("read a Claude Code transcript line", path, error))?
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
        *remaining -= 1;
        if too_large {
            push_line_too_large(
                info,
                path,
                line,
                byte_start,
                safe_offset,
                limits,
                diagnostics,
            );
            changes.push(Change::upsert(normalize::line_too_large_record(
                source,
                info,
                path,
                normalize::Position {
                    line,
                    byte_start,
                    byte_end: safe_offset,
                },
                &context,
            )));
            continue;
        }
        let contents = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
        let contents = contents.strip_suffix(b"\r").unwrap_or(contents);
        if contents.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_slice(contents) {
            Ok(value) => value,
            Err(error) => {
                push_diagnostic(
                    diagnostics,
                    Diagnostic::warning(
                        "claude_code.transcript.invalid_json",
                        format!("transcript line {line} is invalid JSON: {error}"),
                    )
                    .with_origin(SourceRef::json_line(
                        info.source.clone(),
                        path,
                        line,
                        Some(byte_start),
                        Some(safe_offset),
                    )),
                );
                continue;
            }
        };
        if let Some(summary) = summary.as_mut() {
            normalize::update_summary(info, summary, &value);
        }
        let position = Position {
            line,
            byte_start,
            byte_end: safe_offset,
        };
        let invocation_before = context.current_invocation.clone();
        changes.extend(
            normalize::line_records(
                source,
                info,
                path,
                &value,
                position,
                &mut context,
                child_sessions,
            )
            .into_iter()
            .map(Change::upsert),
        );
        for (key, snapshot) in normalize::usage_snapshots(
            &value,
            position,
            invocation_before.or_else(|| context.current_invocation.clone()),
        ) {
            if normalize::should_replace_usage(usage.get(&key), &snapshot) {
                usage.insert(key, snapshot);
            }
        }
    }

    if let Some(summary) = summary.as_ref() {
        let value = serde_json::to_value(summary)
            .map_err(|error| Error::InvalidCheckpoint(error.to_string()))?;
        let fingerprint = fingerprint_json(&value);
        if emitted_summary_fingerprint != Some(fingerprint) {
            changes.push(Change::upsert(normalize::summary_record(
                info, path, summary,
            )));
            emitted_summary_fingerprint = Some(fingerprint);
        }
    }
    let tail = tail_fingerprint(
        path,
        safe_offset,
        "read a Claude Code transcript fingerprint",
    )?;
    Ok((
        TranscriptState {
            offset: safe_offset,
            line,
            tail_fingerprint: tail,
            summary,
            emitted_summary_fingerprint,
            usage,
            context,
        },
        has_more,
    ))
}

struct UsageCandidate {
    snapshot: UsageSnapshot,
    record: Record,
}

#[derive(Clone, Copy)]
struct UsageMessageIndexes {
    first: usize,
    sidechain: Option<usize>,
}

fn reconcile_usage(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    state: &mut checkpoint::ClaudeCheckpoint,
    invalidated_paths: &BTreeSet<PathBuf>,
    changes: &mut Vec<Change>,
) -> Result<()> {
    let mut selected: Vec<UsageCandidate> = Vec::new();
    let mut exact_indexes = BTreeMap::new();
    let mut message_indexes: BTreeMap<String, UsageMessageIndexes> = BTreeMap::new();
    for (path, transcript) in &state.transcripts {
        for (key, snapshot) in &transcript.usage {
            let candidate = UsageCandidate {
                snapshot: snapshot.clone(),
                record: normalize::usage_record(source, info, path, key, snapshot),
            };
            let exact_key = usage_exact_key(&candidate);
            let message_key = usage_message_key(&candidate);
            let duplicate = exact_key
                .as_ref()
                .and_then(|key| exact_indexes.get(key).copied())
                .or_else(|| {
                    let indexes = message_indexes.get(message_key.as_ref()?)?;
                    if candidate.snapshot.sidechain {
                        Some(indexes.first)
                    } else {
                        indexes.sidechain
                    }
                });
            if let Some(index) = duplicate {
                if normalize::should_replace_usage(
                    Some(&selected[index].snapshot),
                    &candidate.snapshot,
                ) {
                    if let Some(key) = usage_exact_key(&selected[index]) {
                        exact_indexes.remove(&key);
                    }
                    let sidechain = candidate.snapshot.sidechain;
                    selected[index] = candidate;
                    if let Some(key) = exact_key {
                        exact_indexes.insert(key, index);
                    }
                    if let Some(key) = message_key {
                        let indexes = message_indexes.entry(key).or_insert(UsageMessageIndexes {
                            first: index,
                            sidechain: None,
                        });
                        if sidechain {
                            indexes.sidechain = Some(index);
                        } else if indexes.sidechain == Some(index) {
                            indexes.sidechain = None;
                        }
                    }
                }
            } else {
                let index = selected.len();
                if let Some(key) = exact_key {
                    exact_indexes.insert(key, index);
                }
                if let Some(key) = message_key {
                    message_indexes.entry(key).or_insert(UsageMessageIndexes {
                        first: index,
                        sidechain: candidate.snapshot.sidechain.then_some(index),
                    });
                }
                selected.push(candidate);
            }
        }
    }

    let mut next_emitted = BTreeMap::new();
    let mut upserts = Vec::new();
    for candidate in selected {
        let id = candidate.record.id.as_str().to_owned();
        let value = serde_json::to_value(&candidate.record)
            .map_err(|error| Error::InvalidCheckpoint(error.to_string()))?;
        let fingerprint = fingerprint_json(&value);
        let emitted = EmittedUsage {
            fingerprint,
            path: candidate.record.origin.path.clone(),
        };
        let unchanged = state
            .emitted_usage
            .get(&id)
            .is_some_and(|previous| previous == &emitted);
        if !unchanged || invalidated_paths.contains(&emitted.path) {
            upserts.push(Change::upsert(candidate.record));
        }
        next_emitted.insert(id, emitted);
    }

    for id in state
        .emitted_usage
        .keys()
        .filter(|id| !next_emitted.contains_key(*id))
    {
        changes.push(Change::Delete(RecordId::new(id.clone())));
    }
    changes.extend(upserts);
    state.emitted_usage = next_emitted;
    Ok(())
}

fn usage_message_key(candidate: &UsageCandidate) -> Option<String> {
    candidate.snapshot.message_id.clone()
}

fn usage_exact_key(candidate: &UsageCandidate) -> Option<(String, Option<String>)> {
    Some((
        candidate.snapshot.message_id.clone()?,
        candidate.snapshot.request_id.clone(),
    ))
}

fn transcript_files(source: &ClaudeCodeSource) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_transcripts(&source.projects_dir(), &source.projects_dir(), &mut files)?;
    files.sort();
    Ok(files)
}

fn child_session_index(
    source: &ClaudeCodeSource,
    info: &ProviderInfo,
    files: &[PathBuf],
) -> BTreeMap<String, RecordId> {
    let mut index = BTreeMap::new();
    for path in files {
        let Some(identity) = normalize::transcript_session_identity(source, path) else {
            continue;
        };
        if identity.is_main {
            continue;
        }
        let record_id =
            normalize::session_record_id(info, &identity.project_key, &identity.external_id);
        if let Some(agent_id) = identity.agent_id.as_ref() {
            index
                .entry(agent_id.clone())
                .or_insert_with(|| record_id.clone());
        }
        let Ok(file) = File::open(path) else { continue };
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();
        let mut scanned_lines = 0;
        let mut scanned_bytes = 0;
        while scanned_lines < MAX_CHILD_SESSION_INDEX_LINES
            && scanned_bytes < MAX_CHILD_SESSION_INDEX_BYTES
        {
            let Ok(line) =
                read_bounded_line(&mut reader, &mut bytes, MAX_CHILD_SESSION_INDEX_LINE_BYTES)
            else {
                break;
            };
            let LineRead::Complete { count, too_large } = line else {
                break;
            };
            scanned_lines += 1;
            scanned_bytes = scanned_bytes.saturating_add(count);
            if too_large {
                continue;
            }
            let contents = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
            let contents = contents.strip_suffix(b"\r").unwrap_or(contents);
            if contents.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<Value>(contents) else {
                continue;
            };
            if let Some(agent_id) = value
                .get("agentId")
                .or_else(|| value.get("agent_id"))
                .and_then(Value::as_str)
                .filter(|agent_id| !agent_id.is_empty())
            {
                index.insert(agent_id.to_owned(), record_id.clone());
                break;
            }
        }
    }
    index
}

fn collect_transcripts(projects: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(directory)
        .map_err(|error| Error::io("read a Claude Code project directory", directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            Error::io(
                "read a Claude Code project directory entry",
                directory,
                error,
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            Error::io("inspect a Claude Code project entry", entry.path(), error)
        })?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_transcripts(projects, &entry.path(), files)?;
        } else if file_type.is_file() && is_transcript(projects, &entry.path()) {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn is_transcript(projects: &Path, path: &Path) -> bool {
    if path.extension() != Some(OsStr::new("jsonl")) {
        return false;
    }
    let Ok(relative) = path.strip_prefix(projects) else {
        return false;
    };
    let components: Vec<_> = relative.components().collect();
    components.len() == 2
        || (components.len() == 4 && components[2].as_os_str() == OsStr::new("subagents"))
}

fn whole_file_source(info: &ProviderInfo, path: PathBuf) -> SourceRef {
    SourceRef::whole_file(info.source.clone(), path)
}

fn push_line_too_large(
    info: &ProviderInfo,
    path: &Path,
    line: u64,
    byte_start: u64,
    byte_end: u64,
    limits: &ScanLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    push_diagnostic(
        diagnostics,
        Diagnostic::warning(
            "claude_code.transcript.line_too_large",
            format!(
                "transcript line {line} exceeds the {} byte limit and was emitted as an unparsed placeholder",
                limits.max_line_bytes
            ),
        )
        .with_origin(SourceRef::json_line(
            info.source.clone(),
            path,
            line,
            Some(byte_start),
            Some(byte_end),
        )),
    );
}

fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if diagnostics.len() + 1 < MAX_DIAGNOSTICS_PER_BATCH {
        diagnostics.push(diagnostic);
    } else if diagnostics.len() + 1 == MAX_DIAGNOSTICS_PER_BATCH {
        diagnostics.push(Diagnostic::warning(
            "claude_code.diagnostics.truncated",
            "additional Claude Code diagnostics were omitted from this batch",
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;

    use crate::providers::claude_code::{ClaudeCodeProvider, ClaudeCodeSource};
    use crate::{Change, Provider, RecordData};

    use super::{scan_transcript, ScanLimits};

    #[test]
    fn oversized_lines_emit_placeholder_records() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().join(".claude");
        let project_dir = config_dir.join("projects/-workspace-project");
        fs::create_dir_all(&project_dir).unwrap();
        let path = project_dir.join("session-1.jsonl");
        fs::write(&path, b"{\"content\":\"this line is too large\"}\n").unwrap();
        let source = ClaudeCodeSource::new(&config_dir);
        let provider = ClaudeCodeProvider::new(source.clone());
        let limits = ScanLimits {
            max_lines_per_batch: 1,
            max_line_bytes: 8,
        };
        let mut remaining = 1;
        let mut changes = Vec::new();
        let mut diagnostics = Vec::new();
        let mut invalidated_paths = BTreeSet::new();

        scan_transcript(
            &source,
            provider.info(),
            &path,
            None,
            &BTreeMap::new(),
            &limits,
            &mut remaining,
            &mut changes,
            &mut diagnostics,
            &mut invalidated_paths,
        )
        .unwrap();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "claude_code.transcript.line_too_large"));
        assert!(changes.iter().any(|change| {
            matches!(
                change,
                Change::Upsert(record)
                    if matches!(
                        &record.data,
                        RecordData::Unknown(unknown)
                            if unknown.kind.as_deref() == Some("line_too_large")
                    )
                        && record.original.is_none()
            )
        }));
    }
}
