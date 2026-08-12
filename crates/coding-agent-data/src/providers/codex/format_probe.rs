mod sqlite;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::format_probe::{
    DiscoveredFormatArtifact, FormatFingerprint, FormatProbe, FormatProbeDiscovery,
    FormatProbeOptions, FormatSourceFingerprint, FORMAT_FINGERPRINT_VERSION,
};
use crate::providers::shared::format_probe::{
    probe_json_lines, probe_json_lines_path, IncompleteRecordPolicy, JsonFormatPolicy,
};
use crate::{Error, Result};

use super::CodexSource;

/// Structural format probe for Codex state databases and rollout artifacts.
#[derive(Clone, Copy, Debug, Default)]
pub struct CodexFormatProbe;

impl FormatProbe for CodexFormatProbe {
    fn probe_path(&self, path: &Path, options: FormatProbeOptions) -> Result<FormatFingerprint> {
        if sqlite::is_sqlite(path)? {
            return sqlite::probe_path(path, options);
        }
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("zst") => {
                probe_zstd_json_lines_path(path, options)
            }
            Some(extension) if extension.eq_ignore_ascii_case("jsonl") => {
                probe_json_lines_path(super::PROVIDER_ID, path, options, &CodexJsonFormatPolicy)
            }
            _ => Err(Error::InvalidConfiguration(format!(
                "unsupported Codex format-probe artifact: {}",
                path.display()
            ))),
        }
    }
}

impl FormatProbeDiscovery for CodexFormatProbe {
    fn discover_artifacts(&self) -> Result<Vec<DiscoveredFormatArtifact>> {
        discover_artifacts_from(&CodexSource::discover()?)
    }
}

fn discover_artifacts_from(source: &CodexSource) -> Result<Vec<DiscoveredFormatArtifact>> {
    let mut artifacts = Vec::new();
    let state_database = source.state_database();
    if state_database.is_file() {
        artifacts.push(DiscoveredFormatArtifact::new(
            "state_database",
            state_database,
        ));
    }
    if let Some(rollout) = super::rollout::latest_rollout(source)? {
        artifacts.push(DiscoveredFormatArtifact::new("latest_rollout", rollout));
    }
    Ok(artifacts)
}

fn probe_zstd_json_lines_path(
    path: &Path,
    options: FormatProbeOptions,
) -> Result<FormatFingerprint> {
    let file =
        File::open(path).map_err(|error| Error::io("open format probe source", path, error))?;
    let decoder = zstd::stream::read::Decoder::new(file)
        .map_err(|error| Error::io("decode format probe source", path, error))?;
    let observations = probe_json_lines(
        BufReader::new(decoder),
        path,
        options,
        &CodexJsonFormatPolicy,
        IncompleteRecordPolicy::Reject,
    )?;
    Ok(FormatFingerprint {
        manifest_version: FORMAT_FINGERPRINT_VERSION,
        provider: crate::ProviderId::new(super::PROVIDER_ID),
        options,
        source: FormatSourceFingerprint::JsonLinesZstd(observations),
    })
}

struct CodexJsonFormatPolicy;

impl JsonFormatPolicy for CodexJsonFormatPolicy {
    fn is_discriminator_field(&self, field: &str) -> bool {
        matches!(field, "type" | "kind" | "role" | "status" | "mode")
    }

    fn is_dynamic_map_field(&self, field: &str) -> bool {
        matches!(
            field,
            "agents_states"
                | "changes"
                | "environment"
                | "environment_variables"
                | "environments"
                | "env"
                | "headers"
                | "mcp_servers"
        )
    }

    fn is_opaque_data_field(&self, field: &str) -> bool {
        matches!(
            field,
            "active_permission_profile"
                | "arguments"
                | "dynamic_tools"
                | "input"
                | "output"
                | "permission_profile"
                | "provider_attributes"
                | "raw"
                | "result"
                | "tools"
                | "value"
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::File;
    use std::io::Write;

    use super::{discover_artifacts_from, CodexFormatProbe};
    use crate::format_probe::{
        FormatComparisonMode, FormatProbe, FormatProbeDiscovery, FormatProbeOptions,
        FormatSourceFingerprint, ProbedFormat,
    };
    use crate::providers::codex::{CodexProvider, CodexSource};
    use crate::providers::shared::format_probe::compatibility_test::{
        artifact_path, assert_baselines_are_valid, assert_provider_scan_is_compatible,
        check_or_update_baselines, hard_link_artifact, inspect_artifacts, FormatBaselineSpec,
    };
    use crate::Error;

    const FORMAT_BASELINES: &[FormatBaselineSpec] = &[
        FormatBaselineSpec::new(
            "state_database",
            FormatComparisonMode::Exact,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/format-baselines/codex/state_database.json"
            ),
        ),
        FormatBaselineSpec::new(
            "latest_rollout",
            FormatComparisonMode::AllowedStructure,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/format-baselines/codex/rollout.json"
            ),
        ),
    ];

    #[test]
    fn committed_codex_format_baselines_are_valid() {
        assert_baselines_are_valid(super::super::PROVIDER_ID, FORMAT_BASELINES);
    }

