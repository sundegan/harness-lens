# Harness Evolution Loop 演进设计

## 1. 背景与决策

Harness Engineering 已在真实研发中投入使用。下一阶段的目标不是先建设一个大而全的“AI 研发流程可观测平台”，而是建立足够可靠的证据，判断现有 workflow、Skill、MCP Tool 与知识能力中哪些最值得改进。

因此，HarnessLens 的演进顺序为：

```text
使用事实
  -> 使用效能与异常模式
  -> 有条件的需求级归因
  -> feedback 与 eval 验证闭环
```

这避免让 requirement correlation、完整结果判定和 Eval 编排成为第一版的阻塞前提。

## 2. 能力分层

| 层级 | 关注对象 | 当前定位 |
| --- | --- | --- |
| 使用事实层 | Skill、MCP Tool、workflow step、Agent session | V1 必做。 |
| 效能分析层 | 成本、耗时、失败、重试、输出大小、版本差异 | V2 重点。 |
| 需求级关联层 | Requirement、Node Run、Attempt、artifact、人工检查点 | 高价值扩展，非 V1 前提。 |
| 反馈与评测层 | 改进项、eval case、rubric、snapshot、baseline | 在前层数据可信后建设。 |

## 3. V1：建立可验证的使用事实

### 3.1 核心对象

```text
Capability
  -> Capability Invocation
       -> Session / Event / Raw Record
```

- `Capability`：一个可命名、可版本化的 Skill、MCP Tool 或 workflow step。
- `Capability Invocation`：该能力一次可识别的调用。
- `Session/Event`：调用的原始执行容器与证据。

V1 的最小字段为：

```text
capability_type
capability_name
capability_version (optional)
invocation_id
session_id
agent/runtime
project/repository
started_at / ended_at / duration
status / error_type / retry_of
token and cost scope
raw_ref
```

### 3.2 采集原则

显式事件优先：workflow runner、Skill wrapper 或 MCP wrapper 应在调用边界写入结构化事件。Codex rollout 等运行时记录用于补充 MCP、command、Token、错误和 session context。

```text
Harness runner / wrapper
        -> structured capability event
Codex rollout / state
        -> runtime event and session context
        -> HarnessLens normalize and correlate
        -> local SQLite and analytics
```

没有可靠 Skill 边界时，只统计可明确识别的 MCP Tool；Skill 使用可显示为 `inferred` 或暂不统计，不能用 session 总 Token 强行填充。

### 3.3 V1 指标

- 调用次数、趋势、使用项目数、session 覆盖；
- P50/P90 耗时；
- 成功、失败、取消、超时和重试率；
- 可获得时的 Token、成本、输出大小与指标覆盖率；
- 版本、Agent、项目和时间范围的对比；
- 高耗时、高失败、高重试、高输出等异常候选。

V1 的 `success` 仅表示调用执行完成。它不代表 Skill 的业务产物被接受，也不代表一个开发需求完成。

### 3.4 证据与数据质量

原始事实、派生指标、估算和推断必须分层：

```text
observed   原始记录或显式事件直接存在
derived    明确字段计算得到
estimated  有受限边界的 Token/成本估算
inferred   基于规则或上下文的判断
unknown    无法可靠判断
```

每个指标与异常候选保留 `raw_ref`、`evidence_refs`、`derivation_version` 和数据覆盖率，支持回到 session 和原始记录核验。

## 4. V2：从统计到改进假设

当 V1 的数据稳定后，分析层可识别跨调用模式：

- 某 MCP Tool 高频、P90 耗时高且输出过大；
- 某 Skill 经常失败后立即重试；
- 某能力升级后，耗时、失败率或 Token 覆盖样本出现明显变化；
- 某类 workflow 组合反复出现同一命令/测试失败；
- 同一资源被重复读取，可能造成上下文成本。

这些结论是**改进假设**，不是自动归因。每个假设应记录：

```text
evidence
affected capability and version
affected project/agent scope
proposed change
before metrics and coverage
owner / decision / status
```

最小成功闭环是：发现一个异常模式 -> 提出一个具体 Skill/MCP/workflow 改进 -> 在后续真实样本中比较同口径数据。

## 5. V3：需求级关联（可选增强）

需求级指标能回答“哪类需求、哪个研发节点效果不佳”，但需要额外的业务语义，不能从 transcript 自动可靠还原。

只有在以下信号可用时才启用：

- 稳定的 `requirement_id`；
- workflow、node 和 attempt 的显式生命周期事件；
- artifact、Git/CI、人工 checkpoint 等结果信号；
- Skill/MCP invocation 与上述生命周期的关联。

目标模型为：

```text
Requirement
  -> Workflow Run
    -> Node Run
      -> Attempt
        -> Capability Invocation / Session / Event
```

完整约束见 [需求级关联设计](./requirement-level-observability.md)。在这些前提未满足前，不应把 session 或 Skill 成功直接作为“需求完成”“节点一次通过”的统计分母。

## 6. V4：Feedback 与 Harness Eval

当能力使用模式和需求级结果均有可信证据后，才将异常样本转为改进与评测资产：

```text
anomaly pattern
  -> improvement item (Skill / KB / MCP / linter / workflow)
  -> reviewed eval candidate
  -> snapshot + input + rubric + golden
  -> rerun with new Harness version
  -> score and compare against baseline
```

Harness Eval 的价值是验证改进是否有效，而不是取代线上真实使用数据。Eval case、线上观测和改进项需共享 capability、版本、分类和证据引用。

## 7. 分阶段验收

| 阶段 | 通过条件 |
| --- | --- |
| V1 | 选定能力的调用边界、耗时、状态和证据可稳定识别；Token/成本精度透明。 |
| V2 | 至少一个异常模式促成具体改进假设，并可在后续真实样本中跟踪。 |
| V3 | 显式生命周期下，可将多个 session 与 capability invocation 关联到需求/节点，且口径可审计。 |
| V4 | 至少一个改进项可由线上指标或受控重跑验证效果，不只依赖主观判断。 |

## 8. 非目标

- 不替代 Langfuse、AgentOps 或 OTel 等通用 LLM/Agent tracing；
- 不以 session replay、远程控制或 Agent 编排为核心产品；
- 不在 V1 自动判断人工介入是否是 AI 失败；
- 不在 V1 自动修改 Skill、KB 或 workflow；
- 不把需求级归因当作没有结构化生命周期时仍必须实现的能力。
