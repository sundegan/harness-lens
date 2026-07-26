# 需求级 AI Coding 关联设计（后续能力）

## 1. 定位与前提

需求级关联是 HarnessLens 的高价值扩展，不是 V1 的前提。

V1 先回答“团队怎样使用 Skill、MCP Tool 和 workflow，哪些能力慢、贵、失败或重复调用”；需求级关联才回答“哪类业务需求、哪个研发节点的效果不佳”。后一个问题需要业务生命周期与结果信号，不能仅凭 Agent transcript 得到可靠答案。

```text
V1
Capability -> Invocation -> Session / Event

Future
Requirement -> Workflow Run -> Node Run -> Attempt
                                  -> Capability Invocation / Session / Event
```

本文定义后续启用需求级统计时必须满足的语义约束，避免将 session、Skill 调用或用户消息错误地当作业务统计主键。

## 2. 为什么不能从 Session 直接推导需求

Agent 原始数据通常组织为：

```text
Session / Thread
  -> Turn
    -> Message / Tool Call / Command
```

真实研发过程通常组织为：

```text
Requirement
  -> 需求分析
  -> TD
  -> 编码
  -> Code Review
  -> 验证与交付
```

两者不存在稳定的一一对应关系：

| 实际情况 | 错误等价关系 | 正确理解 |
| --- | --- | --- |
| 同一 session 先分析再生成 TD | 一个 Session = 一个节点 | 一个 session 可覆盖多个节点片段。 |
| 编码跨多个 session 完成 | 一个 Session = 一个需求 | 多个 session 可属于同一 Node Run。 |
| Skill 运行结束 | 一个 Skill Run = 节点完成 | Skill Run 只是一次 Activity。 |
| 用户补充需求 | 一条用户消息 = AI 失败 | 可能是正常澄清或范围变化。 |
| 人在 IDE 修改 | Agent 未记录 = 无人工投入 | 外部活动需要 Git、checkpoint 或人工补充。 |

因此，以下结论在没有额外信号时都不成立：

```text
Session 成功结束 = 需求完成
Skill 成功返回 = 节点一次通过
Session Token = 编码节点成本
用户消息数量 = 人工纠错次数
```

## 3. 目标模型

启用需求级关联后，业务模型为：

```text
Requirement
  -> Workflow Run
    -> Node Run
      -> Attempt
        -> Activity
          -> Session Segment / Human Activity
            -> Event
```

| 层级 | 含义 |
| --- | --- |
| Requirement | 一项完整业务需求，例如 Jira、PRD 或版本需求。 |
| Workflow Run | 该需求的一次 Harness 流程。 |
| Node Run | 需求分析、TD、编码、Review 等阶段的一次完整运行。 |
| Attempt | Node Run 的首次尝试或返工后的再次尝试。 |
| Activity | Skill、MCP、Agent 后续修改、测试、Review、IDE 修改等活动。 |
| Session Segment | 一个 Agent session 中归属于某个 Activity/Node 的时间区间。 |
| Event | command、MCP、token、message、file change 等原始事实。 |

`Capability Invocation` 是 Activity 的重要组成部分，仍沿用 V1 的事实模型和 raw evidence；需求级层不应重建或替代它。

## 4. 启用条件

需求级统计应满足以下最低条件后再建设：

1. **需求绑定**：稳定的 `requirement_id`，能由 workflow runner、人工选择或权威系统显式提供。
2. **生命周期**：`workflow_run_id`、`node_run_id`、`attempt_id` 的开始、完成、失败、取消等结构化事件。
3. **调用关联**：Skill/MCP invocation 能关联到 session、workflow 或 node，而不是仅靠文本猜测。
4. **结果信号**：artifact、Git/CI、测试、Review 或人工 checkpoint 能表达“产出是否被接受”。
5. **版本信息**：Harness、Skill、KB snapshot 和关键 MCP 配置可标识，支持前后比较。
6. **数据治理**：推断结果、人工修正和原始事实可追溯且可重新计算。

