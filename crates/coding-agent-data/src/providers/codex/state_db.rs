use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Number, Value};

use crate::providers::shared::jsonl::{fingerprint_json, tail_fingerprint};
use crate::{Change, Diagnostic, Error, ProviderInfo, RecordId, Result, Session, SourceRef};

use super::checkpoint::{CodexCheckpoint, FileSignature, StateDatabaseSignature, ThreadState};
use super::normalize;
use super::CodexSource;

pub(super) struct IndexSnapshot {
    pub sessions: BTreeMap<String, Session>,
    pub transcripts: BTreeMap<PathBuf, SessionBinding>,
    pub database_changed: bool,
}

#[derive(Clone)]
pub(super) struct SessionBinding {
    pub external_id: String,
    pub record: RecordId,
}

pub(super) fn scan(
    source: &CodexSource,
    info: &ProviderInfo,
    state: &mut CodexCheckpoint,
    changes: &mut Vec<Change>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<IndexSnapshot> {
    let path = source.state_database();
    if !path.is_file() {
        let database_changed = state.database.take().is_some();
        diagnostics.push(
            Diagnostic::warning(
                "codex.state_db.missing",
                "the Codex state database is unavailable; rollout files are still scanned",
            )
            .with_origin(SourceRef::whole_file(info.source.clone(), &path)),
        );
        return Ok(snapshot_from_checkpoint(
            info,
            &state.threads,
            database_changed,
        ));
    }
    let signature = database_signature(&path)?;
    if state.database.as_ref() == Some(&signature) {
        return Ok(snapshot_from_checkpoint(info, &state.threads, false));
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
    let Some(spawn_parents) = load_spawn_parents(&connection, info, &path, diagnostics) else {
        // Keep the last complete snapshot and its signature. Committing the
        // new signature here would suppress retries and permanently erase
        // relationships from unchanged thread rows.
        return Ok(snapshot_from_checkpoint(info, &state.threads, false));
    };

    let mut statement = match connection.prepare("SELECT * FROM threads ORDER BY id") {
        Ok(statement) => statement,
        Err(error) => {
            diagnostics.push(
                Diagnostic::warning(
                    "codex.state_db.threads_unavailable",
                    format!("the threads table could not be read: {error}"),
                )
                .with_origin(SourceRef::whole_file(info.source.clone(), &path)),
            );
            return Ok(snapshot_from_checkpoint(info, &state.threads, false));
        }
    };
    let columns: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let mut rows = statement
        .query([])
        .map_err(|error| Error::sqlite("query Codex threads", &path, error))?;
    let mut current = BTreeMap::new();
    let mut snapshot = IndexSnapshot {
        sessions: BTreeMap::new(),
        transcripts: BTreeMap::new(),
        database_changed: true,
    };
    let mut identities_complete = true;

    while let Some(row) = rows
        .next()
        .map_err(|error| Error::sqlite("read a Codex thread row", &path, error))?
    {
        let mut object = Map::with_capacity(columns.len());
        for (index, name) in columns.iter().enumerate() {
            let value = row
                .get_ref(index)
                .map(sqlite_value_to_json)
                .map_err(|error| Error::sqlite("decode a Codex thread row", &path, error))?;
            object.insert(name.clone(), value);
        }
        let value = Value::Object(object);
        let spawn_parent = value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| spawn_parents.get(id))
            .map(String::as_str);
        let Some((mut session, mut record)) =
            normalize::thread_session(source, info, &value, spawn_parent)
        else {
            identities_complete = false;
            diagnostics.push(
                Diagnostic::warning(
                    "codex.state_db.thread_without_id",
                    "a Codex thread row without a string id was skipped",
                )
                .with_origin(SourceRef::whole_file(info.source.clone(), &path)),
            );
            continue;
        };
        let indexed_session = session.clone();
        if let Some(rollout_session) = state.rollouts.values().find_map(|rollout| {
            rollout
                .context()
                .rollout_session
                .as_ref()
                .filter(|observed| observed.external_id == session.external_id)
        }) {
            session = normalize::merge_session_metadata(&session, rollout_session);
            record.data = crate::RecordData::Session(session.clone());
        }
        let fingerprint = thread_fingerprint(&value, spawn_parent);
        current.insert(
            session.external_id.clone(),
            ThreadState {
                fingerprint,
                transcript: indexed_session.transcript.clone(),
                // Persist the authoritative SQLite snapshot only. Rollout
                // enrichment already lives in the rollout context and is
                // merged into this scan's output above.
                session: Some(indexed_session),
            },
        );
        snapshot
            .sessions
            .insert(session.external_id.clone(), session.clone());
        if let Some(transcript) = session.transcript.clone() {
            snapshot.transcripts.insert(
                transcript,
                SessionBinding {
                    external_id: session.external_id.clone(),
                    record: record.id.clone(),
                },
            );
        } else if value
            .get("rollout_path")
            .and_then(Value::as_str)
            .is_some_and(|path| !path.is_empty())
        {
            diagnostics.push(
                Diagnostic::warning(
                    "codex.state_db.rollout_outside_source",
                    "a thread rollout path outside the configured Codex roots was ignored",
                )
                .with_origin(SourceRef::whole_file(info.source.clone(), &path)),
            );
        }
        if state
            .threads
            .get(&session.external_id)
            .is_none_or(|previous| previous.fingerprint != fingerprint)
        {
            changes.push(Change::upsert(record));
        }
    }

    if identities_complete {
        for removed in state.threads.keys().filter(|id| !current.contains_key(*id)) {
            let fallback = state.rollouts.values().find_map(|rollout| {
                rollout
                    .context()
                    .rollout_session
                    .as_ref()
                    .filter(|session| session.external_id == **removed)
            });
            if let Some(session) = fallback {
                changes.push(Change::upsert(crate::Record {
                    id: normalize::session_id(info, removed),
                    source: info.source.clone(),
                    session: None,
                    invocation: None,
                    timestamp: session.updated_at,
                    origin: crate::SourceRef {
                        source: info.source.clone(),
                        path: session.transcript.clone().unwrap_or_default(),
                        location: crate::SourceLocation::WholeFile,
                    },
                    data: crate::RecordData::Session(session.clone()),
                    original: None,
                }));
            } else {
                changes.push(Change::Delete(normalize::session_id(info, removed)));
            }
        }
    } else {
        diagnostics.push(
            Diagnostic::warning(
                "codex.state_db.deletions_suppressed",
                "thread deletions were not inferred because at least one row had no usable id",
            )
            .with_origin(SourceRef::whole_file(info.source.clone(), &path)),
        );
        for (external_id, previous) in &state.threads {
            if current.contains_key(external_id) {
                continue;
            }
            current.insert(external_id.clone(), previous.clone());
            snapshot.sessions.insert(
                external_id.clone(),
                previous
                    .session
                    .clone()
                    .unwrap_or_else(|| Session::new(external_id)),
            );
            if let Some(transcript) = previous.transcript.clone() {
                snapshot.transcripts.insert(
                    transcript,
                    SessionBinding {
                        external_id: external_id.clone(),
                        record: normalize::session_id(info, external_id),
                    },
                );
            }
        }
    }
    state.threads = current;
    state.database = Some(signature);
    Ok(snapshot)
}

fn load_spawn_parents(
    connection: &Connection,
    info: &ProviderInfo,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeMap<String, String>> {
    let mut statement = match connection.prepare(
        "SELECT child_thread_id, parent_thread_id FROM thread_spawn_edges ORDER BY child_thread_id",
    ) {
        Ok(statement) => statement,
        Err(error) if is_missing_spawn_edges_table(&error) => return Some(BTreeMap::new()),
        Err(error) => {
            diagnostics.push(
                Diagnostic::warning(
                    "codex.state_db.spawn_edges_unavailable",
                    format!("Codex thread relationships could not be read: {error}"),
                )
                .with_origin(SourceRef::whole_file(info.source.clone(), path)),
            );
            return None;
        }
    };
    let mut rows = match statement.query([]) {
        Ok(rows) => rows,
        Err(error) => {
            diagnostics.push(
                Diagnostic::warning(
                    "codex.state_db.spawn_edges_unavailable",
                    format!("Codex thread relationships could not be queried: {error}"),
                )
                .with_origin(SourceRef::whole_file(info.source.clone(), path)),
            );
            return None;
        }
    };
    let mut parents = BTreeMap::new();
    loop {
        let row = match rows.next() {
            Ok(Some(row)) => row,
            Ok(None) => break,
            Err(error) => {
                diagnostics.push(
                    Diagnostic::warning(
                        "codex.state_db.spawn_edges_incomplete",
                        format!("a Codex thread relationship could not be read: {error}"),
                    )
                    .with_origin(SourceRef::whole_file(info.source.clone(), path)),
                );
                return None;
            }
        };
        match (row.get::<_, String>(0), row.get::<_, String>(1)) {
            (Ok(child), Ok(parent)) if !child.is_empty() && !parent.is_empty() => {
                parents.insert(child, parent);
            }
            _ => {
                diagnostics.push(
                    Diagnostic::warning(
                        "codex.state_db.spawn_edge_invalid",
                        "a Codex thread relationship has an invalid identity",
                    )
                    .with_origin(SourceRef::whole_file(info.source.clone(), path)),
                );
                return None;
            }
        }
    }
    Some(parents)
}

fn is_missing_spawn_edges_table(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(_, Some(message))
            if message.trim() == "no such table: thread_spawn_edges"
    )
}

fn thread_fingerprint(value: &Value, spawn_parent: Option<&str>) -> u64 {
    let Some(spawn_parent) = spawn_parent else {
        return fingerprint_json(value);
    };
    let mut fingerprint = Map::new();
    fingerprint.insert("thread".to_owned(), value.clone());
    fingerprint.insert(
        "spawn_parent".to_owned(),
        Value::String(spawn_parent.to_owned()),
    );
    fingerprint_json(&Value::Object(fingerprint))
}

fn snapshot_from_checkpoint(
    info: &ProviderInfo,
    threads: &BTreeMap<String, ThreadState>,
    database_changed: bool,
) -> IndexSnapshot {
    IndexSnapshot {
        sessions: threads
            .iter()
            .map(|(external_id, state)| {
                (
                    external_id.clone(),
                    state
                        .session
                        .clone()
                        .unwrap_or_else(|| Session::new(external_id)),
                )
            })
            .collect(),
        transcripts: threads
            .iter()
            .filter_map(|(external_id, state)| {
                state.transcript.clone().map(|path| {
                    (
                        path,
                        SessionBinding {
                            external_id: external_id.clone(),
                            record: normalize::session_id(info, external_id),
                        },
                    )
                })
            })
            .collect(),
        database_changed,
    }
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

fn database_signature(path: &Path) -> Result<StateDatabaseSignature> {
    Ok(StateDatabaseSignature {
        database: file_signature(path)?,
        wal: optional_file_signature(&sidecar_path(path, "-wal"))?,
    })
}

fn optional_file_signature(path: &Path) -> Result<Option<FileSignature>> {
    match fs::metadata(path) {
        Ok(metadata) => match signature_from_metadata(path, metadata) {
            Ok(signature) => Ok(Some(signature)),
            Err(Error::Io { source, .. })
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::UnexpectedEof
                ) =>
            {
                // SQLite may checkpoint and replace the WAL between metadata
                // inspection and the fingerprint read. Treat that as an
                // absent sidecar so the next scan performs a full comparison.
                Ok(None)
            }
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io(
            "inspect a Codex state database sidecar",
            path,
            error,
        )),
    }
}

fn file_signature(path: &Path) -> Result<FileSignature> {
    let metadata = fs::metadata(path)
        .map_err(|error| Error::io("inspect the Codex state database", path, error))?;
    signature_from_metadata(path, metadata)
}

fn signature_from_metadata(path: &Path, metadata: fs::Metadata) -> Result<FileSignature> {
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    Ok(FileSignature {
        len: metadata.len(),
        modified_nanos,
        tail_fingerprint: tail_fingerprint(
            path,
            metadata.len(),
            "read a Codex state database signature",
        )?,
    })
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(suffix);
    PathBuf::from(value)
}
