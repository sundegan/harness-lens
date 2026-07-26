# HarnessLens SQLite 数据库架构

## 1. 结论

HarnessLens 的 SQLite 不应成为 Codex、Claude Code 或 Gemini 私有数据结构的镜像。它应是一个 provider-neutral 的本地分析库，按以下三层组织：

```text
Coding Agent local data
        |
        v
Agent Provider / Adapter
        |
        v
┌──────────────────────────────────────────────────────┐
│ 1. 来源与导入层                                      │
│ agent_providers / data_sources / import_runs         │
│ source_artifacts / raw_records / parser_diagnostics  │
├──────────────────────────────────────────────────────┤
│ 2. 统一事实层                                        │
│ projects / sessions / turns / events / messages      │
│ model_calls / usage_observations                     │
│ capabilities / capability_invocations / evidence     │
├──────────────────────────────────────────────────────┤
│ 3. 可重建分析层                                      │
│ analysis_runs / metric_points / anomaly_candidates   │
└──────────────────────────────────────────────────────┘
        |
        v
HarnessLens Application API / UI
```

底层 Agent 结构变化只影响相应 Provider。上层查询始终面对 `Session`、`Event`、`CapabilityInvocation` 等稳定事实，不读取 Codex 表名、Claude JSON 字段或 Gemini 日志类型。

可执行的 V1 基线 DDL 位于 [sql/harness-lens-v1.sql](./sql/harness-lens-v1.sql)。

## 2. 设计原则

### 2.1 Agent 无关

数据库中不创建 `codex_sessions`、`claude_messages` 或 `gemini_tool_calls`。不同 Agent 只通过 `agent_providers` 和 `data_sources` 标识来源。

Provider 私有字段只有三种去向：

1. 能形成跨 Agent 共同语义的字段，写入明确的 canonical 列；
2. 暂时不参与核心查询的扩展信息，写入 `metadata_json` 或 `payload_json`；
3. 无法理解但需要追踪的内容，保留 `raw_records` 引用并产生诊断。

如果某个 JSON 扩展字段成为核心统计条件，应通过数据库迁移提升为有类型的列，不能长期依赖 `json_extract` 实现主查询。

### 2.2 事实与统计分离

`sessions`、`events`、`capability_invocations` 和 `usage_observations` 是事实；`metric_points` 和 `anomaly_candidates` 是派生结果。

事实层保存稳定来源、数据质量和导入版本。分析层绑定 `analysis_run` 与 `derivation_version`，解析规则、计价规则或聚合算法变化后可以整体失效和重算。

### 2.3 Invocation 是统计主键

Session 是 Agent 的执行容器和下钻证据，不是 HarnessLens 的唯一统计对象。V1 的关键关系是：

```text
Capability 1 ─── N Capability Invocation
Session    1 ─── N Capability Invocation
Invocation 1 ─── N Evidence
```

一次 Invocation 表示一次 Skill、MCP Tool、workflow step 或其他 Harness 能力调用。`success` 仅表示调用成功结束，不代表整个需求交付成功。

### 2.4 精度显式化

所有可能被转换或推断的事实使用统一 `quality`：

| 值 | 含义 |
| --- | --- |
| `observed` | 来源明确提供 |
| `derived` | 由明确事实确定性计算 |
| `estimated` | 在受控边界内估算 |
| `inferred` | 由规则或上下文推断 |
| `projected` | 来自非原始投影或摘要 |
| `unknown` | 不能可靠判断 |

统计还需要保存 `covered_sample_count / total_sample_count`。例如只有 40% 的 Invocation 可以可靠归因 Token，UI 必须展示覆盖率，不能将剩余 60% 当作零。

### 2.5 本地隐私优先

绝对路径、prompt、代码、命令参数和工具输出都可能敏感：

