#![cfg(feature = "codex")]

use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(feature = "watch")]
use std::time::Duration;

use coding_agent_data::providers::codex::{CodexProvider, CodexSource};
use coding_agent_data::{AgentDataProvider, DataChange, DataKind};
#[cfg(feature = "watch")]
use coding_agent_data::{WatchOptions, WatchableAgentDataProvider};
use rusqlite::{params, Connection};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    source: CodexSource,
    database_path: std::path::PathBuf,
    rollout_path: std::path::PathBuf,
}

impl Fixture {
    fn new(rollout_contents: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let codex_home = directory.path().join(".codex");
        let rollout_directory = codex_home.join("sessions/2026/01/02");
        fs::create_dir_all(&rollout_directory).unwrap();
        let rollout_path = rollout_directory.join("rollout-2026-01-02T03-04-05-thread-1.jsonl");
        fs::write(&rollout_path, rollout_contents).unwrap();

        let database_path = codex_home.join("state_5.sqlite");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    rollout_path TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    title TEXT NOT NULL
                );
                ",
            )
            .unwrap();
        connection
            .execute(
                "
                INSERT INTO threads (id, rollout_path, updated_at_ms, title)
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![
                    "thread-1",
                    rollout_path.to_string_lossy(),
                    1_700_000_000_000_i64,
                    "Initial title"
                ],
            )
            .unwrap();
        drop(connection);

        Self {
            source: CodexSource::from_paths(&codex_home, &codex_home),
            _directory: directory,
            database_path,
            rollout_path,
        }
    }

    fn provider(&self) -> CodexProvider {
        CodexProvider::new(self.source.clone())
    }
}

#[test]
fn snapshot_and_incremental_scan_only_emit_complete_new_lines() {
    let fixture = Fixture::new(concat!(
        "{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\"}\n",
        "{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"event_msg\"}\n",
        "{\"timestamp\":\"2026-01-02T03:04:07Z\",\"type\":\"partial\"}"
    ));
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    let first_records: Vec<_> = first
        .changes
        .iter()
        .filter_map(|change| match change {
            DataChange::Upsert { record } => Some(record),
            _ => None,
        })
        .collect();
    assert_eq!(first_records.len(), 3);
    assert_eq!(
        first_records
            .iter()
            .filter(|record| record.kind == DataKind::Session)
            .count(),
        1
    );
    assert_eq!(
        first_records
            .iter()
            .filter(|record| record.kind == DataKind::Event)
            .count(),
        2
    );

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    let second_event_types: Vec<_> = second
        .changes
        .iter()
        .filter_map(|change| match change {
            DataChange::Upsert { record } if record.kind == DataKind::Event => {
                record.payload.get("type").and_then(|value| value.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(second_event_types, vec!["partial"]);

    let third = provider.scan(Some(&second.checkpoint)).unwrap();
    assert!(third.changes.is_empty());
    assert!(!third.has_more);
}

#[test]
fn metadata_changes_are_detected_without_updated_at_changing() {
    let fixture =
        Fixture::new("{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\"}\n");
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    Connection::open(&fixture.database_path)
        .unwrap()
        .execute(
            "UPDATE threads SET title = 'Renamed' WHERE id = 'thread-1'",
            [],
        )
        .unwrap();
    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    let records: Vec<_> = second
        .changes
        .iter()
        .filter_map(|change| match change {
            DataChange::Upsert { record } => Some(record),
            _ => None,
        })
        .collect();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].kind, DataKind::Session);
    assert_eq!(records[0].payload["title"], "Renamed");
}

#[test]
fn checkpoint_json_round_trip_preserves_incremental_position() {
    let fixture =
        Fixture::new("{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\"}\n");
    let provider = fixture.provider();
    let first = provider.scan(None).unwrap();

    let encoded = first.checkpoint.to_json().unwrap();
    let decoded = coding_agent_data::Checkpoint::from_json(&encoded).unwrap();
    let second = provider.scan(Some(&decoded)).unwrap();

    assert!(second.changes.is_empty());
}

#[test]
fn compressed_rollouts_are_parsed_once() {
    let fixture = Fixture::new("");
    fs::remove_file(&fixture.rollout_path).unwrap();
    let compressed_path = fixture.rollout_path.with_extension("jsonl.zst");
    let contents = b"{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"compressed_event\"}\n";
    let compressed = zstd::stream::encode_all(&contents[..], 1).unwrap();
    fs::write(&compressed_path, compressed).unwrap();
    let provider = fixture.provider();

    let first = provider.scan(None).unwrap();
    assert!(first.changes.iter().any(|change| {
        matches!(
            change,
            DataChange::Upsert { record }
                if record.payload["type"] == "compressed_event"
        )
    }));

    let second = provider.scan(Some(&first.checkpoint)).unwrap();
    assert!(second.changes.is_empty());
}

#[test]
#[cfg(feature = "watch")]
fn watcher_emits_incremental_changes_with_periodic_reconciliation() {
    let fixture =
        Fixture::new("{\"timestamp\":\"2026-01-02T03:04:05Z\",\"type\":\"session_meta\"}\n");
    let provider = fixture.provider();
    let initial = provider.scan(None).unwrap();
    let watcher = provider
        .watch(
            initial.checkpoint,
            WatchOptions {
                debounce: Duration::from_millis(20),
                reconcile_interval: Duration::from_millis(50),
            },
        )
        .unwrap();

    OpenOptions::new()
        .append(true)
        .open(&fixture.rollout_path)
        .unwrap()
        .write_all(b"{\"timestamp\":\"2026-01-02T03:04:06Z\",\"type\":\"live_event\"}\n")
        .unwrap();

    let batch = watcher
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .expect("the watcher should emit a change batch");
    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            DataChange::Upsert { record } if record.payload["type"] == "live_event"
        )
    }));
}