    #[test]
    fn codex_discovery_selects_state_database_and_latest_rollout() {
        let directory = tempfile::tempdir().unwrap();
        let codex_home = directory.path().join("codex");
        let rollout_directory = codex_home.join("sessions/2026/08/12");
        fs::create_dir_all(&rollout_directory).unwrap();
        let database = codex_home.join("state_5.sqlite");
        let rollout = rollout_directory.join("rollout.jsonl");
        fs::write(&database, []).unwrap();
        fs::write(&rollout, "{}\n").unwrap();
        let source = CodexSource::new(&codex_home, &codex_home);
        let expected_database = source.sqlite_home().join("state_5.sqlite");
        let expected_rollout = source
            .codex_home()
            .join("sessions/2026/08/12/rollout.jsonl");

        let artifacts = discover_artifacts_from(&source).unwrap();

        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].label, "state_database");
        assert_eq!(artifacts[0].path, expected_database);
        assert_eq!(artifacts[1].label, "latest_rollout");
        assert_eq!(artifacts[1].path, expected_rollout);
    }

    #[test]
    fn codex_policy_hides_dynamic_and_opaque_provider_fields() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rollout.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"event\",\"changes\":{\"/private/repository\":{\"kind\":\"update\"}}}\n",
                "{\"type\":\"context\",\"permission_profile\":{\"private_key\":\"secret\"}}\n",
            ),
        )
        .unwrap();

        let fingerprint = CodexFormatProbe
            .probe_path(&path, FormatProbeOptions::default())
            .unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        let serialized = serde_json::to_string(&observations).unwrap();

        assert!(observations.dynamic_map_paths.contains("$/changes"));
        assert!(observations.opaque_paths.contains("$/permission_profile"));
        assert!(!serialized.contains("private_key"));
        assert!(!serialized.contains("/private/repository"));
    }

    #[test]
    fn codex_probe_reads_compressed_json_lines() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rollout.jsonl.zst");
        let file = File::create(&path).unwrap();
        let mut encoder = zstd::stream::write::Encoder::new(file, 0).unwrap();
        encoder
            .write_all(b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"redacted\"}}\n")
            .unwrap();
        encoder.finish().unwrap();

        let fingerprint = CodexFormatProbe
            .probe_path(&path, FormatProbeOptions::default())
            .unwrap();

        assert_eq!(fingerprint.source.format(), ProbedFormat::JsonLinesZstd);
    }

    #[test]
    fn codex_probe_rejects_an_invalid_final_record_in_a_compressed_rollout() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rollout.jsonl.zst");
        let file = File::create(&path).unwrap();
        let mut encoder = zstd::stream::write::Encoder::new(file, 0).unwrap();
        encoder.write_all(b"{\"type\":").unwrap();
        encoder.finish().unwrap();

        let fingerprint = CodexFormatProbe
            .probe_path(&path, FormatProbeOptions::default())
            .unwrap();

        assert!(fingerprint.has_unreadable_records());
        let FormatSourceFingerprint::JsonLinesZstd(observations) = fingerprint.source else {
            panic!("expected compressed JSONL observations");
        };
        assert_eq!(observations.malformed_records, 1);
        assert_eq!(observations.incomplete_records, 0);
    }

    #[test]
    fn codex_probe_rejects_unknown_artifact_formats() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("artifact.txt");
        fs::write(&path, "{}\n").unwrap();

        assert!(CodexFormatProbe
            .probe_path(&path, FormatProbeOptions::default())
            .is_err());
    }

    #[test]
    #[ignore = "reads the developer's local Codex data; run with `make test-provider-formats`"]
    fn local_format_compatibility() {
        let artifacts = match CodexFormatProbe.discover_artifacts() {
            Ok(artifacts) if artifacts.is_empty() => {
                println!("codex: skipped, no supported local artifacts were found");
                return;
            }
            Ok(artifacts) => artifacts,
            Err(Error::SourceNotFound(_)) => {
                println!("codex: skipped, the local Provider is not installed");
                return;
            }
            Err(_) => panic!("codex: failed to discover local format artifacts"),
        };
        let fingerprints =
            inspect_artifacts("codex", &CodexFormatProbe, &artifacts, FORMAT_BASELINES);
        let database = artifact_path(&artifacts, "state_database");
        let database_fingerprint = CodexFormatProbe
            .probe_path(database, FormatProbeOptions::default())
            .unwrap_or_else(|_| panic!("codex: failed to inspect the state database"));
        let FormatSourceFingerprint::Sqlite(database_schema) = database_fingerprint.source else {
            panic!("codex: state_database is not SQLite");
        };
        assert!(
            database_schema.objects.contains_key("threads"),
            "codex: the state database no longer contains the threads table"
        );

        let rollout = artifact_path(&artifacts, "latest_rollout");
        let directory = tempfile::tempdir()
            .unwrap_or_else(|_| panic!("codex: failed to create an isolated compatibility source"));
        let sessions = directory.path().join("sessions/2000/01/01");
        fs::create_dir_all(&sessions).unwrap_or_else(|_| {
            panic!("codex: failed to prepare an isolated compatibility source")
        });
        let linked_rollout =
            sessions.join(if rollout.extension().is_some_and(|value| value == "zst") {
                "rollout.jsonl.zst"
            } else {
                "rollout.jsonl"
            });
        hard_link_artifact("codex", rollout, &linked_rollout);

        let sqlite_home = database
            .parent()
            .unwrap_or_else(|| panic!("codex: the state database has no parent directory"));
        let source = CodexSource::new(directory.path(), sqlite_home);
        assert_provider_scan_is_compatible(
            "codex",
            &CodexProvider::new(source),
            &[
                ("state_database", database),
                ("latest_rollout", &linked_rollout),
            ],
            &["codex.state_db.rollout_outside_source"],
            |diagnostic| diagnostic.code.starts_with("codex.state_db."),
        );
        check_or_update_baselines("codex", &fingerprints);
    }
}