- `data_sources.privacy_mode=reference_only` 时仅保存来源 locator、hash 和位置；
- `raw_records.payload_json` 与 `messages.content_text` 均允许为空；
- 项目与工作目录同时提供原值和 hash 字段，团队导出时可以只使用 hash；
- Provider 不应将凭证文件、Keychain 内容或认证信息导入 HarnessLens。

## 3. 表分组与职责

### 3.1 来源与导入层

| 表 | 职责 | 关键唯一键 |
| --- | --- | --- |
| `agent_providers` | Provider 类型，例如 Codex、Claude Code、Gemini | `id` |
| `data_sources` | 某台机器上的一个实际数据源 | `(agent_provider_id, locator_hash)` |
| `import_runs` | 一次导入尝试、parser 版本和完成状态 | `id` |
| `source_artifacts` | 一个 SQLite、JSONL、日志文件或逻辑对象 | `(data_source_id, artifact_key)` |
| `raw_records` | 行、SQLite row 或事件级来源引用/可选快照 | `(source_artifact_id, source_position)` |
| `parser_diagnostics` | 兼容性、字段缺失、部分读取和解析错误 | `id` |

这里的 `source_artifacts` 不假设数据一定是文件。Provider 可以将 SQLite 表、API page 或压缩成员映射为逻辑 artifact；`source_position` 可以是 JSONL byte offset、SQLite 主键或其他稳定 cursor。

`raw_records` 主要用于幂等和证据定位，不要求复制完整原始数据。需要快照时，纯文本写入 `payload_text`，结构化内容写入 `payload_json`；`content_fingerprint` 用于判断同一位置的内容是否被重写。

### 3.2 统一事实层

| 表 | 职责 |
| --- | --- |
| `projects` | 跨来源的项目/仓库身份 |
| `sessions` | Agent 执行容器 |
| `session_relations` | fork、spawn、resume、continuation 等关系 |
| `turns` | 用户轮次或 Provider 能稳定识别的执行轮次 |
| `events` | 有序 canonical timeline，`kind` 是开放字符串 |
| `messages` | 对消息类 Event 的查询投影，可按隐私设置不保存正文 |
| `models` / `model_calls` | 模型身份与一次模型调用 |
| `usage_observations` | Token/成本观察值及其归属范围 |
| `capabilities` | Skill、MCP Tool、Tool、Workflow 等稳定能力定义 |
| `capability_invocations` | 一次能力调用，是 V1 分析主事实 |
| `invocation_evidence` | Invocation 到 Event/Raw Record 的可下钻证据 |
| `model_prices` | 有生效时间和版本的本地计价表 |

`events.kind` 不使用封闭枚举。Provider 可以输出稳定的通用类型，例如：

```text
message
model_call
tool_call
tool_result
command
file_change
usage
error
compaction
session_state
unknown
```

未知类型仍可写入 `events(kind='unknown', payload_json=...)` 并生成 warning，避免 Agent 升级后整段数据被静默丢弃。

### 3.3 可重建分析层

| 表 | 职责 |
| --- | --- |
| `analysis_runs` | 一次确定版本的统计重建 |
| `metric_definitions` | 指标名称、单位、值类型和聚合口径 |
| `metric_points` | 按主体、时间窗和维度保存的可失效快照 |
| `anomaly_rules` | 版本化异常候选规则 |
| `anomaly_candidates` | 规则产生的待调查信号 |

`metric_points.subject_type + subject_id` 是有意使用的多态分析键，因为这些行只是缓存，不是业务事实。应用写入时必须验证：

- `project` 指向 `projects.id`；
- `session` 指向 `sessions.id`；
- `capability` 指向 `capabilities.id`；
- `model` 指向 `models.id`；
- `agent_provider` 指向 `agent_providers.id`；
- `global` 使用空 `subject_id`。

这样可以避免为每种指标创建一套几乎相同的表。事实层仍然全部使用真实外键。

## 4. 关键关系

