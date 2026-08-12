use std::path::Path;

use crate::format_probe::{
    DiscoveredFormatArtifact, FormatFingerprint, FormatProbe, FormatProbeDiscovery,
    FormatProbeOptions,
};
use crate::providers::shared::format_probe::{probe_json_lines_path, JsonFormatPolicy};
use crate::{Error, Result};

use super::ClaudeCodeSource;

/// Structural format probe for Claude Code transcript artifacts.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClaudeCodeFormatProbe;

impl FormatProbe for ClaudeCodeFormatProbe {
    fn probe_path(&self, path: &Path, options: FormatProbeOptions) -> Result<FormatFingerprint> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("jsonl") => probe_json_lines_path(
                super::PROVIDER_ID,
                path,
                options,
                &ClaudeCodeJsonFormatPolicy,
            ),
            _ => Err(Error::InvalidConfiguration(format!(
                "unsupported Claude Code format-probe artifact: {}",
                path.display()
            ))),
        }
    }
}

impl FormatProbeDiscovery for ClaudeCodeFormatProbe {
    fn discover_artifacts(&self) -> Result<Vec<DiscoveredFormatArtifact>> {
        discover_artifacts_from(&ClaudeCodeSource::discover()?)
    }
}

fn discover_artifacts_from(source: &ClaudeCodeSource) -> Result<Vec<DiscoveredFormatArtifact>> {
    Ok(super::transcript::latest_transcript(source)?
        .map(|path| DiscoveredFormatArtifact::new("latest_transcript", path))
        .into_iter()
        .collect())
}

struct ClaudeCodeJsonFormatPolicy;

impl JsonFormatPolicy for ClaudeCodeJsonFormatPolicy {
    fn is_discriminator_field(&self, field: &str) -> bool {
        matches!(field, "type" | "kind" | "role" | "status" | "mode")
    }

    fn is_dynamic_map_field(&self, field: &str) -> bool {
        matches!(
            field,
            "environment" | "environment_variables" | "env" | "headers" | "mcp_servers"
        )
    }

    fn is_opaque_data_field(&self, field: &str) -> bool {
        matches!(
            field,
            "arguments" | "input" | "output" | "raw" | "result" | "toolUseResult" | "value"
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{discover_artifacts_from, ClaudeCodeFormatProbe};
    use crate::format_probe::{
        FormatComparisonMode, FormatProbe, FormatProbeDiscovery, FormatProbeOptions,
        FormatSourceFingerprint,
    };
    use crate::providers::claude_code::{ClaudeCodeProvider, ClaudeCodeSource};
    use crate::providers::shared::format_probe::compatibility_test::{
        artifact_path, assert_baselines_are_valid, assert_provider_scan_is_compatible,
        check_or_update_baselines, hard_link_artifact, inspect_artifacts, FormatBaselineSpec,
    };
    use crate::Error;

    const FORMAT_BASELINES: &[FormatBaselineSpec] = &[FormatBaselineSpec::new(
        "latest_transcript",
        FormatComparisonMode::AllowedStructure,
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/format-baselines/claude_code/transcript.json"
        ),
    )];

    #[test]
    fn committed_claude_code_format_baselines_are_valid() {
        assert_baselines_are_valid(super::super::PROVIDER_ID, FORMAT_BASELINES);
    }

    #[test]
    fn claude_code_discovery_selects_latest_transcript() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().join("claude");
        let project_dir = config_dir.join("projects/-workspace");
        fs::create_dir_all(&project_dir).unwrap();
        let transcript = project_dir.join("session.jsonl");
        fs::write(&transcript, "{}\n").unwrap();
        let source = ClaudeCodeSource::new(&config_dir);
        let expected_transcript = source.projects_dir().join("-workspace/session.jsonl");

        let artifacts = discover_artifacts_from(&source).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].label, "latest_transcript");
        assert_eq!(artifacts[0].path, expected_transcript);
    }

    #[test]
    fn claude_code_policy_hides_tool_payload_fields() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.jsonl");
        fs::write(
            &path,
            "{\"type\":\"assistant\",\"toolUseResult\":{\"private_key\":\"secret\"}}\n",
        )
        .unwrap();

        let fingerprint = ClaudeCodeFormatProbe
            .probe_path(&path, FormatProbeOptions::default())
            .unwrap();
        let FormatSourceFingerprint::JsonLines(observations) = fingerprint.source else {
            panic!("expected JSONL observations");
        };
        let serialized = serde_json::to_string(&observations).unwrap();

        assert!(observations.opaque_paths.contains("$/toolUseResult"));
        assert!(!serialized.contains("private_key"));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn claude_code_probe_rejects_non_transcript_formats() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transcript.json");
        fs::write(&path, "{}\n").unwrap();

        assert!(ClaudeCodeFormatProbe
            .probe_path(&path, FormatProbeOptions::default())
            .is_err());
    }

    #[test]
    #[ignore = "reads the developer's local Claude Code data; run with `make test-provider-formats`"]
    fn local_format_compatibility() {
        let artifacts = match ClaudeCodeFormatProbe.discover_artifacts() {
            Ok(artifacts) if artifacts.is_empty() => {
                println!("claude-code: skipped, no supported local artifacts were found");
                return;
            }
            Ok(artifacts) => artifacts,
            Err(Error::SourceNotFound(_)) => {
                println!("claude-code: skipped, the local Provider is not installed");
                return;
            }
            Err(_) => panic!("claude-code: failed to discover local format artifacts"),
        };
        let fingerprints = inspect_artifacts(
            "claude-code",
            &ClaudeCodeFormatProbe,
            &artifacts,
            FORMAT_BASELINES,
        );
        let transcript = artifact_path(&artifacts, "latest_transcript");
        let directory = tempfile::tempdir().unwrap_or_else(|_| {
            panic!("claude-code: failed to create an isolated compatibility source")
        });
        let project = directory.path().join("projects/local");
        fs::create_dir_all(&project).unwrap_or_else(|_| {
            panic!("claude-code: failed to prepare an isolated compatibility source")
        });
        let linked_transcript = project.join("transcript.jsonl");
        hard_link_artifact("claude-code", transcript, &linked_transcript);

        let source = ClaudeCodeSource::new(directory.path());
        assert_provider_scan_is_compatible(
            "claude-code",
            &ClaudeCodeProvider::new(source),
            &[("latest_transcript", &linked_transcript)],
            &[],
            |_| false,
        );
        check_or_update_baselines("claude-code", &fingerprints);
    }
}
