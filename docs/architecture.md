# HarnessLens 架构设计

## 1. 项目定位

HarnessLens 是一个本地优先的 AI Coding Harness 使用效能分析工具。它的首要任务是将团队对 workflow、Skill、MCP Tool 和 Agent 的真实使用记录转化为可信统计与可下钻的证据，帮助团队确定最值得改进的 Harness 能力。

第一版不以“完整还原一个业务需求的研发流程”为前提，也不以“查看每个 Agent 的执行过程”为终点。Session timeline 是诊断证据；核心统计对象是 `Capability` 与 `Capability Invocation`。

```text
Agent runtime records
  -> Capability Invocation facts
  -> Capability usage and health metrics
  -> anomaly candidates
  -> evidence drilldown
```

需求级关联、feedback loop 与 Harness Eval 都建立在这些事实之上，但属于后续演进能力。详见 [V1 架构设计](./mvp-architecture.md) 和 [Harness Evolution Loop 演进设计](./harness-evolution-observability.md)。

## 2. 设计目标

- **本地优先**：默认读取并保存本机执行数据；敏感原始内容不自动上传。
- **事实优先**：名称、时间、状态、错误、原始引用等事实与推断、人工反馈分层保存。
- **能力优先**：先分析 Skill、MCP Tool 与 workflow 的使用效能，不伪造需求级结果。
- **Codex-first**：先稳定支持 Codex 本地记录和结构化 Harness 事件。
- **Adapter-ready**：不同 Agent 的数据格式通过 adapter 隔离，UI 不依赖私有日志 schema。
- **精度透明**：每个字段、聚合和 Token 归属保留来源、计算方式和数据质量。
- **证据可追溯**：从能力排行和异常直接下钻到 invocation、session 与 raw record。

## 3. 总体架构

```text
┌───────────────────────────────────────────────────────────────┐
│                     HarnessLens UI                             │
│ Overview / Capability list / Detail / Invocation / Timeline    │
└──────────────────────────────▲────────────────────────────────┘
                               │
┌──────────────────────────────┴────────────────────────────────┐
│                    Application services                         │
│ query metrics / import / refresh / diagnostics / raw evidence  │
└──────────────────────────────▲────────────────────────────────┘
                               │
┌──────────────────────────────┴────────────────────────────────┐
│                      Analytics core                             │
│ capability registry / correlation / aggregation / anomaly rules │
└──────────────────────────────▲────────────────────────────────┘
                               │
┌──────────────────────────────┴────────────────────────────────┐
│                       Local store                               │
│ capabilities / invocations / sessions / events / raw records   │
└──────────────────────────────▲────────────────────────────────┘
                               │
┌──────────────────────────────┴────────────────────────────────┐
│              Local data access and ingestion                    │
│ coding-agent-data Providers | Harness hook/wrapper/events       │
└───────────────────────────────────────────────────────────────┘
```

`coding-agent-data` 负责发现、只读解析、增量跟踪并监听本机 Coding Agent
数据，通过 Provider-neutral 的 `ChangeBatch` 隔离私有存储格式。HarnessLens
application adapter 再将这些记录和显式 Harness 事件转换为 canonical facts。
UI 和聚合查询只消费 canonical facts，不直接读取 Codex rollout 或其他 Agent
私有数据。

## 4. 分层职责

### 4.1 Local Data Access

可复用 crate [`coding-agent-data`](../crates/coding-agent-data) 负责访问本机
Coding Agent 数据。首个 `CodexProvider` 读取 `state_5.sqlite`、
`rollout-*.jsonl` 和 `.jsonl.zst`，并监听活跃及归档 rollout 的变化。

公共边界包括：

```text
ProviderDescriptor  声明 Provider 身份与 discover/scan/watch 能力
AgentDataProvider   从空状态或不透明 checkpoint 扫描变化
DataRecord          统一 key、kind、timestamp、source 和 payload envelope
DataChange          upsert/delete/reset-source/remove-source
Diagnostics         暴露缺失文件、解析失败和 checkpoint 降级
```

该 crate 的规范化止于通用数据与变更协议；Provider 原生 payload 不等于
HarnessLens 业务事实。application adapter 负责将其映射为 canonical
session/event/raw record，再执行 capability correlation。后续 Claude Code、
Gemini CLI、OpenCode 等 Provider 满足相同边界，不能把私有类型泄漏到上层。

完整边界与当前实现见 [`coding-agent-data` 定位与架构](./coding-agent-data.md)。

### 4.2 Capability Correlation

负责将原始事件关联为一次明确的能力调用。

优先级为：

1. Harness workflow runner、Skill wrapper 或 MCP wrapper 写入的显式 `invocation_id`；
2. Agent runtime 中结构明确的 MCP tool call/result；
3. 受控规则推断的 Skill 使用，必须标记数据质量和证据。

这层不负责推断业务需求是否完成，也不把 Skill 成功返回解释为业务成功。

### 4.3 Analytics Core

负责可靠聚合与排序：

- 调用量、使用趋势、项目/session 覆盖；
- P50/P90 耗时、失败、取消和重试率；
- 可获得时的 Token、成本、输出大小和覆盖率；
- 基于同类 capability 历史数据的异常候选；
- 聚合指标与源事实之间的链接。

异常规则只能提出“值得调查”的信号。例如高 P90 耗时不等于 MCP 本身有缺陷；它需要保留调用样本供人判断。

### 4.4 Store

Store 使用本地 SQLite，持久化 canonical facts、导入状态和可失效的指标快照。

```text
harness-lens.sqlite
  agent_providers
  data_sources
  import_runs
  source_artifacts
  raw_records
  sessions / turns / events
  capabilities
  capability_invocations
  usage_observations
  parser_diagnostics
  analysis_runs / metric_points
  anomaly_candidates
```

