# `coding-agent-data` 定位与架构

## 1. Crate 定位

`coding-agent-data` 是一个面向桌面应用、CLI、分析工具、历史查看器及其他本机
Coding Agent 数据消费者的只读 Rust 数据访问库。开发者可以用它构建本机数据
浏览、索引、同步、分析和可视化功能，无需分别适配每个 Agent 的私有数据目录、
存储介质、文件格式和 schema。

它通过一套统一的 Rust API 提供：

- 数据源发现；
- 数据读取与解析；
- 统一的 record/change 表达；
- 基于 checkpoint 的增量同步；
- 实时数据变更监听；
- 来源定位与结构化诊断。

当前只实现 Codex Provider。这里描述的是库的长期边界，不表示当前已经支持所有
Coding Agent 或某个 Agent 的全部本机数据。

## 2. 职责边界

### 2.1 负责

1. 发现调用方授权访问的本机 Agent 数据目录。
2. 只读解析 Provider 已支持的 SQLite、JSONL 或压缩数据。
3. 将不同来源包装为稳定的记录标识、数据类别、时间、来源引用和变更类型。
4. 通过不透明 checkpoint 保存 Provider 私有的增量读取状态。
5. 将文件系统变更转换为经过重新扫描确认的 `ChangeBatch`。
6. 在缺少文件、记录损坏、格式变化或 checkpoint 失效时返回可处理的诊断。

### 2.2 不负责

- 不修改、迁移、修复或删除 Agent 的源数据；
- 不读取认证文件、凭据或系统 Keychain；
- 不负责消费者的业务数据库、统计指标、异常分析或 UI；
- 不将一段 Agent 数据自动解释为 Requirement、Workflow 或
  Capability Invocation；
- 不保证未知 Provider schema 自动获得完整语义；
- 不负责将敏感 payload 上传、脱敏或导出。

Provider-neutral 指公共访问协议保持中立，不表示所有 payload 已经转换为统一业务
语义。`DataRecord` 提供统一 envelope，而 `payload` 仍可保留 Provider 原生字段。
消费者应在自己的 adapter 中将这些记录映射成业务 canonical facts。

## 3. 分层与依赖方向

```text
Desktop app / CLI / analytics / indexer / viewer
                         |
                         v
              Provider-neutral Rust API
 AgentDataProvider / ChangeBatch / DataRecord / Checkpoint
                         |
                         v
                  Provider implementations
             codex / future claude / future ...
                         |
                         v
       Provider-specific local SQLite / JSONL / artifacts
```

只有具体 Provider 可以依赖对应 Agent 的目录名、表名、文件格式和 JSON 字段。
消费者只依赖公共模型和 trait。这样 Agent 的存储格式变化不会传播到桌面 UI、
CLI 或分析层。

当前代码位于同一个 crate 中，并通过目录隔离公共 API 与具体实现：

```text
crates/coding-agent-data/src/
  lib.rs                  # 稳定导出面
  model.rs                # record、change、checkpoint、diagnostic
  provider.rs             # Provider trait 与 capability
  watch.rs                # 通用 watcher handle/options
  providers/
    codex/
      discovery.rs        # Codex 数据目录发现
      scanner.rs          # SQLite、JSONL、zstd 与增量 checkpoint
      watcher.rs          # 文件监听、debounce 与 reconciliation
```

在 API 仍快速演进、Provider 共享同一组公共模型的阶段，将 API 与实现放在一个
crate 中可以降低发布和版本协调成本。若未来某个 Provider 形成独立发布节奏、
依赖体量或维护团队，再将其升级为单独 crate。

## 4. 公共 API

### 4.1 Provider

`AgentDataProvider` 是同步扫描边界：

```rust
pub trait AgentDataProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;
    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<ChangeBatch>;
}
```

- `scan(None)` 从空 checkpoint 开始读取当前快照；
- `scan(Some(checkpoint))` 只返回该 Provider 识别到的后续变化；
- `has_more=true` 时，消费者应继续用新 checkpoint 调用 `scan`；
- checkpoint 是可序列化但语义不透明的 Provider 私有状态。

支持实时监听的 Provider 实现 `WatchableAgentDataProvider`。`watch` 接收已经成功
处理的 checkpoint，并返回连续的 `ChangeBatch`。

### 4.2 统一变更模型

```text
DataRecord
  key          Provider 范围内稳定的记录标识
  kind         session / event / artifact / usage / goal / memory / ...
  timestamp    可选的统一时间表达
  source       Provider、路径以及 SQLite row 或 JSONL 行位置
  payload      Provider 原生或已部分规范化的数据

DataChange
  Upsert       新增或更新记录
  Delete       删除单条稳定记录
  ResetSource  来源已被重写，消费者应重建该来源
  RemoveSource 来源已经消失
```

