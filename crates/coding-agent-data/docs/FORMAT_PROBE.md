<div align="center">

<p><a href="FORMAT_PROBE.md"><strong>English</strong></a> | <a href="FORMAT_PROBE_ZH.md">中文</a></p>

</div>

# Format Probe

The format probe detects changes in Provider-private local formats and verifies that the current Provider adapters can still read and normalize representative Codex and Claude Code data.

It combines two checks:

| Check | Question | Method |
| ----- | -------- | ------ |
| Structural compatibility | Did the persisted Provider format gain an unapproved change? | Build a structural fingerprint without general payload scalar contents and compare it with a committed, reviewed baseline |
| Adapter compatibility | Can the current adapter actually read representative local data? | Run the real `Provider::scan` against an isolated source and verify normalized output and diagnostics for every representative artifact |

A structural comparison alone can miss an adapter that silently emits no data. An adapter scan alone can silently ignore unknown fields. The local compatibility test therefore runs both checks.

## Coverage

| Provider | Representative artifact | Inspected structure | Baseline policy | Adapter verification |
| -------- | ----------------------- | ------------------- | --------------- | -------------------- |
| Codex | `state_5.sqlite` | Tables, views, columns, indexes, and DDL digests | `Exact` | The state database must produce normalized Records |
| Codex | Latest rollout `.jsonl` or `.jsonl.zst` | JSON paths, value kinds, approved discriminators, and probe-policy markers | `AllowedStructure` | The representative rollout must produce normalized Records |
| Claude Code | Latest transcript `.jsonl` | JSON paths, value kinds, approved discriminators, and probe-policy markers | `AllowedStructure` | The representative transcript must produce normalized Records |

Discovery intentionally selects the latest representative JSONL artifact so the check remains fast and bounded. It does not replay every historical Session.

## Execution flow

```text
Discover representative Provider-local artifacts
    ↓
Inspect read-only and freeze structural fingerprints
    ↓
Verify each artifact contains readable, valid structure
    ↓
Run the real Provider adapter against an isolated source
    ↓
Require normalized Records per artifact and no compatibility errors
    ↓
Compare the frozen fingerprints with reviewed baselines
    ↓
Pass when compatible, fail on unapproved drift
```

Fingerprints are captured before the adapter scan. If an active JSONL file is appended during the scan, the baseline operation still uses structure that was already presented to the adapter in this test run.

Provider sources remain read-only:

- SQLite schemas are inspected through a read-only connection without reading data rows.
- The latest JSONL artifact is hard-linked into a temporary isolated source, without copying transcript contents.
- The probe and tests never write to Provider databases, rollouts, or transcripts.

## Comparison policies

### Exact

`Exact` is used for completely enumerable formats such as SQLite schemas. Any addition, removal, or change to observed tables, views, columns, indexes, or relevant DDL digests is incompatible.

### AllowedStructure

One JSONL artifact usually exercises only part of a Provider's complete event vocabulary. Requiring every current Session to reproduce all previously seen fields would create false failures. `AllowedStructure` is directional:

| Current observation | Result | Reason |
| ------------------- | ------ | ------ |
| Omits an approved field or event type | Allowed | The current Session may not exercise that capability |
| Adds a JSON path not present in the baseline | Fails | It may be a new field or changed nesting |
| Adds a JSON value kind at a known path | Fails | Existing parser assumptions may no longer hold |
| Adds an approved-policy discriminator value | Fails | It may be a new event, role, status, or mode |
| Adds a dynamic-map, opaque, or depth-limit marker | Fails | The Provider-private inspection policy needs review |

Plain JSONL and JSONL.ZST are canonicalized to the same JSONL representation in allowed-structure baselines, so compression alone is not format drift.

## Failure conditions

The local compatibility test fails under these conditions:

| Category | Failure |
| -------- | ------- |
| Discovery | A configured representative artifact is missing, or a discovered artifact has no baseline |
| JSONL readability | No records, no valid JSON records, malformed complete JSON, an oversized record, or truncated diagnostics |
| Compressed archive | A `.jsonl.zst` cannot be decoded or its final record is invalid |
| SQLite readability | Read-only open or schema inspection fails, no user-defined object exists, or a required Codex table disappears |
| Adapter scan | Scan fails, does not converge within the bounded batch count, or an artifact produces no `Change::Upsert` |
| Adapter diagnostics | Error diagnostics, invalid JSON, oversized records, truncated diagnostics, or Provider-defined compatibility diagnostics |
| Baseline | Corrupt baseline, mismatched Provider/version/policy, or an unapproved structural difference |

An active plain `.jsonl` may be mid-append. A final record without a line ending that is not yet valid JSON is counted as `incomplete_records` rather than immediately treated as corruption. A `.jsonl.zst` is a closed archive and does not receive this exception.

## Run locally

Run from the `coding-agent-data` crate:

```bash
cd crates/coding-agent-data
make test-provider-formats
```

The test reads representative local Provider data. A Provider that is not installed or has no supported artifact is explicitly skipped. Once some artifacts are discovered, a missing representative artifact required by the configured baselines fails so that only a complete Provider surface passes.

Successful output contains only artifact types and structural counts, normalized Change and Batch counts, diagnostic codes and counts, and baseline results. It does not print local source paths, prompts, message bodies, tool payloads, or SQLite rows.

Normal `cargo test` does not read developer-local Provider data. The local compatibility tests use `#[ignore]` and run only through the command above or an explicit ignored-test invocation. Baseline validity and comparison algorithms remain part of the normal test suite.

## Review and update baselines

Do not update a baseline merely to make a drift failure pass. First verify the upstream change, update the adapter when needed, and add a minimal sanitized fixture:

```text
Local compatibility test detects drift
    ↓
Inspect the structural difference and upstream format
    ↓
Fix or extend the Provider adapter
    ↓
Add a minimal sanitized regression test
    ↓
Verify the real adapter succeeds
    ↓
Update and review the baseline
```

Explicit update commands:

```bash
make update-provider-format-baselines
make test-provider-formats
```

| Baseline type | Update behavior |
| ------------- | --------------- |
| SQLite `Exact` | Replace the baseline with the current complete schema fingerprint |
| JSONL `AllowedStructure` | Merge current structure into previously approved structure without deleting known structure absent from this sample |

Baselines are replaced atomically through a temporary file in the same directory. An unreadable or invalid baseline, mismatched Provider, or mismatched comparison policy fails instead of being silently overwritten. Fingerprint-version or structural probe-limit changes explicitly report a rebuild.

Review changes to:

| Provider | Artifact | Baseline |
| -------- | -------- | -------- |
| Codex | State database | [`../tests/format-baselines/codex/state_database.json`](../tests/format-baselines/codex/state_database.json) |
| Codex | Rollout | [`../tests/format-baselines/codex/rollout.json`](../tests/format-baselines/codex/rollout.json) |
| Claude Code | Transcript | [`../tests/format-baselines/claude_code/transcript.json`](../tests/format-baselines/claude_code/transcript.json) |

## Public API

The `format-probe` feature exposes Provider-neutral fingerprint and comparison contracts. Provider implementations own private discovery and format interpretation.

```rust,no_run
use std::path::Path;

use coding_agent_data::format_probe::{
    FormatBaseline, FormatProbe, FormatProbeOptions,
};
use coding_agent_data::providers::codex::CodexFormatProbe;

fn check_artifact(
    baseline: &FormatBaseline,
    artifact: &Path,
) -> coding_agent_data::Result<bool> {
    let current = CodexFormatProbe.probe_path(
        artifact,
        FormatProbeOptions::default(),
    )?;
    Ok(baseline.check(&current).is_compatible())
}
```