数据库按“来源与导入 → 统一事实 → 可重建分析”三层组织；完整表结构、约束与 V1 DDL 见 [SQLite 数据库架构](./sqlite-database-architecture.md)。

关键约束：

- `raw_records` 是事实追溯入口，不将完整敏感内容复制到每个聚合表；
- `capability_invocations` 存储调用边界和质量，不用 session 总 Token 填充未知的能力 Token；
- `metric_points` 可随 parser、计价或聚合规则变化失效并重算；
- 所有派生数据携带 `derivation_version` 和 `evidence_refs`。

### 4.5 Application Services

通过 Tauri command 或内部 service 提供稳定 API：

```text
list_capabilities(filter)
get_capability_metrics(capability_id, range)
list_invocations(filter)
get_invocation(invocation_id)
get_session(session_id)
import_source(adapter_id, source)
refresh_source(source_id)
get_diagnostics(source_id)
```

UI 不直接访问原始文件、SQLite 或 adapter。

### 4.6 UI

页面按“发现问题 -> 判断可信度 -> 查看证据”组织：

- Overview：调用总量、数据覆盖率、Top capability、异常候选；
- Capability list：Skill/MCP Tool 的筛选、排序和趋势；
- Capability detail：耗时、状态、Token/成本（如可用）、版本和样本分布；
- Invocation detail：单次调用边界、session、错误、关联事件与 raw reference；
- Session timeline：解释某次调用的前后上下文；
- Diagnostics：解析失败、缺失字段与数据质量。

## 5. Canonical Data Model

### 5.1 Capability

```text
id
type                 skill | mcp_tool | workflow_step
name                 Skill 名称或 server/tool
version              可空；来自 Harness 或 registry
source               explicit | adapter | inferred
metadata
```

### 5.2 Capability Invocation

```text
id
capability_id
adapter_id
source_invocation_id
session_id
project / repository
started_at / ended_at / duration_ms
status               running | success | failed | cancelled | unknown
error_type
retry_of
token_scope          observed | estimated | unknown
token_summary
cost_scope           observed | estimated | unknown
cost_summary
confidence
raw_refs
```

`status=success` 只表示调用层执行完成，不代表需求、研发节点或最终产物成功。

### 5.3 Session 与 Event

Session 保存 Agent 原始执行容器的元数据。Event 保存 command、MCP tool call/result、token usage、error、compaction 等细粒度事实。它们是 Invocation 和统计结果的证据来源，不是产品的唯一统计主键。

### 5.4 数据质量

```text
observed   原始记录或显式事件直接提供
derived    由明确字段计算，例如 end - start
estimated  有受限边界或模型价格表支持的估算
inferred   由命名、文本或上下文规则推断
unknown    无法可靠取得
```

`estimated` 与 `inferred` 不得在 UI 中伪装成精确事实；聚合页需要同时显示对应指标的数据覆盖率。

## 6. Token 和成本归因

Token 与成本是高价值指标，也是最容易被误导的指标。

```text
可直接关联的模型调用 Token -> observed
可由明确 invocation 时间窗聚合的 session token -> estimated
仅有整段 session token 且调用边界不清 -> unknown
```

MCP 调用的耗时和状态通常可由 call/result 关联；其 Token 并不必然独立存在。Skill 可能跨多轮模型、MCP 和命令调用，只有显式开始/结束边界或运行时提供关联 ID 时才可计算调用级 Token。

## 7. Adapter 路线

### 阶段 1：Codex Adapter

- 从 `state_5.sqlite` 读取 session 元数据；
- 解析 rollout JSONL 的 MCP、command、token、错误和时间线事件；
- 导入显式 Harness capability 事件；
- 保留 source path、offset、fingerprint 和 parser version。

### 阶段 2：Adapter Registry

当需要第二个 Agent 时，引入 `AgentAdapter` trait、capability declaration 和 adapter diagnostics；UI 按 `adapter_id` 筛选。

### 阶段 3：多 Agent 与 OTel

OTel 可作为统一事件输入和补充数据源，但不取代 capability 语义层。没有显式 capability 标识的 OTel span 只能提供 runtime trace，不应自动转成 Skill 使用事实。

## 8. 隐私与数据边界

默认本地保存原始 prompt、命令参数、代码内容和 MCP 输出。团队级汇总若被引入，只发送必要的脱敏结构化字段，例如能力名、版本、时间、状态、Token、成本、错误分类和 hash 化引用。

导出或上传前必须支持：

- 路径、仓库、分支和用户标识的脱敏；
- prompt、命令参数、MCP 输出的排除或摘要；
- 记录保留期限和本地清理；
- 显式的用户/团队授权。

## 9. 验证路线

在扩展产品前，先验证：

1. Skill/MCP 名称和调用边界能否在真实 Harness 中稳定识别；
2. 调用耗时、状态、错误和原始证据能否相互校验；
3. Token/成本的覆盖率能否透明呈现；
4. 是否能从一个异常候选导出一个可验证的 Harness 改进假设。

若这些假设不成立，应收窄采集范围或增加显式事件，而不是用更复杂的推断和 Dashboard 掩盖数据缺口。

## 10. 后续需求级与改进闭环

当团队已具备稳定的 requirement、workflow、artifact 和人工 checkpoint 生命周期事件时，可在上述模型之上增加：

```text
Requirement
  -> Workflow Run
    -> Node Run
      -> Attempt
        -> Capability Invocation / Session / Event
```

它支持一次通过率、返工、需求类型和 Harness 版本效果等更高价值指标，但不改变 V1 的事实模型。详见 [需求级关联设计](./requirement-level-observability.md)。