公共枚举使用 `#[non_exhaustive]`，消费者必须允许未来增加数据类别、来源位置和
变更类型。

## 5. 当前 Codex Provider

### 5.1 数据源发现

Codex home 按以下顺序解析：

1. `CODEX_HOME`；
2. 默认的 `~/.codex`。

SQLite home 按以下顺序解析：

1. Codex home 下 `config.toml` 的 `sqlite_home`；
2. `CODEX_SQLITE_HOME`；
3. Codex home。

调用方也可以通过 `CodexSource::from_paths` 显式指定两个目录，避免依赖进程环境。

### 5.2 当前读取范围

| 来源 | 当前输出 |
| --- | --- |
| `state_5.sqlite` 的 `threads` | `DataKind::Session`，保留完整 row payload 和 SQLite row 来源引用 |
| `sessions/**/rollout-*.jsonl` | `DataKind::Event`，保留 JSON payload、行号和安全 byte range |
| `archived_sessions/**/rollout-*.jsonl` | 与活跃 rollout 相同的事件变更 |
| `.jsonl.zst` rollout | 解压后按行输出事件与行号引用 |

Provider 不读取 `auth.json` 等凭据文件。目前也不宣称支持 Codex 的其他 SQLite、
日志、目标或记忆数据；这些能力需要以后基于真实格式单独实现和测试。

### 5.3 增量一致性

- SQLite thread 使用完整 row fingerprint 识别新增、修改和删除；
- 普通 JSONL 只在读到完整换行后推进安全 byte offset；
- JSONL 尾部 fingerprint 用于识别 checkpoint 之前的截断或重写；
- 压缩 rollout 使用文件 signature 与已处理行号；
- 来源被重写时输出 `ResetSource`，来源消失时输出 `RemoveSource`；
- 单批记录数、单行大小和诊断数量均有上限。

文件系统事件只是“可能发生变化”的提示。Watcher 会在 debounce 后重新扫描，
启动时立即 reconciliation，并按周期再次 reconciliation，以覆盖事件合并、丢失
或 scan/watch 切换窗口。因此消费者应以 `ChangeBatch` 和 checkpoint 为准，不能
直接把原始文件系统事件当成数据事实。

## 6. 消费者接入约定

推荐生命周期：

```text
discover or construct provider
  -> scan(None)
  -> apply ChangeBatch
  -> persist checkpoint
  -> while has_more: scan(checkpoint)
  -> watch(last successfully persisted checkpoint)
  -> apply each batch and atomically advance checkpoint
```

关键约束：

1. 只有在一批变化成功处理后才能持久化它返回的 checkpoint。
2. `ResetSource` 必须触发对应来源的局部重建，不能与旧记录直接拼接。
3. `RemoveSource` 必须按消费者自己的保留策略清理或标记来源数据。
4. diagnostics 需要进入可观测日志或诊断界面，但日志不应泄露 payload 和绝对路径。
5. checkpoint 丢失后允许重新全量扫描；稳定 key 和变更语义用于保证消费者幂等。

HarnessLens 当前在 Tauri 后台启动 Codex 扫描与 watcher，并记录批次计数和诊断。
监控层在 `~/.harness-lens/agent-data-checkpoint.json` 保存最后成功处理的 checkpoint，
从而在应用重启后继续增量扫描；文件缺失、损坏或与当前数据源不兼容时会自动从
空 checkpoint 重建。
将 `ChangeBatch` 映射并持久化为 HarnessLens canonical facts，仍属于后续的应用层
接入工作，不能把“已监听”描述成“业务数据已经完成导入”。

## 7. 安全与可复用性

- SQLite 使用 read-only 与 query-only 模式；
- 源文件原地读取，不执行写回或 migration；
- 默认不访问网络；
- crate 不拥有消费者的配置目录或业务数据库；
- 日志不得默认输出 prompt、代码、工具参数、完整 payload 或凭据；
- 测试 fixture 应使用合成或彻底脱敏的数据。

这些边界使同一个 crate 可以安全地被多个本地工具复用，同时让调用方自行决定
数据保留、隐私、索引、分析和展示策略。

## 8. 与 HarnessLens 的关系

```text
coding-agent-data ChangeBatch
  -> HarnessLens application adapter
  -> canonical source/session/event/raw record
  -> capability correlation
  -> local analytics store
  -> application API / UI
```

`coding-agent-data` 提供本机 Agent 数据变更，HarnessLens 再负责业务语义、
持久化和分析：

- HarnessLens UI 不读取 Codex 私有表和 JSON 字段；
- Capability Invocation 只由显式事件或经过验证的规则产生；
- session 汇总 Token 不能被无依据地分摊到 capability invocation。

相关文档：

- [HarnessLens 总体架构](./architecture.md)
- [HarnessLens V1 架构](./mvp-architecture.md)
- [HarnessLens SQLite 数据库架构](./sqlite-database-architecture.md)
