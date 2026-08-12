use std::fs::File;
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::{Error, Result};

const TAIL_FINGERPRINT_BYTES: u64 = 4096;

pub(crate) enum LineRead {
    Eof,
    Partial { too_large: bool },
    Complete { count: usize, too_large: bool },
}

pub(crate) fn read_bounded_line(
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
            if !too_large && json_line_contents(bytes).len() > max_line_bytes {
                too_large = true;
                bytes.clear();
            }
            return Ok(if count == 0 {
                LineRead::Eof
            } else {
                LineRead::Partial { too_large }
            });
        }

        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        count = count.saturating_add(consumed);
        if !too_large && bytes.len().saturating_add(consumed) <= max_line_bytes.saturating_add(2) {
            bytes.extend_from_slice(&available[..consumed]);
        } else {
            too_large = true;
            bytes.clear();
        }
        let complete = available[consumed - 1] == b'\n';
        reader.consume(consumed);
        if complete {
            if !too_large && json_line_contents(bytes).len() > max_line_bytes {
                too_large = true;
                bytes.clear();
            }
            return Ok(LineRead::Complete { count, too_large });
        }
    }
}

pub(crate) fn is_complete_json_value(bytes: &[u8]) -> bool {
    let contents = json_line_contents(bytes);
    !contents.is_empty() && serde_json::from_slice::<Value>(contents).is_ok()
}

fn json_line_contents(bytes: &[u8]) -> &[u8] {
    let contents = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    contents.strip_suffix(b"\r").unwrap_or(contents)
}

pub(crate) fn tail_fingerprint(path: &Path, offset: u64, action: &'static str) -> Result<u64> {
    if offset == 0 {
        return Ok(0);
    }
    let start = offset.saturating_sub(TAIL_FINGERPRINT_BYTES);
    let mut file = File::open(path).map_err(|error| Error::io(action, path, error))?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| Error::io(action, path, error))?;
    let mut bytes = vec![0; (offset - start) as usize];
    file.read_exact(&mut bytes)
        .map_err(|error| Error::io(action, path, error))?;
    Ok(fingerprint_bytes(&bytes))
}

pub(crate) fn fingerprint_json(value: &Value) -> u64 {
    fingerprint_bytes(value.to_string().as_bytes())
}

pub(crate) fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{is_complete_json_value, read_bounded_line, LineRead};

    #[test]
    fn oversized_lines_are_discarded_without_losing_the_next_line() {
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
    fn complete_json_is_recognized_without_a_trailing_newline() {
        assert!(is_complete_json_value(br#"{"type":"event"}"#));
        assert!(is_complete_json_value(b"{\"type\":\"event\"}\r"));
        assert!(!is_complete_json_value(br#"{"type":"event""#));
        assert!(!is_complete_json_value(b""));
    }

    #[test]
    fn line_limit_excludes_lf_and_crlf_endings() {
        for line in [b"1234\n".as_slice(), b"1234\r\n".as_slice()] {
            let mut reader = Cursor::new(line);
            let mut bytes = Vec::new();

            let result = read_bounded_line(&mut reader, &mut bytes, 4).unwrap();

            assert!(matches!(
                result,
                LineRead::Complete {
                    too_large: false,
                    ..
                }
            ));
        }
    }
}
