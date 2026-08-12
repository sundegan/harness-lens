//! Provider-neutral contracts for structural format inspection.
//!
//! Provider adapters own artifact discovery, source-format interpretation, and
//! private-schema rules. This module defines only the public probe interface,
//! deterministic fingerprints, and structural differences consumed by tools.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProviderId, Result};

/// Current version of the serialized format-fingerprint representation.
pub const FORMAT_FINGERPRINT_VERSION: u32 = 2;

/// How a current format fingerprint is checked against an approved baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatComparisonMode {
    /// Every observed structural detail must match the baseline.
    ///
    /// This mode is suitable for complete schemas such as SQLite databases.
    Exact,
    /// The current artifact may contain any subset of the approved structure.
    ///
    /// This mode is suitable for JSONL artifacts where one Session usually
    /// exercises only part of the Provider's complete event vocabulary. New
    /// paths, value kinds, discriminator values, or probe-policy markers are
    /// reported, while approved details absent from the current sample are not.
    AllowedStructure,
}

/// Limits applied while inspecting one provider artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatProbeOptions {
    /// Maximum number of bytes allowed before a JSONL line ending.
    pub max_line_bytes: usize,
    /// Maximum JSON nesting depth included in the fingerprint.
    pub max_depth: usize,
    /// Maximum number of individual diagnostics retained in the fingerprint.
    pub max_diagnostics: usize,
}

impl Default for FormatProbeOptions {
    fn default() -> Self {
        Self {
            max_line_bytes: crate::DEFAULT_MAX_JSON_LINE_BYTES,
            max_depth: 32,
            max_diagnostics: 100,
        }
    }
}

/// Provider artifact format identified by a format probe.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbedFormat {
    /// Plain newline-delimited JSON.
    JsonLines,
    /// Zstandard-compressed newline-delimited JSON.
    JsonLinesZstd,
    /// SQLite database.
    Sqlite,
}

/// Provider-neutral interface for inspecting one provider artifact.
///
/// Implementations belong to individual providers because they interpret
/// provider-private paths, formats, discriminator fields, and opaque payloads.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
///
/// use coding_agent_data::format_probe::{
///     FormatBaseline, FormatProbe, FormatProbeOptions,
/// };
/// use coding_agent_data::providers::codex::CodexFormatProbe;
///
/// fn format_is_compatible(
///     baseline: &FormatBaseline,
///     artifact: &Path,
/// ) -> coding_agent_data::Result<bool> {
///     let current =
///         CodexFormatProbe.probe_path(artifact, FormatProbeOptions::default())?;
///     Ok(baseline.check(&current).is_compatible())
/// }
/// ```
pub trait FormatProbe {
    /// Inspects one artifact using the implementing provider's format rules.
    fn probe_path(&self, path: &Path, options: FormatProbeOptions) -> Result<FormatFingerprint>;
}

/// One provider artifact selected for convenient local format inspection.
///
/// Discovery returns the local path as probe input, but [`FormatFingerprint`]
/// never embeds that path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredFormatArtifact {
    /// Stable, human-readable role such as `state_database` or `latest_rollout`.
    pub label: String,
    /// Provider-local artifact path.
    pub path: PathBuf,
}

impl DiscoveredFormatArtifact {
    /// Creates a discovered artifact.
    pub fn new(label: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            label: label.into(),
            path: path.into(),
        }
    }
}

/// Provider-specific discovery of representative local format artifacts.
///
/// Implementations own their private directory layout and choose a bounded set
/// of current artifacts suitable for a quick local probe.
pub trait FormatProbeDiscovery: FormatProbe {
    /// Discovers representative artifacts from the provider's default source.
    fn discover_artifacts(&self) -> Result<Vec<DiscoveredFormatArtifact>>;
}

/// Deterministic structural description of one provider artifact.
///
/// The fingerprint contains no inspected path, database row, or general JSON
/// payload scalar. Field names, schema names, and bounded provider-selected
/// discriminator values are retained because they describe the observed
/// format.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatFingerprint {
    /// Version of this crate's fingerprint representation.
    ///
    /// Provider implementations should use [`FORMAT_FINGERPRINT_VERSION`].
    pub manifest_version: u32,
    /// Provider whose artifact produced this fingerprint.
    pub provider: ProviderId,
    /// Limits used while observing the artifact.
    pub options: FormatProbeOptions,
    /// Provider-specific source format and its provider-neutral observations.
    #[serde(flatten)]
    pub source: FormatSourceFingerprint,
}