```text
agent_providers
    └── data_sources
          ├── import_runs
          ├── source_artifacts ── raw_records
          └── sessions
                ├── turns ── events ── messages
                ├── model_calls ── usage_observations
                └── capability_invocations
                       ├── invocation_evidence
                       └── usage_observations

capabilities ── capability_invocations
projects     ── sessions
models       ── model_calls ── model_prices

analysis_runs
    ├── metric_points
    └── anomaly_candidates ── anomaly_rules
```

## 5. Provider 输出与 Store 写入契约

`coding-agent-data` Provider 不返回 Provider 的数据库连接，而是返回稳定的
Provider-neutral 变更批次：

```rust
struct ChangeBatch {
    changes: Vec<DataChange>,
    checkpoint: Checkpoint,
    diagnostics: Vec<Diagnostic>,
    has_more: bool,
}
```

其中 `DataRecord` 统一 key、kind、timestamp 和 source reference，但 payload
允许保留 Provider 原生字段。HarnessLens application adapter 先把记录转换为
`SessionFact`、`EventFact`、`RawRecord` 等 canonical facts，Store 再负责
事务化写入 SQLite。推荐导入流程：

1. 创建并提交 `import_runs(status='running')`，以便崩溃后识别未完成任务；
2. 在短事务中 upsert `source_artifacts`、`raw_records` 和一个 session 的 canonical facts；
3. 同一批次内写入 evidence，更新计数与 checkpoint；
4. 所有批次结束后，用最终事务将 import run 标记为 `succeeded` 或 `partial`；
5. 创建新的 `analysis_run`，重建受影响时间窗的指标。

`coding-agent-data` 负责发现、读取、解析、通用变更规范化、增量 checkpoint
和监听，不执行业务映射或聚合 SQL。HarnessLens application adapter 负责业务
映射，Store 负责数据库写入。

## 6. 幂等、增量和数据修订

### 6.1 幂等键

核心幂等键如下：

```text
data source:  agent_provider_id + locator_hash
artifact:     data_source_id + artifact_key
raw record:   source_artifact_id + source_position
session:      data_source_id + external_session_id
turn:         session_id + ordinal
event:        session_id + sequence
invocation:   session_id + ordinal
metric point: analysis_run + metric + subject + period + dimensions_hash
```

内部 `id` 建议使用 UUID v7 或 ULID；不要将 Codex/Claude/Gemini 的 ID 直接当作全局主键。

### 6.2 增量读取

Provider 将最后安全 cursor 编码在不透明 `Checkpoint` 中。HarnessLens 只在成功
应用对应 `ChangeBatch` 后，将序列化 checkpoint 保存到
`import_runs.checkpoint_json`。对 append-only artifact，Provider 只解析新增部分。

cursor 不是事实，丢失后允许从头读取；唯一键和 fingerprint 必须保证重复导入不会产生重复事实。

`data_sources` 采用显式清理而非数据库级联删除。正常“停用来源”只设置 `enabled=0`；若用户要求彻底清除，Store 必须在一个受控事务中按 facts → artifacts → import runs → data source 的顺序删除，避免误删整套历史证据。

### 6.3 原始数据被重写

Agent 的 compact、resume、归档或升级可能改变之前的数据。若同一个 `source_position` 的 fingerprint 改变：

1. 不进行跨整个数据库的危险覆盖；
2. 标记对应 session 需要重建；
3. 在单 session 事务中删除其旧 child facts 后重新导入；
4. 保留 import run 与诊断；
5. 使受影响的 analysis run 失效并重算。

对无法安全识别边界的变化，将导入标为 `partial`，不要把旧事实和新事实拼接成看似完整的 timeline。

## 7. Token、成本与重复计数

`usage_observations.scope` 明确区分：

```text
session
turn
model_call
capability_invocation
```

不同 scope 可能描述同一批 Token，不能直接跨 scope 求和。推荐聚合原则：

