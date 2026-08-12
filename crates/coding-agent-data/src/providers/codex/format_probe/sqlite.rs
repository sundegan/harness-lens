use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use crate::format_probe::{
    FormatFingerprint, FormatProbeOptions, FormatSourceFingerprint, SqliteColumn, SqliteColumnKind,
    SqliteFingerprint, SqliteIndex, SqliteIndexTerm, SqliteObject, SqliteObjectKind,
    FORMAT_FINGERPRINT_VERSION,
};
use crate::{Error, ProviderId, Result};

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

pub(super) fn is_sqlite(path: &Path) -> Result<bool> {
    let mut file =
        File::open(path).map_err(|error| Error::io("inspect format probe source", path, error))?;
    let mut header = [0_u8; SQLITE_HEADER.len()];
    match file.read_exact(&mut header) {
        Ok(()) => Ok(header == SQLITE_HEADER),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(Error::io("inspect format probe source", path, error)),
    }
}

pub(super) fn probe_path(path: &Path, options: FormatProbeOptions) -> Result<FormatFingerprint> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| Error::sqlite("open format probe source read-only", path, error))?;

    let mut statement = connection
        .prepare(
            "SELECT name, type, sql
             FROM sqlite_master
             WHERE type IN ('table', 'view')
               AND name NOT GLOB 'sqlite_*'
             ORDER BY name",
        )
        .map_err(|error| Error::sqlite("inspect SQLite schema objects", path, error))?;
    let objects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| Error::sqlite("inspect SQLite schema objects", path, error))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| Error::sqlite("inspect SQLite schema objects", path, error))?;
    drop(statement);

    let mut fingerprint = SqliteFingerprint::default();
    for (name, kind, definition) in objects {
        let columns = columns(&connection, path, &name)?;
        let indexes = if kind == "table" {
            indexes(&connection, path, &name)?
        } else {
            BTreeMap::new()
        };
        fingerprint.objects.insert(
            name,
            SqliteObject {
                kind: if kind == "view" {
                    SqliteObjectKind::View
                } else {
                    SqliteObjectKind::Table
                },
                definition_digest: definition.as_deref().map(sql_digest),
                columns,
                indexes,
            },
        );
    }

    Ok(FormatFingerprint {
        manifest_version: FORMAT_FINGERPRINT_VERSION,
        provider: ProviderId::new(super::super::PROVIDER_ID),
        options,
        source: FormatSourceFingerprint::Sqlite(fingerprint),
    })
}

fn columns(connection: &Connection, path: &Path, object: &str) -> Result<Vec<SqliteColumn>> {
    let mut statement = connection
        .prepare(
            "SELECT name, type, \"notnull\", pk, hidden
             FROM pragma_table_xinfo(?1)
             ORDER BY cid",
        )
        .map_err(|error| Error::sqlite("inspect SQLite columns", path, error))?;
    let columns = statement
        .query_map([object], |row| {
            Ok(SqliteColumn {
                name: row.get(0)?,
                declared_type: row.get(1)?,
                not_null: row.get::<_, i64>(2)? != 0,
                primary_key_position: row.get(3)?,
                kind: column_kind(row.get(4)?),
            })
        })
        .map_err(|error| Error::sqlite("inspect SQLite columns", path, error))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| Error::sqlite("inspect SQLite columns", path, error))?;
    Ok(columns)
}

fn column_kind(hidden: i64) -> SqliteColumnKind {
    match hidden {
        0 => SqliteColumnKind::Normal,
        1 => SqliteColumnKind::Hidden,
        2 => SqliteColumnKind::GeneratedVirtual,
        3 => SqliteColumnKind::GeneratedStored,
        value => SqliteColumnKind::Unknown(value),
    }
}

fn sql_digest(sql: &str) -> String {
    format!("{:x}", Sha256::digest(sql.as_bytes()))
}