| API | Purpose |
| --- | ------- |
| `FormatProbe::probe_path` | Fingerprint an explicit Provider artifact |
| `FormatProbeDiscovery::discover_artifacts` | Discover representative local artifacts using Provider rules |
| `FormatFingerprint::diff` | Compare two fingerprints exactly |
| `FormatFingerprint::check_compatibility` | Check structural drift and unreadable records using an explicit policy |
| `FormatBaseline::check` | Check a current fingerprint using the baseline's stored policy |
| `FormatCompatibilityReport` | Report differences, comparison policy, and unreadable-record state |

When callers do not have an explicit path, they can first use `FormatProbeDiscovery::discover_artifacts` to obtain representative artifacts with stable labels, then select the corresponding baseline for each label.

External CLIs, CI jobs, and desktop applications can use this API directly. The crate-local `make test-provider-formats` command additionally runs the real adapter, so it provides broader compatibility evidence.

## Fingerprint privacy boundary

Fingerprints retain:

- Provider ID, fingerprint version, and probe limits
- JSON paths and JSON value kinds
- Short, Provider-selected discriminators such as event `type`, message `role`, and execution `status`
- Dynamic-map, opaque-payload, and depth-limit paths
- SQLite objects, columns, indexes, and SHA-256 DDL digests

Fingerprints do not retain:

- Inspected local paths
- General JSON string, number, or boolean values
- Prompts, message bodies, reasoning, tool arguments, or tool results
- SQLite data rows
- API keys, credentials, or environment-variable values

Each Provider's `JsonFormatPolicy` must classify fields that can contain user content, tool payloads, or dynamic keys as opaque or dynamic maps. Baseline changes still require both automated sensitive-data checks and human review.

## Code boundaries

| File | Responsibility |
| ---- | -------------- |
| [`../src/format_probe.rs`](../src/format_probe.rs) | Public API, fingerprint and baseline models, and Provider-neutral comparison algorithms |
| [`../src/providers/shared/format_probe.rs`](../src/providers/shared/format_probe.rs) | Shared JSONL observer and local compatibility-test helpers |
| [`../src/providers/codex/format_probe.rs`](../src/providers/codex/format_probe.rs) | Codex discovery, private JSON policy, and compatibility test |
| [`../src/providers/codex/format_probe/sqlite.rs`](../src/providers/codex/format_probe/sqlite.rs) | Read-only Codex SQLite schema inspection |
| [`../src/providers/claude_code/format_probe.rs`](../src/providers/claude_code/format_probe.rs) | Claude Code discovery, private JSON policy, and compatibility test |

Provider-private directories, schemas, discriminator policies, and diagnostic rules must stay within the corresponding Provider module. The root `format_probe` module must not depend on Codex or Claude Code private formats.

## Fingerprint version

`FORMAT_FINGERPRINT_VERSION` versions the serialized fingerprint protocol, not the Provider data format. It normally changes when:

- fingerprint-field semantics change
- JSON path or value-kind observation rules change
- SQLite schema observation rules change
- comparison semantics change incompatibly

A Provider adding a field does not itself require a fingerprint-version bump. The baseline comparison reports it as ordinary structural drift.

## Known limitations

| Limitation | Effect |
| ---------- | ------ |
| Samples only the latest representative JSONL | Does not prove compatibility with every historical artifact |
| Directional JSONL comparison | Cannot prove that an approved field was permanently removed when the current sample does not contain it |
| Retains only Provider-selected discriminators | String-enum changes outside that policy are not reported separately, although their paths and value kinds remain observed |
| Unknown is a valid unified-model fallback | Not every Unknown Record or Event indicates adapter incompatibility |
| Temporary isolation requires hard links | A rare unsupported or cross-filesystem environment fails explicitly instead of copying sensitive transcripts |

The probe provides structural and adapter-compatibility evidence for the representative artifacts it inspected. It is not proof of complete historical or semantic coverage.
