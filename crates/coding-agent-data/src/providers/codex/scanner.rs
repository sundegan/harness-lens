use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use super::{CodexSource, PROVIDER_ID};
use crate::{
    ChangeBatch, Checkpoint, DataChange, DataKind, DataRecord, DataTimestamp, Diagnostic, Error,
    ProviderId, RecordKey, Result, SourceLocation, SourceRef,
};

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

const CHECKPOINT_VERSION: u32 = 1;
const MAX_DIAGNOSTICS_PER_BATCH: usize = 1_000;
const STATE_DATABASE_FILENAME: &str = "state_5.sqlite";
const TAIL_FINGERPRINT_BYTES: u64 = 4 * 1024;

#[derive(Clone, Debug)]
pub(super) struct ScanLimits {
    max_records_per_batch: usize,
    max_line_bytes: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_records_per_batch: 100_000,
            max_line_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CodexCheckpoint {
    version: u32,
    codex_home: PathBuf,
    sqlite_home: PathBuf,
    thread_fingerprints: BTreeMap<String, u64>,
    rollout_files: BTreeMap<PathBuf, RolloutCheckpoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "format", rename_all = "snake_case")]
enum RolloutCheckpoint {
    Plain {
        offset: u64,
        line: u64,
        tail_fingerprint: u64,
    },
    Compressed {
        signature: FileSignature,
        line: u64,
        complete: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
struct FileSignature {
    len: u64,
    modified_nanos: u64,
}

pub(super) fn scan(
    source: &CodexSource,
    limits: &ScanLimits,
    checkpoint: Option<&Checkpoint>,
) -> Result<ChangeBatch> {
    let mut diagnostics = Vec::new();
    let mut state = decode_checkpoint(source, checkpoint, &mut diagnostics)?;
    let mut changes = Vec::new();

    scan_threads(source, &mut state, &mut changes, &mut diagnostics)?;
    let has_more = scan_rollouts(source, limits, &mut state, &mut changes, &mut diagnostics)?;

    let state =
        serde_json::to_value(state).map_err(|error| Error::InvalidCheckpoint(error.to_string()))?;
    Ok(ChangeBatch {
        changes,
        checkpoint: Checkpoint::new(ProviderId::new(PROVIDER_ID), state),
        diagnostics,
        has_more,
    })
}

fn decode_checkpoint(
    source: &CodexSource,
    checkpoint: Option<&Checkpoint>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<CodexCheckpoint> {
    let Some(checkpoint) = checkpoint else {
        return Ok(new_checkpoint(source));
    };
    if checkpoint.provider().as_str() != PROVIDER_ID {
        return Err(Error::InvalidCheckpoint(format!(
            "expected provider {PROVIDER_ID}, found {}",
            checkpoint.provider().as_str()
        )));
    }

    let parsed: CodexCheckpoint = serde_json::from_value(checkpoint.state().clone())
        .map_err(|error| Error::InvalidCheckpoint(error.to_string()))?;
    if parsed.version != CHECKPOINT_VERSION {
        return Err(Error::InvalidCheckpoint(format!(
            "unsupported Codex checkpoint version {}",
            parsed.version
        )));
    }
    if parsed.codex_home != source.codex_home() || parsed.sqlite_home != source.sqlite_home() {
        push_diagnostic(
            diagnostics,
            Diagnostic::warning(
                "codex.checkpoint.source_changed",
                "the Codex data directories changed; a fresh scan was started",
            ),
        );
        return Ok(new_checkpoint(source));
    }
    Ok(parsed)
}

fn new_checkpoint(source: &CodexSource) -> CodexCheckpoint {
    CodexCheckpoint {
        version: CHECKPOINT_VERSION,
        codex_home: source.codex_home().to_path_buf(),
        sqlite_home: source.sqlite_home().to_path_buf(),
        ..CodexCheckpoint::default()
    }
}

fn scan_threads(
    source: &CodexSource,
    state: &mut CodexCheckpoint,
    changes: &mut Vec<DataChange>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let path = source.state_database_path();
    if !path.is_file() {
        push_diagnostic(
            diagnostics,
            Diagnostic::warning(
                "codex.state_db.missing",
                "the Codex state database is not available; rollout files are still scanned",
            )
            .with_source(path),
        );
        return Ok(());
    }

    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| Error::sqlite("open the Codex state database read-only", &path, error))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|error| Error::sqlite("configure the Codex state database", &path, error))?;
    connection
        .execute_batch("PRAGMA query_only = ON;")
        .map_err(|error| Error::sqlite("enable SQLite query-only mode", &path, error))?;

    let mut statement = match connection.prepare("SELECT * FROM threads ORDER BY id") {
        Ok(statement) => statement,
        Err(error) => {
            push_diagnostic(
                diagnostics,
                Diagnostic::warning(
                    "codex.state_db.threads_unavailable",
                    format!("the threads table could not be read: {error}"),
                )
                .with_source(path),
            );
            return Ok(());
        }
    };
    let column_names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let mut rows = statement
        .query([])
        .map_err(|error| Error::sqlite("query Codex threads", &path, error))?;
    let mut current_fingerprints = BTreeMap::new();

    while let Some(row) = rows
        .next()
        .map_err(|error| Error::sqlite("read a Codex thread row", &path, error))?
    {
        let mut object = Map::with_capacity(column_names.len());
        for (index, name) in column_names.iter().enumerate() {
            let value = row
                .get_ref(index)
                .map(sqlite_value_to_json)
                .map_err(|error| Error::sqlite("decode a Codex thread row", &path, error))?;
            object.insert(name.clone(), value);
        }
        let payload = Value::Object(object);
        let Some(id) = payload.get("id").and_then(Value::as_str) else {
            push_diagnostic(
                diagnostics,
                Diagnostic::warning(
                    "codex.state_db.thread_without_id",
                    "a Codex thread row without a string id was skipped",
                )
                .with_source(&path),
            );
            continue;
        };

        let fingerprint = fingerprint_json(&payload);
        current_fingerprints.insert(id.to_owned(), fingerprint);
        if state.thread_fingerprints.get(id) == Some(&fingerprint) {
            continue;
        }

        changes.push(DataChange::Upsert {
            record: DataRecord {
                key: thread_record_key(id),
                kind: DataKind::Session,
                timestamp: thread_timestamp(&payload),
                source: SourceRef {
                    provider: ProviderId::new(PROVIDER_ID),
                    path: path.clone(),
                    location: SourceLocation::SqliteRow {
                        database: STATE_DATABASE_FILENAME.to_owned(),
                        table: "threads".to_owned(),
                        key: id.to_owned(),
                    },
                },
                payload,
            },
        });
    }

    for removed_id in state
        .thread_fingerprints
        .keys()
        .filter(|id| !current_fingerprints.contains_key(*id))
    {
        changes.push(DataChange::Delete {
            key: thread_record_key(removed_id),
        });
    }
    state.thread_fingerprints = current_fingerprints;
    Ok(())
}

fn sqlite_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::Number(Number::from(value)),
        ValueRef::Real(value) => Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_string())),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(format!("hex:{}", encode_hex(value))),
    }
}