fn indexes(
    connection: &Connection,
    path: &Path,
    table: &str,
) -> Result<BTreeMap<String, SqliteIndex>> {
    let mut statement = connection
        .prepare(
            "SELECT indexes.name, indexes.\"unique\", indexes.partial, schema.sql
             FROM pragma_index_list(?1) AS indexes
             LEFT JOIN sqlite_master AS schema
               ON schema.type = 'index' AND schema.name = indexes.name
             WHERE indexes.origin != 'pk'
             ORDER BY indexes.name",
        )
        .map_err(|error| Error::sqlite("inspect SQLite indexes", path, error))?;
    let indexes = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| Error::sqlite("inspect SQLite indexes", path, error))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| Error::sqlite("inspect SQLite indexes", path, error))?;
    drop(statement);

    let mut result = BTreeMap::new();
    for (name, unique, partial, definition) in indexes {
        let mut terms_statement = connection
            .prepare(
                "SELECT cid, name, \"desc\", coll
                 FROM pragma_index_xinfo(?1)
                 WHERE key = 1
                 ORDER BY seqno",
            )
            .map_err(|error| Error::sqlite("inspect SQLite index terms", path, error))?;
        let terms = terms_statement
            .query_map([&name], |row| {
                Ok(SqliteIndexTerm {
                    column_id: row.get(0)?,
                    column_name: row.get(1)?,
                    descending: row.get::<_, i64>(2)? != 0,
                    collation: row.get(3)?,
                })
            })
            .map_err(|error| Error::sqlite("inspect SQLite index terms", path, error))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| Error::sqlite("inspect SQLite index terms", path, error))?;
        result.insert(
            name,
            SqliteIndex {
                unique,
                partial,
                definition_digest: definition.as_deref().map(sql_digest),
                terms,
            },
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use super::{is_sqlite, probe_path};
    use crate::format_probe::{FormatProbeOptions, FormatSourceFingerprint, SqliteColumnKind};

    #[test]
    fn detection_treats_short_files_as_non_sqlite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("artifact.bin");
        fs::write(&path, b"short").unwrap();

        assert!(!is_sqlite(&path).unwrap());
    }

    #[test]
    fn probe_reads_schema_without_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "
                    CREATE TABLE threads (
                        id TEXT PRIMARY KEY,
                        title TEXT NOT NULL,
                        tokens_used INTEGER
                    );
                    CREATE UNIQUE INDEX threads_title_idx ON threads(title);
                    INSERT INTO threads VALUES ('secret-id', 'secret-title', 42);
                    ",
                )
                .unwrap();
        }

        let fingerprint = probe_path(&path, FormatProbeOptions::default()).unwrap();
        let FormatSourceFingerprint::Sqlite(observations) = fingerprint.source else {
            panic!("expected SQLite observations");
        };
        let table = &observations.objects["threads"];
        assert_eq!(table.columns[0].name, "id");
        assert_eq!(table.columns[1].declared_type, "TEXT");
        assert!(table.indexes["threads_title_idx"].unique);
        let serialized = serde_json::to_string(&observations).unwrap();
        assert!(!serialized.contains("secret-id"));
        assert!(!serialized.contains("secret-title"));
    }

    #[test]
    fn probe_keeps_non_reserved_names_that_begin_with_sqlite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "
                CREATE TABLE sqliteX (id INTEGER);
                CREATE TABLE entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT
                );
                ",
            )
            .unwrap();

        let fingerprint = probe_path(&path, FormatProbeOptions::default()).unwrap();
        let FormatSourceFingerprint::Sqlite(observations) = fingerprint.source else {
            panic!("expected SQLite observations");
        };

        assert!(observations.objects.contains_key("sqliteX"));
        assert!(!observations.objects.contains_key("sqlite_sequence"));
    }

    #[test]
    fn probe_handles_generated_columns_and_expression_indexes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "
                    CREATE TABLE metrics (
                        value TEXT,
                        normalized TEXT GENERATED ALWAYS AS (lower(value)) STORED
                    );
                    CREATE INDEX metrics_value_idx
                    ON metrics(lower(value) DESC)
                    WHERE value IS NOT NULL;
                    ",
                )
                .unwrap();
        }

        let fingerprint = probe_path(&path, FormatProbeOptions::default()).unwrap();
        let FormatSourceFingerprint::Sqlite(observations) = fingerprint.source else {
            panic!("expected SQLite observations");
        };
        let table = &observations.objects["metrics"];
        assert_eq!(
            table
                .columns
                .iter()
                .find(|column| column.name == "normalized")
                .unwrap()
                .kind,
            SqliteColumnKind::GeneratedStored
        );
        let index = &table.indexes["metrics_value_idx"];
        assert!(index.partial);
        assert_eq!(index.terms[0].column_id, -2);
        assert!(index.terms[0].descending);
    }

    #[test]
    fn diff_detects_definition_changes_with_the_same_surface_shape() {
        let directory = tempfile::tempdir().unwrap();
        let baseline_path = directory.path().join("baseline.sqlite");
        let current_path = directory.path().join("current.sqlite");
        Connection::open(&baseline_path)
            .unwrap()
            .execute_batch(
                "
                CREATE TABLE entries (value TEXT);
                CREATE INDEX entries_value_idx ON entries(lower(value))
                WHERE value IS NOT NULL;
                ",
            )
            .unwrap();
        Connection::open(&current_path)
            .unwrap()
            .execute_batch(
                "
                CREATE TABLE entries (value TEXT);
                CREATE INDEX entries_value_idx ON entries(upper(value))
                WHERE value != '';
                ",
            )
            .unwrap();

        let baseline = probe_path(&baseline_path, FormatProbeOptions::default()).unwrap();
        let current = probe_path(&current_path, FormatProbeOptions::default()).unwrap();

        assert!(baseline.diff(&current).changes.iter().any(|change| {
            change
                .path
                .ends_with("/indexes/entries_value_idx/definition_digest")
        }));
    }
}