impl FormatFingerprint {
    /// Compares this baseline fingerprint with a newly observed fingerprint.
    pub fn diff(&self, current: &Self) -> FormatDiff {
        FormatDiff::between(self, current)
    }

    /// Compares this fingerprint with a current observation using `mode`.
    pub fn diff_with_mode(&self, current: &Self, mode: FormatComparisonMode) -> FormatDiff {
        FormatDiff::between_with_mode(self, current, mode)
    }

    /// Checks structural drift and unreadable records in one operation.
    pub fn check_compatibility(
        &self,
        current: &Self,
        mode: FormatComparisonMode,
    ) -> FormatCompatibilityReport {
        FormatCompatibilityReport {
            comparison_mode: mode,
            diff: self.diff_with_mode(current, mode),
            unreadable_records: current.has_unreadable_records(),
        }
    }

    /// Returns whether the artifact contained records the probe could not read.
    ///
    /// A trailing JSONL record that is merely incomplete is not treated as
    /// unreadable because an active Provider may still be appending it.
    pub fn has_unreadable_records(&self) -> bool {
        match &self.source {
            FormatSourceFingerprint::JsonLines(observations)
            | FormatSourceFingerprint::JsonLinesZstd(observations) => {
                observations.malformed_records > 0
                    || observations.oversized_records > 0
                    || !observations.diagnostics.is_empty()
                    || observations.diagnostics_truncated > 0
            }
            FormatSourceFingerprint::Sqlite(_) => false,
        }
    }

    fn comparable_value(&self) -> Value {
        let source = match &self.source {
            FormatSourceFingerprint::JsonLines(observations) => serde_json::json!({
                "format": "json_lines",
                "options": {
                    "max_line_bytes": self.options.max_line_bytes,
                    "max_depth": self.options.max_depth,
                },
                "fields": observations.fields,
                "discriminators": observations.discriminators,
                "depth_limited_paths": observations.depth_limited_paths,
                "dynamic_map_paths": observations.dynamic_map_paths,
                "opaque_paths": observations.opaque_paths,
            }),
            FormatSourceFingerprint::JsonLinesZstd(observations) => serde_json::json!({
                "format": "json_lines_zstd",
                "options": {
                    "max_line_bytes": self.options.max_line_bytes,
                    "max_depth": self.options.max_depth,
                },
                "fields": observations.fields,
                "discriminators": observations.discriminators,
                "depth_limited_paths": observations.depth_limited_paths,
                "dynamic_map_paths": observations.dynamic_map_paths,
                "opaque_paths": observations.opaque_paths,
            }),
            FormatSourceFingerprint::Sqlite(observations) => serde_json::json!({
                "format": "sqlite",
                "objects": observations.objects,
            }),
        };
        serde_json::json!({
            "manifest_version": self.manifest_version,
            "provider": self.provider,
            "source": source,
        })
    }

    fn without_runtime_observations(&self) -> Self {
        let mut fingerprint = self.clone();
        match &mut fingerprint.source {
            FormatSourceFingerprint::JsonLines(observations)
            | FormatSourceFingerprint::JsonLinesZstd(observations) => {
                observations.records_seen = 0;
                observations.valid_records = 0;
                observations.malformed_records = 0;
                observations.incomplete_records = 0;
                observations.oversized_records = 0;
                observations.diagnostics.clear();
                observations.diagnostics_truncated = 0;
            }
            FormatSourceFingerprint::Sqlite(_) => {}
        }
        fingerprint
    }
}

/// Approved structural contract used to check later Provider artifacts.
///
/// A baseline stores only deterministic structure. Runtime record counts and
/// diagnostics are removed when it is created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatBaseline {
    /// Comparison semantics appropriate for this artifact.
    pub comparison_mode: FormatComparisonMode,
    /// Approved structural fingerprint.
    pub fingerprint: FormatFingerprint,
}