fn encode_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn thread_record_key(id: &str) -> RecordKey {
    RecordKey::new(format!("{PROVIDER_ID}:session:{id}"))
}

fn thread_timestamp(payload: &Value) -> Option<DataTimestamp> {
    payload
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .or_else(|| {
            payload
                .get("updated_at")
                .and_then(Value::as_i64)
                .map(unix_millis)
        })
        .map(DataTimestamp::UnixMilliseconds)
}

fn unix_millis(value: i64) -> i64 {
    if value.unsigned_abs() >= 1_000_000_000_000 {
        value
    } else {
        value.saturating_mul(1000)
    }
}

fn scan_rollouts(
    source: &CodexSource,
    limits: &ScanLimits,
    state: &mut CodexCheckpoint,
    changes: &mut Vec<DataChange>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool> {
    let files = rollout_files(source)?;
    let current_files: BTreeSet<PathBuf> = files.iter().cloned().collect();
    let mut remaining_records = limits.max_records_per_batch.saturating_sub(changes.len());
    let mut has_more = false;

    for path in files {
        if remaining_records == 0 {
            has_more = true;
            break;
        }

        let previous = state.rollout_files.get(&path).cloned();
        let (next, file_has_more) = if is_compressed_rollout(&path) {
            scan_compressed_rollout(
                source,
                &path,
                previous,
                limits,
                &mut remaining_records,
                changes,
                diagnostics,
            )?
        } else {
            scan_plain_rollout(
                source,
                &path,
                previous,
                limits,
                &mut remaining_records,
                changes,
                diagnostics,
            )?
        };
        state.rollout_files.insert(path, next);
        has_more |= file_has_more;
    }

    let removed: Vec<PathBuf> = state
        .rollout_files
        .keys()
        .filter(|path| !current_files.contains(*path))
        .cloned()
        .collect();
    for path in removed {
        changes.push(DataChange::RemoveSource {
            source: whole_file_source(path.clone()),
        });
        state.rollout_files.remove(&path);
    }

    Ok(has_more)
}

fn scan_plain_rollout(
    source: &CodexSource,
    path: &Path,
    previous: Option<RolloutCheckpoint>,
    limits: &ScanLimits,
    remaining_records: &mut usize,
    changes: &mut Vec<DataChange>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(RolloutCheckpoint, bool)> {
    let (mut offset, mut line, expected_tail, had_previous) = match previous {
        Some(RolloutCheckpoint::Plain {
            offset,
            line,
            tail_fingerprint,
        }) => (offset, line, tail_fingerprint, true),
        Some(RolloutCheckpoint::Compressed { .. }) => (0, 0, 0, true),
        None => (0, 0, 0, false),
    };
    let metadata =
        fs::metadata(path).map_err(|error| Error::io("inspect a Codex rollout", path, error))?;
    let tail_matches = offset <= metadata.len()
        && (offset == 0 || tail_fingerprint(path, offset)? == expected_tail);
    if had_previous && !tail_matches {
        changes.push(DataChange::ResetSource {
            source: whole_file_source(path.to_path_buf()),
        });
        push_diagnostic(
            diagnostics,
            Diagnostic::warning(
                "codex.rollout.reset",
                "a rollout changed before its checkpoint; the source was reparsed",
            )
            .with_source(path),
        );
        offset = 0;
        line = 0;
    }

    let file = File::open(path)
        .map_err(|error| Error::io("open a Codex rollout read-only", path, error))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| Error::io("seek to the Codex rollout checkpoint", path, error))?;
    let mut safe_offset = offset;
    let mut bytes = Vec::new();
    let mut has_more = false;

    loop {
        if *remaining_records == 0 {
            has_more = true;
            break;
        }
        bytes.clear();
        let (count, too_large) =
            match read_bounded_line(&mut reader, &mut bytes, limits.max_line_bytes)
                .map_err(|error| Error::io("read a Codex rollout line", path, error))?
            {
                LineRead::Eof | LineRead::Partial { .. } => break,
                LineRead::Complete { count, too_large } => (count, too_large),
            };

        let byte_start = safe_offset;
        safe_offset = safe_offset.saturating_add(count as u64);
        line = line.saturating_add(1);
        *remaining_records -= 1;
        if too_large {
            push_line_too_large_diagnostic(path, line, limits, diagnostics);
            continue;
        }
        parse_rollout_line(
            source,
            path,
            &bytes,
            RolloutPosition {
                line,
                byte_start: Some(byte_start),
                byte_end: Some(safe_offset),
            },
            changes,
            diagnostics,
        );
    }

    let tail_fingerprint = tail_fingerprint(path, safe_offset)?;
    Ok((
        RolloutCheckpoint::Plain {
            offset: safe_offset,
            line,
            tail_fingerprint,
        },
        has_more,
    ))
}

fn scan_compressed_rollout(
    source: &CodexSource,
    path: &Path,
    previous: Option<RolloutCheckpoint>,
    limits: &ScanLimits,
    remaining_records: &mut usize,
    changes: &mut Vec<DataChange>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(RolloutCheckpoint, bool)> {
    let signature = file_signature(path)?;
    let (previous_signature, processed_lines, complete, had_previous) = match previous {
        Some(RolloutCheckpoint::Compressed {
            signature,
            line,
            complete,
        }) => (Some(signature), line, complete, true),
        Some(RolloutCheckpoint::Plain { .. }) => (None, 0, false, true),
        None => (None, 0, false, false),
    };
    if previous_signature == Some(signature) && complete {
        return Ok((
            RolloutCheckpoint::Compressed {
                signature,
                line: processed_lines,
                complete: true,
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
        changes.push(DataChange::ResetSource {
            source: whole_file_source(path.to_path_buf()),
        });
        push_diagnostic(
            diagnostics,
            Diagnostic::warning(
                "codex.rollout.compressed_reset",
                "a compressed rollout changed and was reparsed",
            )
            .with_source(path),
        );
    }

    let file = File::open(path)
        .map_err(|error| Error::io("open a compressed Codex rollout", path, error))?;
    let decoder = zstd::stream::read::Decoder::new(file)
        .map_err(|error| Error::io("open the Codex zstd stream", path, error))?;
    let mut reader = BufReader::new(decoder);
    let mut bytes = Vec::new();
    let mut line = 0_u64;
    let mut is_complete = true;
    let mut has_more = false;

    loop {
        bytes.clear();
        let (too_large, is_final_line) =
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
            if is_final_line {
                break;
            }
            continue;
        }
        if *remaining_records == 0 {
            line = line.saturating_sub(1);
            is_complete = false;
            has_more = true;
            break;
        }
        *remaining_records -= 1;
        if too_large {
            push_line_too_large_diagnostic(path, line, limits, diagnostics);
            continue;
        }
        parse_rollout_line(
            source,
            path,
            &bytes,
            RolloutPosition {
                line,
                byte_start: None,
                byte_end: None,
            },
            changes,
            diagnostics,
        );
        if is_final_line {
            break;
        }
    }

    Ok((
        RolloutCheckpoint::Compressed {
            signature,
            line,
            complete: is_complete,
        },
        has_more,
    ))
}

#[derive(Clone, Copy)]
struct RolloutPosition {
    line: u64,
    byte_start: Option<u64>,
    byte_end: Option<u64>,
}

fn parse_rollout_line(
    source: &CodexSource,
    path: &Path,
    bytes: &[u8],
    position: RolloutPosition,
    changes: &mut Vec<DataChange>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let contents = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let contents = contents.strip_suffix(b"\r").unwrap_or(contents);
    if contents.is_empty() {
        return;
    }
    let payload: Value = match serde_json::from_slice(contents) {
        Ok(payload) => payload,
        Err(error) => {
            push_diagnostic(
                diagnostics,
                Diagnostic::warning(
                    "codex.rollout.invalid_json",
                    format!("rollout line {} is invalid JSON: {error}", position.line),
                )
                .with_source(path),
            );
            return;
        }
    };
    let source_identity = rollout_identity(source, path);
    let position_identity = position
        .byte_start
        .map(|offset| format!("byte:{offset}"))
        .unwrap_or_else(|| format!("line:{}", position.line));
    changes.push(DataChange::Upsert {
        record: DataRecord {
            key: RecordKey::new(format!(
                "{PROVIDER_ID}:event:{source_identity}:{position_identity}"
            )),
            kind: DataKind::Event,
            timestamp: payload
                .get("timestamp")
                .and_then(Value::as_str)
                .map(|value| DataTimestamp::Rfc3339(value.to_owned())),
            source: SourceRef {
                provider: ProviderId::new(PROVIDER_ID),
                path: path.to_path_buf(),
                location: SourceLocation::JsonLine {
                    line: position.line,
                    byte_start: position.byte_start,
                    byte_end: position.byte_end,
                },
            },
            payload,
        },
    });
}

enum LineRead {
    Eof,
    Partial { too_large: bool },
    Complete { count: usize, too_large: bool },
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    bytes: &mut Vec<u8>,
    max_line_bytes: usize,
) -> std::io::Result<LineRead> {
    bytes.clear();
    let mut count: usize = 0;
    let mut too_large = false;

    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(if count == 0 {
                LineRead::Eof
            } else {
                LineRead::Partial { too_large }
            });
        }
        let chunk_len = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if !too_large {
            if bytes.len().saturating_add(chunk_len) <= max_line_bytes.saturating_add(1) {
                bytes.extend_from_slice(&available[..chunk_len]);
            } else {
                bytes.clear();
                too_large = true;
            }
        }
        let complete = available[chunk_len - 1] == b'\n';
        reader.consume(chunk_len);
        count = count.saturating_add(chunk_len);
        if complete {
            return Ok(LineRead::Complete { count, too_large });
        }
    }
}

fn push_line_too_large_diagnostic(
    path: &Path,
    line: u64,
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
        .with_source(path),
    );
}

fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if diagnostics.len() < MAX_DIAGNOSTICS_PER_BATCH {
        diagnostics.push(diagnostic);
    }
}

