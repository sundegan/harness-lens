<div align="center">

<p><a href="README.md">English</a> | <a href="README_ZH.md"><strong>中文</strong></a></p>

</div>

# coding-agent-data

`coding-agent-data` 是一个 Rust 库，为本地 coding agent 数据提供统一的 API 和数据模型。它面向桌面应用、CLI、分析工具、历史查看器等场景，让上层应用无需了解不同 Agent 的数据位置、存储格式和私有 Schema，就可以浏览、索引、同步、分析和可视化这些数据。

应用通过一致的、Provider-neutral 的 API 访问不同 coding agent 的数据。每个 Provider 负责发现和解析自己的本地数据源，应用则可以按照自身需求保存、索引、同步、脱敏和展示归一化后的记录。

## 能力

- 自动发现各 Provider 的本地数据源
- 提供带稳定身份和来源引用的归一化记录，同时保留 Provider 原始数据
- 提供可序列化、对上层透明的 Checkpoint，用于有界增量扫描
- 可选的文件系统监控，支持防抖和定期重新校验
- 格式探针，用于发现 Provider 本地数据格式变化

## 支持的 Provider

| Provider    | 本地数据源                                         |
| ----------- | -------------------------------------------------- |
| Codex       | `state_5.sqlite`、rollout `.jsonl` 和 `.jsonl.zst` |
| Claude Code | 主 Agent 和子 Agent 的 transcript `.jsonl`         |

## 快速开始

在当前 workspace 中添加依赖：

```toml
[dependencies]
coding-agent-data = { path = "crates/coding-agent-data", default-features = false, features = ["codex"] }
```

执行首次扫描，并在 Provider 仍有后续数据时继续扫描：

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
    // 在同一个事务中应用 batch.changes 并保存 batch.checkpoint。
    for change in &batch.changes {
        println!("{change:?}");
    }

    Ok(())
}
```

只有从头开始扫描时才传入 `None`。应用每个 Batch 后，应在同一个事务中保存其变更和 Checkpoint，然后再把该 Checkpoint 传给下一次 `scan`。当 `has_more` 为 `true` 时，应立即继续扫描。完成追赶后，如果 Provider 提供对应的 watch 功能，可以从最后一个已应用的 Checkpoint 启动 `WatchProvider::watch`。

## 文档

- [中文术语表与语义模型指南](./docs/GLOSSARY_ZH.md)
- [格式探针设计与使用指南](./docs/FORMAT_PROBE_ZH.md)
- [全部 crate 文档](./docs/README.md)
