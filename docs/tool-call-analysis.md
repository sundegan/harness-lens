# MCP 工具调用分析数据定义

工具调用分析读取 Codex 和 Claude Code 的本地持久化数据，但不会修改 Provider 的源文件、SQLite 或 transcript。完整链路为：

```text
Provider read
→ normalized ToolCall / ToolResult / Retry
→ session_events evidence
→ MCP-only mcp_tool_call / mcp_tool_call_retry projections
→ typed Tauri commands
→ Svelte analysis and evidence drill-down
```

Provider 私有路径、schema 和事件格式只存在于对应 Adapter。SQLite 由 Rust 管理；Svelte 只接收类型化查询结果。

## `mcp_tool_call` 事实表

`mcp_tool_call` 一行代表一次 Provider 明确归因为 MCP 的工具调用。唯一身份由 `source_id + source_path + call_id` 决定，避免不同 Provider、派生 session 或源文件复用原生 `call_id` 时发生冲突。

命令执行、文件读写、内置搜索、Provider Hosted Tool、Plugin、Custom Tool 和来源未知的调用不会进入该表。它们仍作为统一 `ToolCall` 事件保存在 `session_events`，供会话历史展示。

事实分为：

- 归属：Provider、source、session、Agent invocation。
- 工具身份：名称、namespace、工具类型、MCP Server。
- 生命周期：开始、完成、耗时、状态、结果和错误证据。
- 证据关系：`call_event_id`、`result_event_id`。
- 派生分析：不可逆输入指纹、精确重复链和内部重试证据。

完整输入输出不复制到 `mcp_tool_call`。详情按需通过事件 ID 读取 `session_events.event_json`，避免在分析事实中再次保存调用参数、代码和本地路径。

调用和结果保持独立证据：

- ToolCall 先到时创建或更新调用事实；后续状态观察不会增加调用次数。
- ToolResult 先到时只保存到 `session_events`；后续明确归因为 MCP 的 ToolCall 到达后再关联已有结果。
- 缺失 ToolResult 时 `has_result = false`，不自动判定失败。
- ToolResult 被删除后，清空结果、耗时和完成时间，并从仍存在的 ToolCall 恢复状态；没有 Call 证据时回退为 `unknown`。
- Call 和 Result 证据都不存在时删除该事实。

## MCP 归因边界

统一来源类型为：

```text
built_in / mcp / provider_hosted / plugin / custom / unknown
```

MCP Server 只有在 Provider 明确把来源归类为 `mcp` 时才记录。`namespace` 或形如 `mcp__server__tool` 的工具名称本身不是 MCP 证据：

- Codex 的 MCP 生命周期事件映射为 `mcp`。
- Claude Code `mcp_tool_use` 映射为 `mcp`。
- Claude Code `server_tool_use` 映射为 `provider_hosted`。
- 普通 `tool_use` 即使名称带 MCP 前缀，也不会仅凭名称猜测来源。

`mcp_tool_call` 不保存 `source_kind`，因为表自身即代表 MCP-only 契约。`mcp_server` 缺失时保留未知值，不根据 namespace 或工具名称猜测。

## `mcp_tool_call_retry` 显式重试证据

`mcp_tool_call_retry` 只保存 Provider 持久化且能结构化关联到 MCP 调用的 Retry 观察，包括时间、attempt、delay、Invocation 和 `mcp_tool_call_id`。

Retry 是独立事件而不是 `mcp_tool_call` 的状态：

- Provider 有结构化关联时连接到对应调用。
- 无法可靠关联到 MCP 调用时不进入 MCP 重试投影，原始事件仍保留在 `session_events`。
- `explicit_retry_count` 是对应 Retry 证据的计数，可由投影重算。

## 状态、耗时和比例

状态为：

```text
pending / awaiting_approval / in_progress / completed / failed /
cancelled / declined / unknown
```

页面核心指标：

| 指标 | 定义 |
| --- | --- |
| 调用数 | `mcp_tool_call` 行数 |
| 工具数 | 筛选范围内不同工具身份数 |
| 成功率 | `completed / (completed + failed)` |
| 平均耗时 | 只统计 `duration_ms IS NOT NULL` 的样本 |
| 重复调用 | 同一 Agent invocation 内，相同工具身份与输入指纹的后续调用数 |

`declined` 和 `cancelled` 分别统计，不属于执行失败。缺失结果不代表失败。未知状态、未知时间和缺失耗时继续保存在事实表中，不能当作成功、失败或零。

成功率返回 numerator、denominator、rate 和 unknown count；耗时缺失的数据不会按零耗时计算。

## 精确重复

精确重复要求：

```text
同一 Agent invocation
+ 同一 Provider 和工具身份
+ 相同 canonical JSON 输入指纹
+ 不同 call_id
```

canonical JSON 对对象 key 递归排序，保留数组顺序，不删除路径、时间、随机 ID 或其他字段。指纹包含 MCP、namespace、MCP Server、工具名和规范化输入。这个指标只表示输入完全一致，不声称业务语义相同。

每条调用记录：

- `repeat_group_id`
- `repeat_of_id`
- `repeat_index`

底层仍保留可重建的 Retry 证据和关系，但页面与公开分析契约不展示“显式/推断重试”等概念，也不会把重复调用解释为重试。

