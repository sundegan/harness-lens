use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::format_probe::{
    FormatFingerprint, FormatProbeDiagnostic, FormatProbeOptions, FormatSourceFingerprint,
    JsonLinesFingerprint, JsonValueKind, FORMAT_FINGERPRINT_VERSION,
};
use crate::providers::shared::jsonl::{read_bounded_line, LineRead};
use crate::{Error, Result};

pub(crate) trait JsonFormatPolicy {
    fn is_discriminator_field(&self, field: &str) -> bool;

    fn is_dynamic_map_field(&self, field: &str) -> bool;

    fn is_opaque_data_field(&self, field: &str) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IncompleteRecordPolicy {
    AllowTrailing,
    #[cfg(feature = "codex")]
    Reject,
}

pub(crate) fn probe_json_lines_path(
    provider: &str,
    path: &Path,
    options: FormatProbeOptions,
    policy: &impl JsonFormatPolicy,
) -> Result<FormatFingerprint> {
    let file =
        File::open(path).map_err(|error| Error::io("open format probe source", path, error))?;
    let observations = probe_json_lines(
        BufReader::new(file),
        path,
        options,
        policy,
        IncompleteRecordPolicy::AllowTrailing,
    )?;
    Ok(FormatFingerprint {
        manifest_version: FORMAT_FINGERPRINT_VERSION,
        provider: crate::ProviderId::new(provider),
        options,
        source: FormatSourceFingerprint::JsonLines(observations),
    })
}

pub(crate) fn probe_json_lines(
    mut reader: impl BufRead,
    path: &Path,
    options: FormatProbeOptions,
    policy: &impl JsonFormatPolicy,
    incomplete_record_policy: IncompleteRecordPolicy,
) -> Result<JsonLinesFingerprint> {
    let mut fingerprint = JsonLinesFingerprint::default();
    let mut bytes = Vec::new();
    let mut record = 0_u64;

    loop {
        let (too_large, complete) =
            match read_bounded_line(&mut reader, &mut bytes, options.max_line_bytes)
                .map_err(|error| Error::io("read format probe source", path, error))?
            {
                LineRead::Eof => break,
                LineRead::Partial { too_large } => (too_large, false),
                LineRead::Complete { too_large, .. } => (too_large, true),
            };
        if bytes_are_empty_or_whitespace(&bytes) && !too_large {
            continue;
        }

        record = record.saturating_add(1);
        fingerprint.records_seen = fingerprint.records_seen.saturating_add(1);
        if too_large {
            fingerprint.oversized_records = fingerprint.oversized_records.saturating_add(1);
            push_probe_diagnostic(
                &mut fingerprint,
                options.max_diagnostics,
                FormatProbeDiagnostic {
                    code: "format_probe.json_line_too_large".to_owned(),
                    record: Some(record),
                    message: format!(
                        "record exceeds the configured {} byte limit",
                        options.max_line_bytes
                    ),
                },
            );
            continue;
        }

        let contents = trim_line_ending(&bytes);
        match serde_json::from_slice::<Value>(contents) {
            Ok(value) => {
                fingerprint.valid_records = fingerprint.valid_records.saturating_add(1);
                JsonObserver {
                    max_depth: options.max_depth,
                    policy,
                    fingerprint: &mut fingerprint,
                }
                .observe(&value, "$", None, 0);
            }
            Err(error) => {
                if !complete && incomplete_record_policy == IncompleteRecordPolicy::AllowTrailing {
                    fingerprint.incomplete_records =
                        fingerprint.incomplete_records.saturating_add(1);
                    continue;
                }
                fingerprint.malformed_records = fingerprint.malformed_records.saturating_add(1);
                push_probe_diagnostic(
                    &mut fingerprint,
                    options.max_diagnostics,
                    FormatProbeDiagnostic {
                        code: "format_probe.invalid_json".to_owned(),
                        record: Some(record),
                        message: format!(
                            "record is not valid JSON at line {}, column {}",
                            error.line(),
                            error.column()
                        ),
                    },
                );
            }
        }
    }

    Ok(fingerprint)
}

struct JsonObserver<'a, Policy> {
    max_depth: usize,
    policy: &'a Policy,
    fingerprint: &'a mut JsonLinesFingerprint,
}