impl FormatBaseline {
    /// Creates an approved baseline from an inspected artifact.
    pub fn new(comparison_mode: FormatComparisonMode, fingerprint: &FormatFingerprint) -> Self {
        let mut fingerprint = fingerprint.without_runtime_observations();
        if comparison_mode == FormatComparisonMode::AllowedStructure {
            if let FormatSourceFingerprint::JsonLinesZstd(observations) = &fingerprint.source {
                fingerprint.source = FormatSourceFingerprint::JsonLines(observations.clone());
            }
        }
        Self {
            comparison_mode,
            fingerprint,
        }
    }

    /// Checks a current fingerprint against this baseline.
    pub fn check(&self, current: &FormatFingerprint) -> FormatCompatibilityReport {
        self.fingerprint
            .check_compatibility(current, self.comparison_mode)
    }
}

/// Format-specific observations carried by a [`FormatFingerprint`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "format", content = "observations", rename_all = "snake_case")]
pub enum FormatSourceFingerprint {
    /// Plain JSONL observations.
    JsonLines(JsonLinesFingerprint),
    /// Zstandard-compressed JSONL observations.
    JsonLinesZstd(JsonLinesFingerprint),
    /// SQLite schema observations.
    Sqlite(SqliteFingerprint),
}

impl FormatSourceFingerprint {
    /// Returns the detected artifact format.
    pub const fn format(&self) -> ProbedFormat {
        match self {
            Self::JsonLines(_) => ProbedFormat::JsonLines,
            Self::JsonLinesZstd(_) => ProbedFormat::JsonLinesZstd,
            Self::Sqlite(_) => ProbedFormat::Sqlite,
        }
    }
}

/// Structural observations collected from a JSONL artifact.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonLinesFingerprint {
    /// Number of non-empty physical records inspected.
    pub records_seen: u64,
    /// Number of records successfully parsed as JSON.
    pub valid_records: u64,
    /// Number of line-terminated records that were not valid JSON values.
    pub malformed_records: u64,
    /// Number of trailing records that were incomplete when inspected.
    #[serde(default)]
    pub incomplete_records: u64,
    /// Number of records exceeding [`FormatProbeOptions::max_line_bytes`].
    pub oversized_records: u64,
    /// JSON paths and all value kinds observed at each path.
    pub fields: BTreeMap<String, BTreeSet<JsonValueKind>>,
    /// Enum-like values retained by the provider's discriminator policy.
    pub discriminators: BTreeMap<String, BTreeSet<String>>,
    /// Paths whose children were not inspected because the depth limit was reached.
    pub depth_limited_paths: BTreeSet<String>,
    /// Paths whose object keys were replaced by a wildcard.
    pub dynamic_map_paths: BTreeSet<String>,
    /// Paths whose nested structure was intentionally not inspected.
    pub opaque_paths: BTreeSet<String>,
    /// Bounded structural diagnostics.
    pub diagnostics: Vec<FormatProbeDiagnostic>,
    /// Number of diagnostics omitted after reaching the configured limit.
    pub diagnostics_truncated: u64,
}

/// JSON value category used in structural fingerprints.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonValueKind {
    /// JSON null.
    Null,
    /// JSON boolean.
    Boolean,
    /// Signed or unsigned integer representable by `serde_json`.
    Integer,
    /// Non-integer JSON number.
    Float,
    /// JSON string. The string contents are not retained.
    String,
    /// JSON array.
    Array,
    /// JSON object.
    Object,
}

/// A privacy-safe problem found while probing one artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatProbeDiagnostic {
    /// Stable machine-readable diagnostic code.
    pub code: String,
    /// One-based JSONL record number, when applicable.
    pub record: Option<u64>,
    /// Explanation that does not contain source contents.
    pub message: String,
}

/// SQLite schema observed without reading database rows.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteFingerprint {
    /// User-defined tables and views keyed by schema object name.
    pub objects: BTreeMap<String, SqliteObject>,
}

/// One SQLite table or view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteObject {
    /// Schema object kind.
    pub kind: SqliteObjectKind,
    /// SHA-256 digest of SQLite's stored DDL, when available.
    pub definition_digest: Option<String>,
    /// Declared columns in ordinal order.
    pub columns: Vec<SqliteColumn>,
    /// Observed indexes keyed by SQLite index name.
    pub indexes: BTreeMap<String, SqliteIndex>,
}

