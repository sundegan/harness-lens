# HarnessLens V1 架构设计

## 1. V1 目标与边界

V1 的目标不是还原完整研发需求流程，而是建立可信的 Harness 使用事实：团队如何使用 Skill 与 MCP Tool，这些调用花了多久、消耗多少 Token、是否失败或重试，以及证据在哪里。

V1 的统计主键是 `Capability Invocation`，而不是 Requirement、Session 或 Skill 文本名称。

```text
Capability
  -> Invocation
       -> Runtime event / session segment / raw record
```

`Capability` 是一个可配置、可版本化的 Harness 能力，例如某个 Skill 或 `server/tool` 形式的 MCP Tool。`Invocation` 是该能力的一次明确调用。

### V1 包含

- 通过 `coding-agent-data` 发现、增量读取并监听 Codex 本机数据；
- 将 Codex session 与 rollout 记录导入 HarnessLens canonical facts；
- 结构化 Skill/MCP capability 事件的接入；
- 调用量、使用趋势、项目/session 覆盖、耗时、状态、失败、重试统计；
- 可获得时的 Token、成本、输出大小和错误分类；
- 聚合页、筛选、异常候选和原始证据下钻；
- SQLite 本地存储、解析诊断与数据质量标识。

### V1 不包含

- 需求、TD、编码、Review 的自动节点识别；
- 一次通过率、需求返工率或完整研发交付耗时；
- 根据 transcript 自动判定人工介入原因；
- 自动生成或修改 Skill、KB、linter；
- 完整 feedback loop、eval 平台和云端团队协作；
- 一次接入所有 AI Coding Agent。

## 2. 最小架构

```text
Codex local data                 Harness hook / wrapper / runner
        |                                      |
        v                                      |
 coding-agent-data CodexProvider               |
 discover / parse / checkpoint / watch         |
        |                                      |
        +-------------+------------------------+
                      v
        HarnessLens application adapters
          normalize / diagnostics / import
                      v
           Capability correlation
  explicit event first, transcript inference fallback
                      v
             Local SQLite store
 capability / invocation / raw record / aggregate
                      v
               Application API
                      v
       Capability analytics and evidence UI
```

V1 优先接入 Codex，但不把 UI 与存储绑定到 Codex 私有 JSONL schema。
`coding-agent-data` 以 Provider-neutral record/change API 隔离数据访问；
HarnessLens application adapter 负责转换业务 canonical facts。其他 Agent
通过新的 Provider 扩展。

## 3. 采集策略

### 3.1 优先级

1. **显式 capability 事件**：Harness runner、Skill wrapper 或 MCP wrapper 在调用边界写入事件。
2. **运行时结构化事件**：例如 Codex rollout 中明确的 MCP tool call/result。
3. **受控推断**：仅在命名规则、日志结构和上下文足以支持时推断 Skill 使用，并标记质量。

第一优先级是 V1 的关键前提。没有显式 Skill 事件时，V1 仍可先完整统计 MCP Tool，并将 Skill 统计限制为“可识别的调用”，而不是假装全覆盖。

### 3.2 最小事件契约

```json
{
  "event": "harness.capability.completed",
  "invocation_id": "inv-01J...",
  "capability": {
    "type": "skill",
    "name": "technical-design",
    "version": "git-sha"
  },
  "session_id": "agent-session-id",
  "project": "marketing-service",
  "started_at": "2026-07-22T10:00:00Z",
  "completed_at": "2026-07-22T10:01:12Z",
  "status": "success",
  "error_type": null,
  "raw_ref": "local://events/2026-07-22.jsonl#42"
}
```

MCP Tool 的 `capability.name` 使用稳定的 `server/tool` 格式。调用参数、输出和 prompt 默认只保留本地 raw reference 或脱敏摘要。

### 3.3 Token 与成本口径

不能把一个 session 的总 Token 平均分摊给其中所有 Skill/MCP 调用。

| 数据 | V1 口径 |
| --- | --- |
| MCP Tool 耗时、状态、错误 | 通常可由 call/result 明确得到，标为 `observed` 或 `derived`。 |
| MCP Tool 输出大小 | 可由原始结果计算，标为 `derived`。 |
| 单次 MCP 调用直接 Token | 仅在运行时明确提供时记录，否则为 `unknown`。 |
| Skill 调用耗时、状态 | 需要显式事件；没有时只提供受限推断。 |
| Skill Token | 可用明确的 start/end session segment 聚合时标为 `estimated`；没有可靠边界时为 `unknown`。 |
| 成本 | 基于已知模型价格和 Token 字段计算，必须保留计价版本与精度。 |