- 模型消耗总量优先聚合不重叠的 `model_call` observation；
- 只有 session 总量时保留 `scope=session`，不平均分摊给 Invocation；
- Invocation 有显式边界或来源直接提供时才写 invocation scope；
- 由时间窗归因时标记 `estimated` 并保留 `derivation_version`；
- 成本由 `model_prices` 计算时保存 `pricing_version`，价格变化后可重算。

## 8. SQLite 运行配置

每个连接都应执行：

```sql
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
```

应用初始化数据库时建议执行：

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
```

写入模型采用“单 writer + 短事务”，读取连接保持只读。不要让 Provider 持有 HarnessLens 数据库连接；Provider 读取 Agent 数据源，Store 独立写入 HarnessLens SQLite。

迁移应使用按版本排序、只前进的 migration 文件，而不是运行时根据表是否存在临时补列。数据库启动时检查 `schema_metadata.schema_version`，高于当前应用支持版本时拒绝写入。

## 9. V1 指标映射

| 产品指标 | 事实来源 | 推荐算法 |
| --- | --- | --- |
| `invocation_count` | `capability_invocations` | `COUNT(*)` |
| `active_projects` | Invocation → Session → Project | `COUNT(DISTINCT project_id)` |
| `active_sessions` | `capability_invocations.session_id` | `COUNT(DISTINCT session_id)` |
| `duration_p50/p90` | `duration_ms` | 仅统计非空值，同时保存覆盖率 |
| `failure_rate` | `status` | `failed / 可判定 invocation` |
| `cancel_rate` | `status` | `cancelled / 可判定 invocation` |
| `retry_rate` | `retry_of_invocation_id` | 有 retry 关系的 invocation 占比 |
| `token_total` | `usage_observations` | 按不重叠 scope 聚合 |
| `cost_total` | usage + model price | 带 pricing version |
| `output_bytes_p90` | `output_bytes` | 仅统计非空值并显示覆盖率 |
| `data_coverage_rate` | metric point coverage columns | `covered / total` |

SQLite 没有内建 percentile 聚合。V1 可以在 Rust 分析层按排序后的样本计算 P50/P90，再写入 `metric_points`；不要用 `AVG` 冒充 P90。

## 10. Rust 模块与 crate 边界

```text
crates/
  coding-agent-data/         # Provider-neutral API + 各 Agent Provider
    src/providers/codex/     # Codex 私有目录、schema、解析和 watcher
  harness-lens-store/        # 后续：SQLite migration/repository/transaction
  harness-lens-analytics/    # 后续：指标、覆盖率、异常候选

src-tauri/
  src/agent_data.rs          # 后台扫描/监听与应用生命周期接入
  src/...                    # application adapter 和 commands
```

依赖方向：

```text
coding-agent-data providers ─> coding-agent-data public model/traits
HarnessLens adapter         ─> coding-agent-data
harness-lens-store          ─> HarnessLens canonical facts
harness-lens-analytics ─> harness-lens-store query API
Tauri commands         ─> application services
```

`harness-lens-store` 不引用 Codex 私有实现；`coding-agent-data` 也不引用
`harness-lens-store`。二者通过 HarnessLens application adapter 和 canonical
facts 连接。Provider 是否在未来拆成单独 crate，应由独立发布节奏、依赖体量和
维护边界决定，而不是预先为每个 Agent 创建 package。

## 11. 实施顺序

1. 使用现有 `coding-agent-data` CodexProvider 完成只读扫描、checkpoint 与监听；
2. 将 SQL 基线变成 `harness-lens-store` 的 migration `0001_initial.sql`；
3. 实现 application adapter 与 import/source/session/event/invocation Repository；
4. 用合成 fixture 和真实脱敏样本验证幂等、source reset 和 raw evidence；
5. 实现 usage 与 model price，验证 Token 不发生跨 scope 重复计数；
6. 增加 analysis run、metric point、P50/P90、anomaly candidates 和 UI 查询。

V1 不必一开始实现每个表的完整 UI，但数据库边界应从第一版就区分事实与缓存，避免未来为接入第二个 Agent 重构主存储。