fn rollout_files(source: &CodexSource) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rollout_files(&source.codex_home().join("sessions"), &mut files)?;
    collect_rollout_files(&source.codex_home().join("archived_sessions"), &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rollout_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
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
            files.push(entry.path());
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

fn is_compressed_rollout(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".jsonl.zst"))
}

fn rollout_identity(source: &CodexSource, path: &Path) -> String {
    path.strip_prefix(source.codex_home())
        .unwrap_or(path)
        .to_string_lossy()
        .replace(['/', '\\'], ":")
}

fn whole_file_source(path: PathBuf) -> SourceRef {
    SourceRef {
        provider: ProviderId::new(PROVIDER_ID),
        path,
        location: SourceLocation::WholeFile,
    }
}

fn file_signature(path: &Path) -> Result<FileSignature> {
    let metadata =
        fs::metadata(path).map_err(|error| Error::io("inspect a Codex rollout", path, error))?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    Ok(FileSignature {
        len: metadata.len(),
        modified_nanos,
    })
}

fn tail_fingerprint(path: &Path, offset: u64) -> Result<u64> {
    if offset == 0 {
        return Ok(0);
    }
    let start = offset.saturating_sub(TAIL_FINGERPRINT_BYTES);
    let mut file =
        File::open(path).map_err(|error| Error::io("open a Codex rollout", path, error))?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| Error::io("seek in a Codex rollout", path, error))?;
    let mut bytes = vec![0; (offset - start) as usize];
    file.read_exact(&mut bytes)
        .map_err(|error| Error::io("read a Codex rollout fingerprint", path, error))?;
    Ok(fingerprint_bytes(&bytes))
}

fn fingerprint_json(value: &Value) -> u64 {
    fingerprint_bytes(value.to_string().as_bytes())
}

fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}
