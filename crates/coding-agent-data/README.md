<div align="center">

<p><a href="README.md"><strong>English</strong></a> | <a href="README_ZH.md">中文</a></p>

</div>

# coding-agent-data

`coding-agent-data` is a Rust library that provides a unified API and data
model to access local coding-agent data. It is designed for desktop apps, CLIs,
analytics tools, history viewers, and similar applications. It supports
browsing, indexing, synchronizing, analyzing, and visualizing that data without
requiring applications to understand each agent's data locations, storage
formats, or schemas.

Applications access data from different coding agents through a consistent,
provider-neutral API. Each provider finds and interprets its own local sources.
Applications can store, index, synchronize, redact, and present the normalized
records in ways that fit their needs.

## Capabilities

- Auto discover provider-specific local data sources.
- Normalized, source-scoped records with stable identities and source
  references, while retaining original data for provider-specific use cases.
- Opaque, serializable checkpoints for bounded, incremental scans.
- Optional filesystem monitoring with debounce and periodic reconciliation.

## Glossary

[English glossary and semantic guide](./GLOSSARY.md)

## Supported providers

| Provider    | Local data sources                                   |
| ----------- | ---------------------------------------------------- |
| Codex       | `state_5.sqlite`, rollout `.jsonl`, and `.jsonl.zst` |
| Claude Code | Main and subagent transcript `.jsonl`                |

## Quick start

From this workspace:

```toml
[dependencies]
coding-agent-data = { path = "crates/coding-agent-data", default-features = false, features = ["codex"] }
```

Run an initial scan and continue until the provider reports that the source is
caught up:

```rust,no_run
use coding_agent_data::providers::codex::CodexProvider;
use coding_agent_data::{Batch, Checkpoint, Provider, Result};

fn synchronize() -> Result<Checkpoint> {
    let provider = CodexProvider::discover()?;
    let mut checkpoint = None;

    loop {
        let batch = provider.scan(checkpoint.as_ref())?;
        let has_more = batch.has_more;

        apply_batch_atomically(&batch)?;

        if !has_more {
            return Ok(batch.checkpoint);
        }

        checkpoint = Some(batch.checkpoint);
    }
}

fn apply_batch_atomically(batch: &Batch) -> Result<()> {
    // Apply `batch.changes` and persist `batch.checkpoint` in one transaction.
    for change in &batch.changes {
        println!("{change:?}");
    }

    Ok(())
}
```

Pass `None` only when starting from the beginning. Apply each batch and persist
its checkpoint in the same transaction. Only then pass that checkpoint to the
next `scan`. When `has_more` is `true`, scan again immediately. After catch-up,
providers with the corresponding watch feature can start `WatchProvider::watch`
from the last applied checkpoint.