## 项目、Provider 和 MCP 维度

- Provider 使用 Adapter 的稳定 ID，例如 `codex`、`claude-code`。
- 项目按稳定 `project_key` 聚合，页面展示 `project_name`，不公开完整本地路径。
- `project_key` 优先组合规范化 Git remote 与 cwd；没有 remote 时使用 cwd，并在本地保存不可逆散列。
- 所有比较均只使用 `mcp_tool_call` 中的 MCP 调用。

对比页面展示 Provider、项目和 MCP Server 三个维度。Provider 与 MCP Server 并排展示，项目对比独占一行，避免多列表格相互挤压。

## 时间趋势和筛选

时间范围支持最近 24 小时、7 天、30 天、90 天、全部和自定义范围。趋势按请求中的 IANA timezone 分桶，支持小时、日、ISO 周和月，并正确处理 DST。

趋势在起止范围内补齐没有调用的连续 bucket。没有 timestamp 的调用不会被伪造到某个时间 bucket，也不会计入无法证明其归属的限定时间范围。

页面联合筛选包括：

- 时间范围
- Provider
- 项目
- 工具名称
- MCP Server

筛选选项使用 faceted count：计算某个维度的选项时排除该维度自身条件，同时保留其他筛选。所有筛选值通过 SQLite 占位符绑定。

调用明细使用服务端分页、受限 page size 和稳定排序。Provider 与 MCP Server 分列显示，时间位于最后一列。搜索范围只包含工具身份、MCP Server、会话标题和项目名称等展示字段，不扫描完整工具输入输出。

## Tauri API 和详情跳转

工具调用页面使用四个独立命令：

```text
get_tool_call_analysis
get_tool_call_filter_options
get_tool_call_page
get_tool_call_detail
```

分析接口返回概览、连续时间趋势、状态与工具排行，以及 Provider、项目、MCP Server 对比和生成时间。分页和详情独立查询，避免把全部事件载入分析快照。

单次详情返回：

- 工具调用事实和来源维度
- ToolCall 与 ToolResult 事件
- 所属 session 和待定位事件 ID

“查看所属会话”切换到会话模块，根据 session ID 加载详情，滚动并高亮 `call_event_id`；Call 证据缺失时回退到 `result_event_id`。分析页的筛选和页码状态保留。

## 重放、删除和 source scope

每个 Provider Worker 持有独立 `source_id`、checkpoint、同步状态和 watcher。Codex 与 Claude Code 可同时同步，某个 Provider 失败不会清理其他 Provider 数据。

迁移或投影语义变化只重建 HarnessLens 自身可重建数据，不修改 Agent 原始源：

- Provider checkpoint 无效时只删除对应 `source_id` 的投影后重放。
- rollout/transcript Reset 只删除对应 `source_id + source_path` 后重放。
- Remove 删除对应源文件事实。
- 单条 Delete 清理事件、Retry 和受影响的重复/重试链。
- checkpoint 只在同一事务中的事实投影成功后推进。

## 测试与真实数据验证

原生 E2E 使用隔离数据库种子，不启动真实 monitor，覆盖 Codex/Claude、多个 MCP Server 和项目、成功/失败/拒绝/取消/缺失结果、精确重复统计、单条详情和会话事件定位。

真实本地验证使用 ignored Rust 测试，将 Provider 扫描结果投影到临时 HarnessLens SQLite。该测试只输出 Provider、批次数、change/diagnostic 数、工具调用数和完整性计数，不输出 transcript、工具输入输出、命令或本地路径：

```bash
cargo test -p harness-lens real_local_tool_call_projection_is_consistent \
  --all-features -- --ignored --nocapture
```

需要单独验证某个 Provider 时，可设置 `HARNESS_LENS_REAL_PROVIDER=codex` 或
`HARNESS_LENS_REAL_PROVIDER=claude-code`。未设置时同时验证两个 Provider。

验证包含：

```sql
PRAGMA quick_check;
PRAGMA foreign_key_check;

SELECT COUNT(*)
FROM mcp_tool_call tc
LEFT JOIN agent_sessions s ON s.id = tc.session_id
WHERE s.id IS NULL;

SELECT COUNT(*)
FROM mcp_tool_call tc
LEFT JOIN session_events e ON e.id = tc.call_event_id
WHERE tc.call_event_id IS NOT NULL AND e.id IS NULL;

SELECT COUNT(*)
FROM mcp_tool_call tc
LEFT JOIN session_events e ON e.id = tc.result_event_id
WHERE tc.result_event_id IS NOT NULL AND e.id IS NULL;

SELECT COUNT(*)
FROM (
    SELECT 1
    FROM mcp_tool_call
    GROUP BY source_id, source_path, call_id
    HAVING COUNT(*) > 1
);

SELECT COUNT(*)
FROM mcp_tool_call_retry retry
LEFT JOIN session_events e ON e.id = retry.id
WHERE e.id IS NULL;

SELECT COUNT(*)
FROM mcp_tool_call_retry retry
LEFT JOIN mcp_tool_call call ON call.id = retry.mcp_tool_call_id
WHERE call.id IS NULL;
```

所有 COUNT 检查必须为 0，`quick_check` 必须为 `ok`，且 `foreign_key_check` 不得返回记录。
