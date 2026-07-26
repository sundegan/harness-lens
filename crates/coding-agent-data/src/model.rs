use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordKey(String);

impl RecordKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    Session,
    Event,
    Artifact,
    Usage,
    Goal,
    Memory,
    Diagnostic,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DataTimestamp {
    UnixMilliseconds(i64),
    Rfc3339(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocation {
    SqliteRow {
        database: String,
        table: String,
        key: String,
    },
    JsonLine {
        line: u64,
        byte_start: Option<u64>,
        byte_end: Option<u64>,
    },
    WholeFile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub provider: ProviderId,
    pub path: PathBuf,
    pub location: SourceLocation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataRecord {
    pub key: RecordKey,
    pub kind: DataKind,
    pub timestamp: Option<DataTimestamp>,
    pub source: SourceRef,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataChange {
    Upsert { record: DataRecord },
    Delete { key: RecordKey },
    ResetSource { source: SourceRef },
    RemoveSource { source: SourceRef },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub source: Option<PathBuf>,
}

impl Diagnostic {
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: code.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<PathBuf>) -> Self {
        self.source = Some(source.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    provider: ProviderId,
    state: Value,
}

impl Checkpoint {
    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|error| Error::InvalidCheckpoint(error.to_string()))
    }

    pub fn from_json(value: &str) -> Result<Self> {
        serde_json::from_str(value).map_err(|error| Error::InvalidCheckpoint(error.to_string()))
    }

    #[cfg(feature = "codex")]
    pub(crate) fn new(provider: ProviderId, state: Value) -> Self {
        Self { provider, state }
    }

    #[cfg(feature = "codex")]
    pub(crate) fn state(&self) -> &Value {
        &self.state
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChangeBatch {
    pub changes: Vec<DataChange>,
    pub checkpoint: Checkpoint,
    pub diagnostics: Vec<Diagnostic>,
    pub has_more: bool,
}