/// SQLite schema object kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqliteObjectKind {
    /// SQLite table.
    Table,
    /// SQLite view.
    View,
}

/// Declared SQLite column metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteColumn {
    /// Column name.
    pub name: String,
    /// Provider-declared SQLite type.
    pub declared_type: String,
    /// Whether the schema declares the column `NOT NULL`.
    pub not_null: bool,
    /// One-based position inside a composite primary key, or zero otherwise.
    pub primary_key_position: i64,
    /// Whether this is a normal, hidden, or generated column.
    pub kind: SqliteColumnKind,
}

/// How SQLite exposes a declared column through `pragma_table_xinfo`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqliteColumnKind {
    /// Ordinary table or view column.
    Normal,
    /// Hidden virtual-table column.
    Hidden,
    /// Virtual generated column.
    GeneratedVirtual,
    /// Stored generated column.
    GeneratedStored,
    /// Column kind introduced by a newer SQLite version.
    Unknown(i64),
}

/// Declared SQLite index metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteIndex {
    /// Whether the index enforces uniqueness.
    pub unique: bool,
    /// Whether the index has a `WHERE` predicate.
    pub partial: bool,
    /// SHA-256 digest of SQLite's stored index DDL, when available.
    pub definition_digest: Option<String>,
    /// Indexed columns or expressions in index order.
    pub terms: Vec<SqliteIndexTerm>,
}

/// One key term in a SQLite index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SqliteIndexTerm {
    /// SQLite column ID, `-1` for `rowid`, or `-2` for an expression.
    pub column_id: i64,
    /// Column name, or `None` for `rowid` and expression terms.
    pub column_name: Option<String>,
    /// Whether the term uses descending order.
    pub descending: bool,
    /// SQLite collation assigned to the term, when reported.
    pub collation: Option<String>,
}

/// Difference between a stored format baseline and a current fingerprint.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatDiff {
    /// Ordered structural changes.
    pub changes: Vec<FormatChange>,
}

impl FormatDiff {
    /// Creates a deterministic structural difference.
    pub fn between(baseline: &FormatFingerprint, current: &FormatFingerprint) -> Self {
        Self::between_with_mode(baseline, current, FormatComparisonMode::Exact)
    }

    /// Creates a deterministic difference with the requested comparison semantics.
    pub fn between_with_mode(
        baseline: &FormatFingerprint,
        current: &FormatFingerprint,
        mode: FormatComparisonMode,
    ) -> Self {
        let mut changes = Vec::new();
        match mode {
            FormatComparisonMode::Exact => diff_values(
                "",
                &baseline.comparable_value(),
                &current.comparable_value(),
                &mut changes,
            ),
            FormatComparisonMode::AllowedStructure => {
                diff_allowed_structure(baseline, current, &mut changes);
            }
        }
        Self { changes }
    }

    /// Returns whether both fingerprints describe the same known structure.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Result of checking one current artifact against an approved baseline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatCompatibilityReport {
    /// Comparison semantics used for this check.
    pub comparison_mode: FormatComparisonMode,
    /// Structural differences that violate the baseline.
    pub diff: FormatDiff,
    /// Whether the probe found malformed, oversized, or otherwise unreadable records.
    pub unreadable_records: bool,
}

impl FormatCompatibilityReport {
    /// Returns whether the current artifact is structurally approved and readable.
    pub fn is_compatible(&self) -> bool {
        self.diff.is_empty() && !self.unreadable_records
    }
}

/// One added, removed, or changed structural value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatChange {
    /// Kind of structural change.
    pub kind: FormatChangeKind,
    /// JSON Pointer path inside the comparable fingerprint representation.
    pub path: String,
    /// Previous structural value, when present.
    pub before: Option<Value>,
    /// Current structural value, when present.
    pub after: Option<Value>,
}

/// Kind of format change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatChangeKind {
    /// A structural value appeared.
    Added,
    /// A structural value disappeared.
    Removed,
    /// An existing structural value changed.
    Changed,
}