impl<Policy: JsonFormatPolicy> JsonObserver<'_, Policy> {
    fn observe(&mut self, value: &Value, path: &str, field_name: Option<&str>, depth: usize) {
        self.fingerprint
            .fields
            .entry(path.to_owned())
            .or_default()
            .insert(json_value_kind(value));

        if let (Some(field_name), Some(value)) = (field_name, value.as_str()) {
            if self.policy.is_discriminator_field(field_name) && is_safe_discriminator(value) {
                self.fingerprint
                    .discriminators
                    .entry(path.to_owned())
                    .or_default()
                    .insert(value.to_owned());
            }
        }

        if depth >= self.max_depth {
            if value.is_array() || value.is_object() {
                self.fingerprint.depth_limited_paths.insert(path.to_owned());
            }
            return;
        }

        if field_name.is_some_and(|field| self.policy.is_opaque_data_field(field))
            && (value.is_array() || value.is_object())
        {
            self.fingerprint.opaque_paths.insert(path.to_owned());
            return;
        }

        match value {
            Value::Array(values) => {
                let child_path = format!("{path}/*");
                for value in values {
                    self.observe(value, &child_path, None, depth.saturating_add(1));
                }
            }
            Value::Object(values)
                if field_name.is_some_and(|field| self.policy.is_dynamic_map_field(field)) =>
            {
                self.fingerprint.dynamic_map_paths.insert(path.to_owned());
                let child_path = format!("{path}/*");
                for value in values.values() {
                    self.observe(value, &child_path, None, depth.saturating_add(1));
                }
            }
            Value::Object(values) => {
                for (name, value) in values {
                    let child_path = format!("{path}/{}", escape_json_pointer(name));
                    self.observe(value, &child_path, Some(name), depth.saturating_add(1));
                }
            }
            _ => {}
        }
    }
}

fn json_value_kind(value: &Value) -> JsonValueKind {
    match value {
        Value::Null => JsonValueKind::Null,
        Value::Bool(_) => JsonValueKind::Boolean,
        Value::Number(value) if value.is_i64() || value.is_u64() => JsonValueKind::Integer,
        Value::Number(_) => JsonValueKind::Float,
        Value::String(_) => JsonValueKind::String,
        Value::Array(_) => JsonValueKind::Array,
        Value::Object(_) => JsonValueKind::Object,
    }
}