## 4. 数据模型

```text
capabilities
  id, type, name, version, source, metadata

invocations
  id, capability_id, adapter_id, source_invocation_id,
  session_id, project, repository, started_at, ended_at, duration_ms,
  status, error_type, retry_of, token_scope, cost_scope,
  confidence, raw_ref

events
  id, invocation_id, session_id, kind, status,
  started_at, ended_at, data_json, confidence, raw_ref

sessions
  id, adapter_id, source_session_id, project, cwd,
  started_at, ended_at, model, token_summary, raw_ref

raw_records
  id, adapter_id, source_path, source_offset, fingerprint, imported_at

import_runs
  id, adapter_id, source, status, parser_version, diagnostics, timestamps
```

这里使用的是概念名称；实际 SQLite 使用 `capability_invocations`、`usage_observations`、`analysis_runs` 和 `metric_points` 等明确表名。完整定义见 [SQLite 数据库架构](./sqlite-database-architecture.md)。

`capability_invocations` 只保存一次能力调用的事实与可追溯引用。聚合指标应按查询计算或保存为可失效的快照，不能取代原始事实。

## 5. 指标与异常候选

V1 仅提供能从事实层可靠得出的指标：

- `invocation_count`：调用次数；
- `active_projects`、`active_sessions`：使用覆盖；
- `duration_p50`、`duration_p90`：调用耗时；
- `failure_rate`、`cancel_rate`、`retry_rate`：运行状态；
- `token_total`、`cost_total`：仅在对应 scope 可用时展示；
- `output_bytes_p90`：MCP 输出大小；
- `data_coverage_rate`：每项指标的可用数据比例。

异常候选是排序建议，不是根因结论。例如：

```text
高频且高耗时：调用量处于前 20%，P90 耗时高于同类能力基线。
高失败率：可判定调用超过最低样本数，失败率显著高于同类基线。
高重试：同一 capability 在短窗口内重复调用，且前次状态为失败或超时。
高输出：MCP 输出 P90 超出阈值，可能造成后续上下文成本。
```

所有异常必须可以下钻到 invocation 与 raw evidence；无法证明的因果关系只作为待确认假设。

## 6. 页面与查询

V1 页面按“从分析到证据”的路径组织：

1. **Overview**：总调用量、可观测覆盖率、Top capability、异常候选。
2. **Capability list**：按 Skill/MCP、项目、Agent、时间和版本筛选。
3. **Capability detail**：趋势、P50/P90、状态分布、调用样本和数据质量。
4. **Invocation detail**：开始/结束、关联 session、原始事件、错误与 raw reference。
5. **Session timeline**：仅用于解释某次调用前后的执行上下文，不作为主统计维度。

## 7. MVP 实施顺序

1. 定义 capability 与 invocation schema，提供本地 JSONL writer/reader。
2. 完成 `coding-agent-data` CodexProvider 的增量读取与监听，并由 HarnessLens
   adapter 映射 session、MCP tool call/result、Token 与原始引用。
3. 由一个真实 Skill 或 workflow runner 发出显式 Skill invocation 事件。
4. 写入 SQLite，完成 capability 列表、详情与下钻。
5. 用真实团队样本验证至少一个异常候选能导向具体改进动作。

## 8. 验收标准

V1 的验收不是“有一个统计大盘”，而是满足以下条件：

- 对选定的 Skill/MCP Tool，调用名称和边界可稳定识别；
- 每次可识别调用至少具备 `name + type + time + status + session/project + raw_ref`；
- 耗时、失败和重试可在聚合页与原始记录之间互相校验；
- Token/成本指标显示覆盖率与精度，不输出虚假的精确归因；
- 团队能利用一个异常样本，提出并跟踪一个具体的 Harness 改进假设。

## 9. 后续演进

当显式 capability 事件、版本信息和结果反馈稳定后，再增加：

```text
Capability Invocation
  -> workflow / node association
  -> requirement association
  -> artifact, Git/CI and human checkpoint
  -> feedback item and eval case
```

需求级关联的设计与约束见 [requirement-level-observability.md](./requirement-level-observability.md)。它是高价值扩展，不是 V1 的数据前提。
