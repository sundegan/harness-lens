# coding-agent-data

`coding-agent-data` is a read-only Rust data-access library for desktop
applications, CLIs, analytics tools, history viewers, and other consumers of
local coding-agent data. Developers can use it to build local browsing,
indexing, synchronization, analysis, and visualization features without having
to adapt separately to each agent's data locations, storage formats, and
schemas.

Applications use one consistent API to work with data from different coding
agents. The library supports source discovery, reading and parsing,
normalization into a common record/change model, checkpoint-based incremental
synchronization, and live change monitoring.

## Capabilities

- auto discover provider-specific local data sources;
- read SQLite rows and line-oriented or compressed artifacts without modifying
  them;
- expose common record identity, data kinds, timestamps, source
  references, changes, and diagnostics;
- resume scans from opaque, serializable checkpoints;
- emit upserts, deletions, source resets, and source removals;
- watch for filesystem changes with debounce and periodic reconciliation.

Provider implementations own source-specific discovery and parsing. Consumers
depend on `AgentDataProvider`, `ChangeBatch`, `DataRecord`, and related common
types instead of private table names, paths, or event schemas. Provider-native
payload fields remain available in `DataRecord::payload`; applications may map
them into their own domain model without coupling that model to this crate.

## Provider support

The first provider supports Codex:

- thread metadata from `state_5.sqlite`;
- rollout event records from active and archived JSONL files;
- compressed `.jsonl.zst` rollouts;
- opaque, serializable checkpoints for incremental scans;
- debounced filesystem watching with periodic reconciliation.

Additional Codex data kinds and coding-agent providers can be added behind the
same public API.

## Usage

```rust,no_run
use coding_agent_data::providers::codex::CodexProvider;
use coding_agent_data::AgentDataProvider;

let provider = CodexProvider::discover()?;
let batch = provider.scan(None)?;

println!("received {} changes", batch.changes.len());
# Ok::<(), coding_agent_data::Error>(())
```

Persist the returned checkpoint only after the batch has been processed
successfully. Pass it to the next `scan` or to `watch` so the provider can emit
only subsequent changes.