fn diff_values(path: &str, before: &Value, after: &Value, changes: &mut Vec<FormatChange>) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let keys = before
                .keys()
                .chain(after.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                let child_path = format!("{path}/{}", escape_json_pointer(&key));
                match (before.get(&key), after.get(&key)) {
                    (Some(before), Some(after)) => {
                        diff_values(&child_path, before, after, changes);
                    }
                    (Some(before), None) => changes.push(FormatChange {
                        kind: FormatChangeKind::Removed,
                        path: child_path,
                        before: Some(before.clone()),
                        after: None,
                    }),
                    (None, Some(after)) => changes.push(FormatChange {
                        kind: FormatChangeKind::Added,
                        path: child_path,
                        before: None,
                        after: Some(after.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ if before == after => {}
        _ => changes.push(FormatChange {
            kind: FormatChangeKind::Changed,
            path: path.to_owned(),
            before: Some(before.clone()),
            after: Some(after.clone()),
        }),
    }
}

fn diff_allowed_structure(
    baseline: &FormatFingerprint,
    current: &FormatFingerprint,
    changes: &mut Vec<FormatChange>,
) {
    diff_scalar(
        "/manifest_version",
        baseline.manifest_version,
        current.manifest_version,
        changes,
    );
    diff_scalar("/provider", &baseline.provider, &current.provider, changes);
    diff_scalar(
        "/source/options/max_line_bytes",
        baseline.options.max_line_bytes,
        current.options.max_line_bytes,
        changes,
    );
    diff_scalar(
        "/source/options/max_depth",
        baseline.options.max_depth,
        current.options.max_depth,
        changes,
    );

    match (&baseline.source, &current.source) {
        (
            FormatSourceFingerprint::JsonLines(baseline)
            | FormatSourceFingerprint::JsonLinesZstd(baseline),
            FormatSourceFingerprint::JsonLines(current)
            | FormatSourceFingerprint::JsonLinesZstd(current),
        ) => diff_allowed_json_lines(baseline, current, changes),
        (FormatSourceFingerprint::Sqlite(baseline), FormatSourceFingerprint::Sqlite(current)) => {
            diff_values(
                "/source/objects",
                &json_value(&baseline.objects),
                &json_value(&current.objects),
                changes,
            )
        }
        (baseline, current) => changes.push(FormatChange {
            kind: FormatChangeKind::Changed,
            path: "/source/format".to_owned(),
            before: Some(json_value(baseline.format())),
            after: Some(json_value(current.format())),
        }),
    }
}

fn diff_allowed_json_lines(
    baseline: &JsonLinesFingerprint,
    current: &JsonLinesFingerprint,
    changes: &mut Vec<FormatChange>,
) {
    diff_allowed_map_sets("/source/fields", &baseline.fields, &current.fields, changes);
    diff_allowed_map_sets(
        "/source/discriminators",
        &baseline.discriminators,
        &current.discriminators,
        changes,
    );
    diff_allowed_set(
        "/source/depth_limited_paths",
        &baseline.depth_limited_paths,
        &current.depth_limited_paths,
        changes,
    );
    diff_allowed_set(
        "/source/dynamic_map_paths",
        &baseline.dynamic_map_paths,
        &current.dynamic_map_paths,
        changes,
    );
    diff_allowed_set(
        "/source/opaque_paths",
        &baseline.opaque_paths,
        &current.opaque_paths,
        changes,
    );
}

fn diff_allowed_map_sets<T>(
    path: &str,
    baseline: &BTreeMap<String, BTreeSet<T>>,
    current: &BTreeMap<String, BTreeSet<T>>,
    changes: &mut Vec<FormatChange>,
) where
    T: Clone + Ord + Serialize,
{
    for (key, current_values) in current {
        let key_path = format!("{path}/{}", escape_json_pointer(key));
        let Some(baseline_values) = baseline.get(key) else {
            changes.push(FormatChange {
                kind: FormatChangeKind::Added,
                path: key_path,
                before: None,
                after: Some(json_value(current_values)),
            });
            continue;
        };
        for value in current_values.difference(baseline_values) {
            changes.push(FormatChange {
                kind: FormatChangeKind::Added,
                path: format!(
                    "{key_path}/{}",
                    escape_json_pointer(&json_scalar_label(value))
                ),
                before: None,
                after: Some(json_value(value)),
            });
        }
    }
}

fn diff_allowed_set(
    path: &str,
    baseline: &BTreeSet<String>,
    current: &BTreeSet<String>,
    changes: &mut Vec<FormatChange>,
) {
    for value in current.difference(baseline) {
        changes.push(FormatChange {
            kind: FormatChangeKind::Added,
            path: format!("{path}/{}", escape_json_pointer(value)),
            before: None,
            after: Some(Value::String(value.clone())),
        });
    }
}

fn diff_scalar<T>(path: &str, before: T, after: T, changes: &mut Vec<FormatChange>)
where
    T: PartialEq + Serialize,
{
    if before != after {
        changes.push(FormatChange {
            kind: FormatChangeKind::Changed,
            path: path.to_owned(),
            before: Some(json_value(before)),
            after: Some(json_value(after)),
        });
    }
}

fn json_scalar_label(value: &impl Serialize) -> String {
    match json_value(value) {
        Value::String(value) => value,
        value => value.to_string(),
    }
}

fn json_value(value: impl Serialize) -> Value {
    serde_json::to_value(value).expect("format fingerprint values must be serializable")
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::{
        FormatBaseline, FormatChangeKind, FormatComparisonMode, FormatFingerprint,
        FormatProbeDiagnostic, FormatProbeOptions, FormatSourceFingerprint, JsonLinesFingerprint,
        JsonValueKind, ProviderId, SqliteFingerprint, FORMAT_FINGERPRINT_VERSION,
    };

    fn json_fingerprint(records_seen: u64, include_status: bool) -> FormatFingerprint {
        let mut observations = JsonLinesFingerprint {
            records_seen,
            valid_records: records_seen,
            ..JsonLinesFingerprint::default()
        };
        observations
            .fields
            .entry("$".to_owned())
            .or_default()
            .insert(JsonValueKind::Object);
        if include_status {
            observations
                .fields
                .entry("$/status".to_owned())
                .or_default()
                .insert(JsonValueKind::String);
        }
        FormatFingerprint {
            manifest_version: FORMAT_FINGERPRINT_VERSION,
            provider: ProviderId::new("test"),
            options: FormatProbeOptions::default(),
            source: FormatSourceFingerprint::JsonLines(observations),
        }
    }

    #[test]
    fn fingerprint_round_trips_as_a_baseline() {
        let fingerprint = json_fingerprint(1, false);
        let serialized = serde_json::to_vec(&fingerprint).unwrap();
        let restored = serde_json::from_slice(&serialized).unwrap();

        assert_eq!(fingerprint, restored);
        assert!(fingerprint.diff(&restored).is_empty());
    }

    #[test]
    fn format_diff_ignores_runtime_counts_and_diagnostics() {
        let baseline = json_fingerprint(1, false);
        let mut current = json_fingerprint(2, false);
        let FormatSourceFingerprint::JsonLines(observations) = &mut current.source else {
            panic!("expected JSONL observations");
        };
        observations.malformed_records = 1;
        observations.diagnostics.push(FormatProbeDiagnostic {
            code: "format_probe.invalid_json".to_owned(),
            record: Some(2),
            message: "record is not valid JSON".to_owned(),
        });

        assert!(baseline.diff(&current).is_empty());
        assert!(!baseline.has_unreadable_records());
        assert!(current.has_unreadable_records());
    }

    #[test]
    fn incomplete_trailing_record_is_not_unreadable() {
        let mut fingerprint = json_fingerprint(1, false);
        let FormatSourceFingerprint::JsonLines(observations) = &mut fingerprint.source else {
            panic!("expected JSONL observations");
        };
        observations.incomplete_records = 1;

        assert!(!fingerprint.has_unreadable_records());
    }

    #[test]
    fn format_diff_reports_structural_changes() {
        let baseline = json_fingerprint(1, false);
        let current = json_fingerprint(2, true);

        assert!(baseline.diff(&current).changes.iter().any(|change| {
            change.kind == FormatChangeKind::Added && change.path.contains("$~1status")
        }));
    }

    #[test]
    fn allowed_structure_ignores_approved_fields_missing_from_current_sample() {
        let baseline = json_fingerprint(2, true);
        let current = json_fingerprint(1, false);

        let report = baseline.check_compatibility(&current, FormatComparisonMode::AllowedStructure);

        assert!(report.is_compatible());
    }

    #[test]
    fn allowed_structure_reports_new_paths_and_value_kinds() {
        let baseline = json_fingerprint(1, false);
        let mut current = json_fingerprint(1, true);
        let FormatSourceFingerprint::JsonLines(observations) = &mut current.source else {
            panic!("expected JSONL observations");
        };
        observations
            .fields
            .entry("$".to_owned())
            .or_default()
            .insert(JsonValueKind::Array);

        let report = baseline.check_compatibility(&current, FormatComparisonMode::AllowedStructure);

        assert!(!report.is_compatible());
        assert!(report
            .diff
            .changes
            .iter()
            .any(|change| change.path.contains("$~1status")));
        assert!(report
            .diff
            .changes
            .iter()
            .any(|change| change.path.ends_with("/array")));
    }

    #[test]
    fn allowed_structure_treats_plain_and_compressed_json_lines_as_equivalent() {
        let baseline = json_fingerprint(1, false);
        let mut current = baseline.clone();
        let FormatSourceFingerprint::JsonLines(observations) = current.source else {
            panic!("expected JSONL observations");
        };
        current.source = FormatSourceFingerprint::JsonLinesZstd(observations);

        assert!(baseline
            .check_compatibility(&current, FormatComparisonMode::AllowedStructure)
            .is_compatible());
        assert!(!baseline.diff(&current).is_empty());
    }

    #[test]
    fn compatibility_report_includes_unreadable_records() {
        let baseline = json_fingerprint(1, false);
        let mut current = baseline.clone();
        let FormatSourceFingerprint::JsonLines(observations) = &mut current.source else {
            panic!("expected JSONL observations");
        };
        observations.malformed_records = 1;

        let report = baseline.check_compatibility(&current, FormatComparisonMode::AllowedStructure);

        assert!(report.diff.is_empty());
        assert!(report.unreadable_records);
        assert!(!report.is_compatible());
    }

    #[test]
    fn baseline_removes_runtime_observations() {
        let mut fingerprint = json_fingerprint(5, false);
        let FormatSourceFingerprint::JsonLines(observations) = &mut fingerprint.source else {
            panic!("expected JSONL observations");
        };
        observations.incomplete_records = 1;
        observations.diagnostics.push(FormatProbeDiagnostic {
            code: "format_probe.invalid_json".to_owned(),
            record: Some(5),
            message: "record is not valid JSON".to_owned(),
        });

        let baseline = FormatBaseline::new(FormatComparisonMode::AllowedStructure, &fingerprint);
        let FormatSourceFingerprint::JsonLines(observations) = baseline.fingerprint.source else {
            panic!("expected JSONL observations");
        };

        assert_eq!(observations.records_seen, 0);
        assert_eq!(observations.incomplete_records, 0);
        assert!(observations.diagnostics.is_empty());
    }

    #[test]
    fn allowed_structure_baseline_canonicalizes_json_line_compression() {
        let mut fingerprint = json_fingerprint(1, false);
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        fingerprint.source = FormatSourceFingerprint::JsonLinesZstd(observations);

        let baseline = FormatBaseline::new(FormatComparisonMode::AllowedStructure, &fingerprint);

        assert!(matches!(
            baseline.fingerprint.source,
            FormatSourceFingerprint::JsonLines(_)
        ));
    }

    #[test]
    fn format_diff_rejects_a_different_provider() {
        let baseline = json_fingerprint(1, false);
        let mut current = baseline.clone();
        current.provider = ProviderId::new("other");

        assert!(baseline
            .diff(&current)
            .changes
            .iter()
            .any(|change| change.path == "/provider"));
    }

    #[test]
    fn sqlite_diff_ignores_json_line_options() {
        let baseline = FormatFingerprint {
            manifest_version: FORMAT_FINGERPRINT_VERSION,
            provider: ProviderId::new("test"),
            options: FormatProbeOptions::default(),
            source: FormatSourceFingerprint::Sqlite(SqliteFingerprint::default()),
        };
        let current = FormatFingerprint {
            options: FormatProbeOptions {
                max_line_bytes: 1,
                max_depth: 1,
                max_diagnostics: 1,
            },
            ..baseline.clone()
        };

        assert!(baseline.diff(&current).is_empty());
    }
}