缺少上述条件时，可以保留 project、repository、session、capability 和 runtime 等 V1 维度，但不得发布需求级一次通过率、返工率或节点成本等强结论。

## 5. 事件契约

最好由统一 workflow runner 写生命周期事件，Skill 只补充调用、artifact、验证和失败语义。

```json
{
  "event": "harness.node.started",
  "requirement_id": "MKT-1234",
  "workflow_run_id": "wf-20260722-001",
  "node_run_id": "node-01J...",
  "attempt_id": "attempt-01J...",
  "node": "technical_design",
  "session_id": "agent-session-id",
  "harness_version": "git-sha",
  "skill_versions": { "technical-design": "git-sha" },
  "input_artifact_refs": ["prd://MKT-1234"],
  "started_at": "2026-07-22T10:00:00Z"
}
```

```json
{
  "event": "harness.node.completed",
  "node_run_id": "node-01J...",
  "status": "success",
  "verification": { "kind": "review", "status": "passed" },
  "output_artifact_refs": ["td://MKT-1234-v1"],
  "completed_at": "2026-07-22T10:30:00Z"
}
```

事件记录的是生命周期事实。`status: success` 仍然需要结合该节点的 acceptance contract 判断是否被业务接受。

## 6. 关联规则与数据质量

关联优先级：

1. 显式 `requirement_id`、`node_run_id`、`attempt_id`；
2. workflow runner、Git branch/worktree、artifact reference 等强关联；
3. 时间区间、Skill 声明的节点类型等受控推导；
4. transcript 文字与长时间无活动等弱推断。

所有业务结论必须携带：

```text
value
source              observed | derived | inferred | unknown
confidence
evidence_refs
derivation_version
```

显式绑定优先。自动推断可用于候选或人工校正，但不得覆盖原始事实。

## 7. 指标口径

需求级指标需要按节点定义 acceptance contract：

| 节点 | 可判定结果示例 |
| --- | --- |
| 需求分析 | 人工接受，关键需求未遗漏。 |
| TD | Review 通过，无结构性返工。 |
| 编码 | 验证通过，阻断性 Review 问题已解决。 |
| Code Review | 发现项经复核有效且闭环。 |

在结果信号可用后，可计算：

- 节点一次通过率；
- rework/rerun rate；
- 人工介入次数与原因分类；
- `tokens_per_successful_node`；
- artifact acceptance 与 verification pass rate；
- 按 requirement type、Harness/Skill/KB 版本的对比。

必须区分 Agent 执行指标与完整节点指标：

```text
Agent active time != 完整节点 wall time
Invocation success != 节点完成
Node completion != 需求交付
```

## 8. 人工介入与外部活动

人工行为不能一律作为失败。初期可使用以下分类，由人工确认高价值样本：

```text
scope_change
requirement_clarification
factual_correction
policy_or_security_approval
review_rejection
verification_failure
environment_or_tool_failure
preference_only
unknown
```

IDE 手工修改、线下讨论和人工 Review 只能通过 Git、CI、artifact 或低成本 checkpoint 粗粒度补充。没有证据时应显示为 `unknown`，而不是声称还原了完整人工投入。

## 9. 与 Feedback 和 Eval 的关系

需求级关联成熟后，线上事实可生成更高质量的改进与评测候选：

```text
Capability / Node anomaly
  -> evidence-backed improvement item
  -> reviewed eval case
  -> same input and snapshot rerun
  -> rubric/golden/baseline comparison
```

但这不改变演进顺序：先验证能力调用事实与使用效能，再在具备显式生命周期的范围内扩展需求级口径。

## 10. 结论

需求级关联值得建设，但只有在稳定的标识、生命周期、结果信号与版本信息已存在时才可信。HarnessLens V1 应先以 `Capability Invocation` 为主键建立分析与证据基础；需求级模型在此之上渐进叠加，而不是用 session 或 transcript 推断替代业务事实。
