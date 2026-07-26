use std::fs;
use std::io::Cursor;

use super::{read_bounded_line, scan, unix_millis, LineRead, ScanLimits};
use crate::providers::codex::CodexSource;

#[test]
fn bounded_line_reader_discards_oversized_contents_and_continues() {
    let mut reader = Cursor::new(b"123456\nok\n");
    let mut bytes = Vec::new();

    let first = read_bounded_line(&mut reader, &mut bytes, 4).unwrap();
    assert!(matches!(
        first,
        LineRead::Complete {
            count: 7,
            too_large: true
        }
    ));
    assert!(bytes.is_empty());

    let second = read_bounded_line(&mut reader, &mut bytes, 4).unwrap();
    assert!(matches!(
        second,
        LineRead::Complete {
            count: 3,
            too_large: false
        }
    ));
    assert_eq!(bytes, b"ok\n");
}

#[test]
fn unix_timestamp_normalization_accepts_seconds_and_milliseconds() {
    assert_eq!(unix_millis(1_700_000_000), 1_700_000_000_000);
    assert_eq!(unix_millis(1_700_000_000_123), 1_700_000_000_123);
}

#[test]
fn malformed_rollout_lines_still_respect_the_batch_limit() {
    let directory = tempfile::tempdir().unwrap();
    let codex_home = directory.path().join(".codex");
    let sessions = codex_home.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("rollout.jsonl"),
        "invalid\ninvalid\ninvalid\n",
    )
    .unwrap();
    let source = CodexSource::from_paths(&codex_home, &codex_home);
    let limits = ScanLimits {
        max_records_per_batch: 2,
        max_line_bytes: 1024,
    };

    let first = scan(&source, &limits, None).unwrap();
    assert!(first.has_more);

    let second = scan(&source, &limits, Some(&first.checkpoint)).unwrap();
    assert!(!second.has_more);
}

#[test]
fn compressed_rollout_parses_a_final_line_without_a_trailing_newline() {
    let directory = tempfile::tempdir().unwrap();
    let codex_home = directory.path().join(".codex");
    let sessions = codex_home.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let contents = b"{\"type\":\"compressed_event\"}";
    let compressed = zstd::stream::encode_all(&contents[..], 1).unwrap();
    fs::write(sessions.join("rollout.jsonl.zst"), compressed).unwrap();
    let source = CodexSource::from_paths(&codex_home, &codex_home);

    let batch = scan(&source, &ScanLimits::default(), None).unwrap();

    assert!(batch.changes.iter().any(|change| {
        matches!(
            change,
            crate::DataChange::Upsert { record }
                if record.payload["type"] == "compressed_event"
        )
    }));
    assert!(!batch.has_more);
}