fn is_safe_discriminator(value: &str) -> bool {
    // Keep provider vocabulary useful for drift detection without retaining
    // arbitrary strings that may contain user content or local identifiers.
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn trim_line_ending(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    bytes.strip_suffix(b"\r").unwrap_or(bytes)
}

fn bytes_are_empty_or_whitespace(bytes: &[u8]) -> bool {
    trim_line_ending(bytes).iter().all(u8::is_ascii_whitespace)
}

fn push_probe_diagnostic(
    fingerprint: &mut JsonLinesFingerprint,
    limit: usize,
    diagnostic: FormatProbeDiagnostic,
) {
    if fingerprint.diagnostics.len() < limit {
        fingerprint.diagnostics.push(diagnostic);
    } else {
        fingerprint.diagnostics_truncated = fingerprint.diagnostics_truncated.saturating_add(1);
    }
}

#[cfg(test)]
pub(crate) mod compatibility_test {
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    use crate::format_probe::{
        DiscoveredFormatArtifact, FormatBaseline, FormatComparisonMode, FormatFingerprint,
        FormatProbe, FormatProbeOptions, FormatSourceFingerprint, JsonLinesFingerprint,
    };
    use crate::{Diagnostic, DiagnosticSeverity, Provider};

    const MAX_SCAN_BATCHES: usize = 10_000;
    const UPDATE_BASELINES_ENV: &str = "CODING_AGENT_DATA_UPDATE_FORMAT_BASELINES";

    pub(crate) struct FormatBaselineSpec {
        pub(crate) artifact_label: &'static str,
        pub(crate) comparison_mode: FormatComparisonMode,
        pub(crate) path: &'static str,
    }

    impl FormatBaselineSpec {
        pub(crate) const fn new(
            artifact_label: &'static str,
            comparison_mode: FormatComparisonMode,
            path: &'static str,
        ) -> Self {
            Self {
                artifact_label,
                comparison_mode,
                path,
            }
        }
    }

    pub(crate) struct ProbedFormatArtifact<'a> {
        spec: &'a FormatBaselineSpec,
        fingerprint: FormatFingerprint,
    }

    pub(crate) fn inspect_artifacts<'a>(
        provider: &str,
        probe: &impl FormatProbe,
        artifacts: &[DiscoveredFormatArtifact],
        baselines: &'a [FormatBaselineSpec],
    ) -> Vec<ProbedFormatArtifact<'a>> {
        let mut inspected = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            let fingerprint = probe
                .probe_path(&artifact.path, FormatProbeOptions::default())
                .unwrap_or_else(|_| {
                    panic!(
                        "{provider} {}: the artifact format could not be inspected",
                        artifact.label
                    )
                });
            assert_eq!(
                fingerprint.provider.as_str(),
                provider,
                "{provider} {}: the probe returned a fingerprint for a different Provider",
                artifact.label
            );
            assert_eq!(
                fingerprint.manifest_version,
                crate::format_probe::FORMAT_FINGERPRINT_VERSION,
                "{provider} {}: the probe returned an unsupported fingerprint version",
                artifact.label
            );
            assert_fingerprint_is_readable(provider, &artifact.label, &fingerprint);
            let spec = baselines
                .iter()
                .find(|spec| spec.artifact_label == artifact.label)
                .unwrap_or_else(|| {
                    panic!(
                        "{provider} {}: no approved format baseline is configured",
                        artifact.label
                    )
                });
            inspected.push(ProbedFormatArtifact { spec, fingerprint });
        }

        for spec in baselines {
            assert!(
                inspected
                    .iter()
                    .any(|artifact| artifact.spec.artifact_label == spec.artifact_label),
                "{provider} {}: no representative local artifact was discovered",
                spec.artifact_label
            );
        }

        inspected
    }

    pub(crate) fn check_or_update_baselines(
        provider: &str,
        artifacts: &[ProbedFormatArtifact<'_>],
    ) {
        for artifact in artifacts {
            if update_baselines_requested() {
                update_baseline(provider, artifact.spec, &artifact.fingerprint);
            } else {
                assert_matches_baseline(provider, artifact.spec, &artifact.fingerprint);
            }
        }
    }

    pub(crate) fn assert_baselines_are_valid(provider: &str, baselines: &[FormatBaselineSpec]) {
        for spec in baselines {
            let baseline = read_baseline(provider, spec);
            assert_eq!(
                baseline.comparison_mode, spec.comparison_mode,
                "{provider} {}: the committed baseline uses the wrong comparison mode",
                spec.artifact_label
            );
            assert_eq!(
                baseline.fingerprint.manifest_version,
                crate::format_probe::FORMAT_FINGERPRINT_VERSION,
                "{provider} {}: the committed baseline uses an unsupported fingerprint version",
                spec.artifact_label
            );
            assert_eq!(
                baseline.fingerprint.provider.as_str(),
                provider,
                "{provider} {}: the committed baseline identifies a different Provider",
                spec.artifact_label
            );
            assert_eq!(
                baseline,
                FormatBaseline::new(spec.comparison_mode, &baseline.fingerprint),
                "{provider} {}: the committed baseline contains runtime observations or is not canonical",
                spec.artifact_label
            );
        }
    }

    pub(crate) fn artifact_path<'a>(
        artifacts: &'a [DiscoveredFormatArtifact],
        label: &str,
    ) -> &'a Path {
        artifacts
            .iter()
            .find(|artifact| artifact.label == label)
            .map(|artifact| artifact.path.as_path())
            .unwrap_or_else(|| panic!("{label}: no representative local artifact was discovered"))
    }

    pub(crate) fn hard_link_artifact(provider: &str, source: &Path, destination: &Path) {
        fs::hard_link(source, destination).unwrap_or_else(|_| {
            panic!(
                "{provider}: failed to hard-link the representative artifact into an isolated source"
            )
        });
    }

    pub(crate) fn assert_provider_scan_is_compatible(
        provider_name: &str,
        provider: &impl Provider,
        expected_artifacts: &[(&str, &Path)],
        ignored_diagnostics: &[&str],
        provider_incompatible_diagnostic: impl Fn(&Diagnostic) -> bool,
    ) {
        let expected_artifacts = expected_artifacts
            .iter()
            .map(|(label, path)| {
                (
                    *label,
                    crate::providers::shared::source_path::normalize((*path).to_path_buf()),
                )
            })
            .collect::<Vec<_>>();
        let mut checkpoint = None;
        let mut batches = 0_usize;
        let mut changes = 0_usize;
        let mut artifact_changes = vec![0_usize; expected_artifacts.len()];
        let mut diagnostic_counts = BTreeMap::<String, usize>::new();
        let mut incompatible_diagnostics = BTreeMap::<String, usize>::new();

        loop {
            assert!(
                batches < MAX_SCAN_BATCHES,
                "{provider_name}: Provider scan did not converge within {MAX_SCAN_BATCHES} batches"
            );
            let batch = provider.scan(checkpoint.as_ref()).unwrap_or_else(|_| {
                panic!(
                    "{provider_name}: the current Provider adapter failed to scan the representative artifact"
                )
            });
            batches += 1;
            changes = changes.saturating_add(batch.changes.len());
            for change in &batch.changes {
                let crate::Change::Upsert(record) = change else {
                    continue;
                };
                for (index, (_, path)) in expected_artifacts.iter().enumerate() {
                    if record.origin.path == *path {
                        artifact_changes[index] = artifact_changes[index].saturating_add(1);
                    }
                }
            }
            for diagnostic in &batch.diagnostics {
                if ignored_diagnostics.contains(&diagnostic.code.as_str()) {
                    continue;
                }
                *diagnostic_counts
                    .entry(diagnostic.code.clone())
                    .or_default() += 1;
                if is_incompatible_diagnostic(diagnostic)
                    || provider_incompatible_diagnostic(diagnostic)
                {
                    *incompatible_diagnostics
                        .entry(diagnostic.code.clone())
                        .or_default() += 1;
                }
            }

            if !batch.has_more {
                break;
            }
            checkpoint = Some(batch.checkpoint);
        }

        for ((artifact, _), changes) in expected_artifacts.iter().zip(artifact_changes) {
            assert!(
                changes > 0,
                "{provider_name} {artifact}: the current Provider adapter produced no normalized records"
            );
        }
        assert!(
            incompatible_diagnostics.is_empty(),
            "{provider_name}: the current Provider adapter reported incompatible local data: {incompatible_diagnostics:?}"
        );

        println!(
            "{provider_name} adapter: compatible, {changes} normalized changes across {batches} batch(es), {} diagnostic(s)",
            diagnostic_counts.values().sum::<usize>()
        );
        if !diagnostic_counts.is_empty() {
            println!("{provider_name} non-failing diagnostics: {diagnostic_counts:?}");
        }
    }

    fn assert_fingerprint_is_readable(
        provider: &str,
        artifact: &str,
        fingerprint: &FormatFingerprint,
    ) {
        match &fingerprint.source {
            FormatSourceFingerprint::JsonLines(observations) => {
                assert_json_lines_are_readable(provider, artifact, "JSONL", observations);
            }
            FormatSourceFingerprint::JsonLinesZstd(observations) => {
                assert_json_lines_are_readable(provider, artifact, "JSONL.ZST", observations);
            }
            FormatSourceFingerprint::Sqlite(observations) => {
                assert!(
                    !observations.objects.is_empty(),
                    "{provider} {artifact}: the SQLite schema contains no user-defined objects"
                );
                let columns = observations
                    .objects
                    .values()
                    .map(|object| object.columns.len())
                    .sum::<usize>();
                let indexes = observations
                    .objects
                    .values()
                    .map(|object| object.indexes.len())
                    .sum::<usize>();
                println!(
                    "{provider} {artifact}: SQLite readable, {} objects, {columns} columns, {indexes} indexes",
                    observations.objects.len()
                );
            }
        }
    }

    fn assert_matches_baseline(
        provider: &str,
        spec: &FormatBaselineSpec,
        fingerprint: &FormatFingerprint,
    ) {
        let baseline = read_baseline(provider, spec);
        assert_eq!(
            baseline.comparison_mode, spec.comparison_mode,
            "{provider} {}: the baseline comparison mode does not match the Provider policy",
            spec.artifact_label
        );
        let report = baseline.check(fingerprint);
        let change_summary = report
            .diff
            .changes
            .iter()
            .take(20)
            .map(|change| format!("{:?} {}", change.kind, change.path))
            .collect::<Vec<_>>();
        let omitted = report
            .diff
            .changes
            .len()
            .saturating_sub(change_summary.len());
        assert!(
            report.is_compatible(),
            "{provider} {}: local format is incompatible with the approved baseline, unreadable_records={}, changes={change_summary:?}, omitted_changes={omitted}",
            spec.artifact_label,
            report.unreadable_records
        );
        println!(
            "{provider} {}: approved format baseline matched ({:?})",
            spec.artifact_label, spec.comparison_mode
        );
    }

    fn read_baseline(provider: &str, spec: &FormatBaselineSpec) -> FormatBaseline {
        let bytes = fs::read(spec.path).unwrap_or_else(|_| {
            panic!(
                "{provider} {}: failed to read the approved format baseline",
                spec.artifact_label
            )
        });
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            panic!(
                "{provider} {}: the approved format baseline is not valid",
                spec.artifact_label
            )
        })
    }

    fn update_baseline(provider: &str, spec: &FormatBaselineSpec, fingerprint: &FormatFingerprint) {
        let mut updated = FormatBaseline::new(spec.comparison_mode, fingerprint);
        if spec.comparison_mode == FormatComparisonMode::AllowedStructure {
            match fs::read(spec.path) {
                Ok(bytes) => {
                    let existing =
                        serde_json::from_slice::<FormatBaseline>(&bytes).unwrap_or_else(|_| {
                            panic!(
                                "{provider} {}: the existing format baseline is not valid",
                                spec.artifact_label
                            )
                        });
                    assert_eq!(
                        existing.comparison_mode, spec.comparison_mode,
                        "{provider} {}: the existing baseline uses the wrong comparison mode",
                        spec.artifact_label
                    );
                    assert_eq!(
                        existing.fingerprint.provider, updated.fingerprint.provider,
                        "{provider} {}: the existing baseline identifies a different Provider",
                        spec.artifact_label
                    );
                    if existing.fingerprint.manifest_version == updated.fingerprint.manifest_version
                        && existing.fingerprint.options.max_line_bytes
                            == updated.fingerprint.options.max_line_bytes
                        && existing.fingerprint.options.max_depth
                            == updated.fingerprint.options.max_depth
                    {
                        merge_approved_json_structure(provider, spec, &mut updated, &existing);
                    } else {
                        println!(
                            "{provider} {}: rebuilding the baseline because the fingerprint version or probe limits changed",
                            spec.artifact_label
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => panic!(
                    "{provider} {}: failed to read the existing format baseline",
                    spec.artifact_label
                ),
            }
        }
        let serialized = serde_json::to_vec_pretty(&updated)
            .unwrap_or_else(|_| panic!("{provider}: failed to serialize the format baseline"));
        let path = Path::new(spec.path);
        let parent = path.parent().unwrap_or_else(|| {
            panic!(
                "{provider} {}: the format baseline path has no parent",
                spec.artifact_label
            )
        });
        fs::create_dir_all(parent).unwrap_or_else(|_| {
            panic!(
                "{provider} {}: failed to prepare the format baseline directory",
                spec.artifact_label
            )
        });
        let mut temporary = tempfile::NamedTempFile::new_in(parent).unwrap_or_else(|_| {
            panic!(
                "{provider} {}: failed to create a temporary format baseline",
                spec.artifact_label
            )
        });
        temporary.write_all(&serialized).unwrap_or_else(|_| {
            panic!(
                "{provider} {}: failed to write the temporary format baseline",
                spec.artifact_label
            )
        });
        temporary.as_file().sync_all().unwrap_or_else(|_| {
            panic!(
                "{provider} {}: failed to flush the temporary format baseline",
                spec.artifact_label
            )
        });
        temporary.persist(path).unwrap_or_else(|_| {
            panic!(
                "{provider} {}: failed to replace the format baseline",
                spec.artifact_label
            )
        });
        println!(
            "{provider} {}: updated approved format baseline",
            spec.artifact_label
        );
    }

    fn merge_approved_json_structure(
        provider: &str,
        spec: &FormatBaselineSpec,
        updated: &mut FormatBaseline,
        existing: &FormatBaseline,
    ) {
        let Some(updated) = json_lines_mut(&mut updated.fingerprint.source) else {
            panic!(
                "{provider} {}: the current allowed-structure baseline is not JSONL",
                spec.artifact_label
            )
        };
        let Some(existing) = json_lines(&existing.fingerprint.source) else {
            panic!(
                "{provider} {}: the existing allowed-structure baseline is not JSONL",
                spec.artifact_label
            )
        };

        for (path, kinds) in &existing.fields {
            updated
                .fields
                .entry(path.clone())
                .or_default()
                .extend(kinds);
        }
        for (path, values) in &existing.discriminators {
            updated
                .discriminators
                .entry(path.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
        updated
            .depth_limited_paths
            .extend(existing.depth_limited_paths.iter().cloned());
        updated
            .dynamic_map_paths
            .extend(existing.dynamic_map_paths.iter().cloned());
        updated
            .opaque_paths
            .extend(existing.opaque_paths.iter().cloned());
    }

    fn json_lines(source: &FormatSourceFingerprint) -> Option<&JsonLinesFingerprint> {
        match source {
            FormatSourceFingerprint::JsonLines(observations)
            | FormatSourceFingerprint::JsonLinesZstd(observations) => Some(observations),
            FormatSourceFingerprint::Sqlite(_) => None,
        }
    }

    fn json_lines_mut(source: &mut FormatSourceFingerprint) -> Option<&mut JsonLinesFingerprint> {
        match source {
            FormatSourceFingerprint::JsonLines(observations)
            | FormatSourceFingerprint::JsonLinesZstd(observations) => Some(observations),
            FormatSourceFingerprint::Sqlite(_) => None,
        }
    }

    fn update_baselines_requested() -> bool {
        std::env::var_os(UPDATE_BASELINES_ENV).is_some_and(|value| value == "1")
    }

    fn assert_json_lines_are_readable(
        provider: &str,
        artifact: &str,
        format: &str,
        observations: &JsonLinesFingerprint,
    ) {
        assert!(
            observations.records_seen > 0,
            "{provider} {artifact}: the {format} artifact contains no records"
        );
        assert!(
            observations.valid_records > 0,
            "{provider} {artifact}: the {format} artifact contains no valid JSON records"
        );
        assert_eq!(
            observations.malformed_records, 0,
            "{provider} {artifact}: the {format} artifact contains malformed JSON records"
        );
        assert_eq!(
            observations.oversized_records, 0,
            "{provider} {artifact}: the {format} artifact contains records beyond the parser limit"
        );
        assert_eq!(
            observations.diagnostics_truncated, 0,
            "{provider} {artifact}: format diagnostics were truncated"
        );
        println!(
            "{provider} {artifact}: {format} readable, {} records, {} fields, {} trailing incomplete",
            observations.records_seen,
            observations.fields.len(),
            observations.incomplete_records
        );
    }

    fn is_incompatible_diagnostic(diagnostic: &Diagnostic) -> bool {
        matches!(diagnostic.severity, DiagnosticSeverity::Error)
            || diagnostic.code.ends_with(".invalid_json")
            || diagnostic.code.ends_with(".line_too_large")
            || diagnostic.code.ends_with(".diagnostics.truncated")
    }

    mod tests {
        use super::{
            assert_json_lines_are_readable, merge_approved_json_structure, FormatBaselineSpec,
        };
        use crate::format_probe::{
            FormatBaseline, FormatComparisonMode, FormatFingerprint, FormatProbeOptions,
            FormatSourceFingerprint, JsonLinesFingerprint, JsonValueKind,
            FORMAT_FINGERPRINT_VERSION,
        };
        use crate::ProviderId;

        fn baseline_with_field(path: &str) -> FormatBaseline {
            let mut observations = JsonLinesFingerprint::default();
            observations
                .fields
                .entry(path.to_owned())
                .or_default()
                .insert(JsonValueKind::String);
            FormatBaseline::new(
                FormatComparisonMode::AllowedStructure,
                &FormatFingerprint {
                    manifest_version: FORMAT_FINGERPRINT_VERSION,
                    provider: ProviderId::new("test"),
                    options: FormatProbeOptions::default(),
                    source: FormatSourceFingerprint::JsonLines(observations),
                },
            )
        }

        #[test]
        fn baseline_update_preserves_previously_approved_json_structure() {
            let existing = baseline_with_field("$/existing");
            let mut updated = baseline_with_field("$/current");
            let spec = FormatBaselineSpec::new(
                "transcript",
                FormatComparisonMode::AllowedStructure,
                "unused.json",
            );

            merge_approved_json_structure("test", &spec, &mut updated, &existing);

            let FormatSourceFingerprint::JsonLines(observations) = updated.fingerprint.source
            else {
                panic!("expected JSONL observations");
            };
            assert!(observations.fields.contains_key("$/existing"));
            assert!(observations.fields.contains_key("$/current"));
        }

        #[test]
        #[should_panic(expected = "contains no valid JSON records")]
        fn compatibility_test_rejects_an_incomplete_only_json_artifact() {
            let observations = JsonLinesFingerprint {
                records_seen: 1,
                incomplete_records: 1,
                ..JsonLinesFingerprint::default()
            };

            assert_json_lines_are_readable("test", "transcript", "JSONL", &observations);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{probe_json_lines_path, JsonFormatPolicy};
    use crate::format_probe::{
        FormatProbeOptions, FormatSourceFingerprint, JsonValueKind, ProbedFormat,
    };

    struct TestPolicy;

    impl JsonFormatPolicy for TestPolicy {
        fn is_discriminator_field(&self, field: &str) -> bool {
            matches!(field, "type" | "status")
        }

        fn is_dynamic_map_field(&self, field: &str) -> bool {
            field == "changes"
        }

        fn is_opaque_data_field(&self, field: &str) -> bool {
            field == "input"
        }
    }

    #[test]
    fn json_probe_records_structure_without_payload_scalar_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"user\",\"message\":{\"content\":\"secret prompt\"}}\n",
                "{\"type\":\"tool\",\"input\":{\"secret_key\":{\"nested_secret\":\"/private/repository\"}},\"status\":\"completed\"}\n",
            ),
        )
        .unwrap();

        let fingerprint =
            probe_json_lines_path("test", &path, FormatProbeOptions::default(), &TestPolicy)
                .unwrap();
        assert_eq!(fingerprint.source.format(), ProbedFormat::JsonLines);
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        assert_eq!(observations.records_seen, 2);
        assert_eq!(observations.valid_records, 2);
        assert!(observations.fields["$/message/content"].contains(&JsonValueKind::String));
        assert_eq!(
            observations.discriminators["$/type"],
            ["tool".to_owned(), "user".to_owned()].into_iter().collect()
        );

        let serialized = serde_json::to_string(&observations).unwrap();
        assert!(observations.opaque_paths.contains("$/input"));
        assert!(!serialized.contains("secret_key"));
        assert!(!serialized.contains("nested_secret"));
        assert!(!serialized.contains("secret prompt"));
        assert!(!serialized.contains("/private/repository"));
    }

    #[test]
    fn dynamic_map_keys_are_replaced_by_a_wildcard() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rollout.jsonl");
        fs::write(
            &path,
            "{\"changes\":{\"/private/repository/src/lib.rs\":{\"type\":\"update\"}}}\n",
        )
        .unwrap();

        let fingerprint =
            probe_json_lines_path("test", &path, FormatProbeOptions::default(), &TestPolicy)
                .unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        let serialized = serde_json::to_string(&observations).unwrap();
        assert!(observations.dynamic_map_paths.contains("$/changes"));
        assert!(observations.fields.contains_key("$/changes/*/type"));
        assert!(!serialized.contains("private"));
        assert!(!serialized.contains("lib.rs"));
    }

    #[test]
    fn malformed_and_oversized_records_are_bounded_diagnostics() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.jsonl");
        fs::write(&path, b"not-json\n{\"content\":\"too large\"}\n{}\n").unwrap();
        let options = FormatProbeOptions {
            max_line_bytes: 8,
            max_depth: 8,
            max_diagnostics: 1,
        };

        let fingerprint = probe_json_lines_path("test", &path, options, &TestPolicy).unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        assert_eq!(observations.records_seen, 3);
        assert_eq!(observations.malformed_records, 1);
        assert_eq!(observations.oversized_records, 1);
        assert_eq!(observations.valid_records, 1);
        assert_eq!(observations.diagnostics.len(), 1);
        assert_eq!(observations.diagnostics_truncated, 1);
    }

    #[test]
    fn line_limit_excludes_line_endings_and_applies_at_end_of_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.jsonl");
        fs::write(&path, b"12345678\r\n123456789").unwrap();
        let options = FormatProbeOptions {
            max_line_bytes: 8,
            ..FormatProbeOptions::default()
        };

        let fingerprint = probe_json_lines_path("test", &path, options, &TestPolicy).unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        assert_eq!(observations.records_seen, 2);
        assert_eq!(observations.valid_records, 1);
        assert_eq!(observations.oversized_records, 1);
        assert_eq!(observations.diagnostics[0].record, Some(2));
    }

    #[test]
    fn incomplete_trailing_record_is_not_reported_as_malformed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.jsonl");
        fs::write(&path, b"{}\n{\"type\":").unwrap();

        let fingerprint =
            probe_json_lines_path("test", &path, FormatProbeOptions::default(), &TestPolicy)
                .unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        assert_eq!(observations.records_seen, 2);
        assert_eq!(observations.valid_records, 1);
        assert_eq!(observations.incomplete_records, 1);
        assert_eq!(observations.malformed_records, 0);
        assert!(observations.diagnostics.is_empty());
    }

    #[test]
    fn non_structural_discriminator_values_are_not_retained() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.jsonl");
        fs::write(&path, "{\"type\":\"private project\"}\n").unwrap();

        let fingerprint =
            probe_json_lines_path("test", &path, FormatProbeOptions::default(), &TestPolicy)
                .unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };

        assert!(observations.discriminators.is_empty());
        assert!(!serde_json::to_string(&observations)
            .unwrap()
            .contains("private project"));
    }
}
