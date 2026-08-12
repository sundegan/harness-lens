<div align="center">

<p><a href="FORMAT_PROBE.md">English</a> | <a href="FORMAT_PROBE_ZH.md"><strong>中文</strong></a></p>

</div>

# 格式探针

格式探针用于尽早发现 Codex、Claude Code 等 Provider 的本地私有格式发生变化，并验证当前 Provider adapter 是否仍能正确读取和归一化这些数据。

它同时回答两个不同的问题：

| 检查           | 回答的问题                                  | 检查方式                                                                            |
| -------------- | ------------------------------------------- | ----------------------------------------------------------------------------------- |
| 结构兼容性     | Provider 持久化格式是否出现了未审核的变化？ | 生成不含业务标量内容的结构指纹，并与提交到仓库的审核基线比较                        |
| Adapter 兼容性 | 当前 adapter 是否真的能读取代表性本地数据？ | 在隔离数据源中运行真实 `Provider::scan`，检查每个代表性 artifact 的归一化产出和诊断 |

只做结构比较可能遗漏“格式看起来已知，但 adapter 没有产生数据”的问题，只运行 adapter 又可能把未知字段静默忽略。格式探针将两项检查组合为同一个本地兼容性测试。

## 支持范围

| Provider    | 代表性 artifact                       | 探测内容                                             | 基线策略                                         | Adapter 验证                            |
| ----------- | ------------------------------------- | ---------------------------------------------------- | ------------------------------------------------ | --------------------------------------- |
| Codex       | `state_5.sqlite`                      | 表、视图、列、索引及 DDL 摘要                        | `Exact`，完整结构必须匹配                        | 状态数据库必须产生归一化 Record         |
| Codex       | 最新 rollout `.jsonl` 或 `.jsonl.zst` | JSON 路径、值类型、受控 discriminator 和探针策略标记 | `AllowedStructure`，当前结构必须是审核结构的子集 | 代表性 rollout 必须产生归一化 Record    |
| Claude Code | 最新 transcript `.jsonl`              | JSON 路径、值类型、受控 discriminator 和探针策略标记 | `AllowedStructure`，当前结构必须是审核结构的子集 | 代表性 transcript 必须产生归一化 Record |

自动发现只选择最新的代表性 JSONL artifact，以保证测试快速且有界。它不会回放全部历史 Session。

## 完整执行流程

```text
发现 Provider 本地代表性 artifact
    ↓
只读检查并固定本轮结构指纹
    ↓
验证 artifact 可读且包含有效结构
    ↓
在隔离数据源中运行真实 Provider adapter
    ↓
确认每个 artifact 都产生归一化 Record，且没有兼容性错误
    ↓
与审核基线比较
    ↓
兼容则通过，出现未审核变化则失败
```

结构指纹在 Adapter 扫描之前生成。这样即使活跃 JSONL 在扫描期间继续追加，本轮写入或比较的仍是已经交给 Adapter 验证过的结构快照。

Provider 源始终只读：

- SQLite 使用只读连接检查 Schema，不读取数据行。
- 最新 JSONL 通过硬链接放入临时隔离目录，不复制 transcript 内容。
- 探针和测试不会写回 Provider 的 SQLite、rollout 或 transcript。

## 比较策略

### Exact

`Exact` 适用于 SQLite 这类可完整枚举的 Schema。表、视图、列、索引、约束相关 DDL 摘要等任一结构发生增加、删除或修改，都会产生不兼容差异。

### AllowedStructure

一份 JSONL 通常只覆盖 Provider 完整事件词汇的一部分，因此不能要求每个最新 Session 都重现历史上见过的全部字段。`AllowedStructure` 使用方向性比较：

| 当前观察                                | 结果 | 原因                              |
| --------------------------------------- | ---- | --------------------------------- |
| 缺少基线中的已知字段或事件类型          | 允许 | 当前 Session 可能没有触发该能力   |
| 出现基线中没有的 JSON 路径              | 失败 | 可能是新增字段或结构层级变化      |
| 已知路径出现新的 JSON 值类型            | 失败 | 可能使现有解析假设失效            |
| 出现新的受控 discriminator              | 失败 | 可能是新增事件、角色、状态或模式  |
| 出现新的动态 Map、opaque 或深度截断路径 | 失败 | Provider 私有探测策略需要重新审核 |

普通 JSONL 和 JSONL.ZST 在允许结构基线中会规范为同一种 JSONL 结构，因此仅压缩方式不同不会产生格式漂移。

## 可读性和 Adapter 失败条件

本地兼容性测试在以下情况下失败：

| 类别          | 失败条件                                                                              |
| ------------- | ------------------------------------------------------------------------------------- |
| Artifact 发现 | 配置了基线的代表性 artifact 没有被发现，或者发现了没有对应基线的 artifact             |
| JSONL 可读性  | 没有记录、没有任何有效 JSON、存在完整但非法的 JSON、记录超过解析上限或诊断被截断      |
| 压缩归档      | `.jsonl.zst` 无法解压，或归档最后一条记录不是有效 JSON                                |
| SQLite 可读性 | 无法以只读方式打开、Schema 查询失败、没有用户定义对象，或 Codex 必需表消失            |
| Adapter 扫描  | 扫描报错、无法在有界批次数内收敛，或某个代表性 artifact 没有产生任何 `Change::Upsert` |
| Adapter 诊断  | 出现错误级诊断、无效 JSON、超大记录、诊断截断或 Provider 定义的兼容性诊断             |
| 基线          | 基线损坏、Provider/指纹版本/比较模式不匹配，或出现未审核结构差异                      |

普通活跃 `.jsonl` 可能正在追加，因此最后一条没有换行且尚未形成有效 JSON 的记录会记为 `incomplete_records`，不会立即判定为损坏。`.jsonl.zst` 是封闭归档，不适用这一宽容规则。

## 本地运行

命令应在 `coding-agent-data` crate 目录执行：

```bash
cd crates/coding-agent-data
make test-provider-formats
```

测试会读取本机 Provider 的代表性数据。Provider 未安装或没有发现任何受支持的 artifact 时会明确跳过。一旦发现了部分 artifact，缺少配置基线要求的其他代表性 artifact 会失败，避免只验证不完整的 Provider 数据面。

成功输出只包含：

- artifact 类型和结构数量
- Adapter 产生的归一化 Change 数量
- Batch 数量
- 诊断代码和数量
- 基线比较结果

输出不会包含本地源路径、Prompt、消息正文、工具输入输出或 SQLite 数据行。

普通 `cargo test` 不会读取开发者本地数据，本地兼容性测试带有 `#[ignore]`，只能通过上述命令或显式运行 ignored tests 执行。提交的基线合法性和比较算法仍属于普通测试。

## 审核并更新基线

发现上游变化时，不应直接更新基线让测试变绿。先确认变化是否被当前 Adapter 正确归一化，必要时修改解析逻辑并增加脱敏 Fixture，然后再更新基线：

```text
本地兼容性测试发现变化
    ↓
检查结构差异和上游格式
    ↓
修复或扩展 Provider adapter
    ↓
增加最小脱敏回归测试
    ↓
确认真实 Adapter 验证通过
    ↓
更新并审查基线
```

显式更新命令：

```bash
make update-provider-format-baselines
make test-provider-formats
```

更新行为：

| 基线类型                 | 更新方式                                                         |
| ------------------------ | ---------------------------------------------------------------- |
| SQLite `Exact`           | 用当前完整 Schema 指纹替换原基线                                 |
| JSONL `AllowedStructure` | 将当前观察到的结构合并到历史审核结构，不删除本次未出现的已知结构 |

基线通过同目录临时文件原子替换。已有基线无法读取、JSON 损坏、Provider 不同或比较模式不同时，更新会失败，不会静默覆盖。指纹版本或关键探针限制变化时会明确提示重建。

更新后必须审查以下文件的 diff：

| Provider    | Artifact   | 基线                                                                                                             |
| ----------- | ---------- | ---------------------------------------------------------------------------------------------------------------- |
| Codex       | 状态数据库 | [`../tests/format-baselines/codex/state_database.json`](../tests/format-baselines/codex/state_database.json)     |
| Codex       | rollout    | [`../tests/format-baselines/codex/rollout.json`](../tests/format-baselines/codex/rollout.json)                   |
| Claude Code | transcript | [`../tests/format-baselines/claude_code/transcript.json`](../tests/format-baselines/claude_code/transcript.json) |

## 外部调用接口

`format-probe` feature 提供 Provider-neutral 的公共模型和比较接口。Provider 实现负责私有路径发现和格式解释。

```rust,no_run
use std::path::Path;

use coding_agent_data::format_probe::{
    FormatBaseline, FormatProbe, FormatProbeOptions,
};
use coding_agent_data::providers::codex::CodexFormatProbe;

fn check_artifact(
    baseline: &FormatBaseline,
    artifact: &Path,
) -> coding_agent_data::Result<bool> {
    let current = CodexFormatProbe.probe_path(
        artifact,
        FormatProbeOptions::default(),
    )?;
    Ok(baseline.check(&current).is_compatible())
}
```

公共 API 的职责：

| API                                        | 用途                                    |
| ------------------------------------------ | --------------------------------------- |
| `FormatProbe::probe_path`                  | 对明确的 Provider artifact 生成结构指纹 |
| `FormatProbeDiscovery::discover_artifacts` | 按 Provider 规则发现代表性本地 artifact |
| `FormatFingerprint::diff`                  | 对两个指纹进行精确结构比较              |
| `FormatFingerprint::check_compatibility`   | 按指定模式同时检查结构差异和不可读记录  |
| `FormatBaseline::check`                    | 使用基线保存的比较策略检查当前指纹      |
| `FormatCompatibilityReport`                | 返回差异列表、比较模式和不可读记录状态  |

调用方没有明确 artifact 路径时，可以先通过 `FormatProbeDiscovery::discover_artifacts` 获取带稳定标签的代表性 artifact，再为每个标签选择对应基线。

公共 API 可以用于外部 CLI、CI 或桌面应用的格式变化探测，但 crate 内的 `make test-provider-formats` 还会额外运行真实 Adapter，覆盖面更完整。

## 指纹内容与隐私边界

结构指纹保留：

- Provider ID、指纹版本和探针限制
- JSON 路径及对应的 JSON 值类型
- Provider 明确选择的短 discriminator，例如事件 `type`、消息 `role` 和执行 `status`
- 动态 Map、opaque payload 和深度截断路径
- SQLite 对象、列和索引定义，以及 DDL 的 SHA-256 摘要

结构指纹不保留：

- 被检查文件的本地路径
- 一般 JSON 字符串、数字或布尔值
- Prompt、消息正文、推理、工具参数和工具结果
- SQLite 数据行
- API Key、凭据或环境变量值

Provider 的 `JsonFormatPolicy` 必须把可能承载用户内容、工具载荷或动态键的字段标记为 opaque 或 dynamic map。新增基线提交前还应执行敏感信息检查并人工审查 diff。

## 代码边界

| 文件                                                                                             | 职责                                                     |
| ------------------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| [`../src/format_probe.rs`](../src/format_probe.rs)                                               | 公共接口、指纹模型、基线模型和 Provider-neutral 比较算法 |
| [`../src/providers/shared/format_probe.rs`](../src/providers/shared/format_probe.rs)             | Provider 共用的 JSONL 结构观察器和本地兼容性测试辅助逻辑 |
| [`../src/providers/codex/format_probe.rs`](../src/providers/codex/format_probe.rs)               | Codex artifact 发现、JSON 私有策略和兼容性测试           |
| [`../src/providers/codex/format_probe/sqlite.rs`](../src/providers/codex/format_probe/sqlite.rs) | Codex SQLite Schema 只读探测                             |
| [`../src/providers/claude_code/format_probe.rs`](../src/providers/claude_code/format_probe.rs)   | Claude Code artifact 发现、JSON 私有策略和兼容性测试     |

Provider 私有目录、Schema、discriminator 规则和诊断规则必须留在对应 Provider 模块。根级 `format_probe` 模块不应依赖 Codex 或 Claude Code 私有格式。

## 指纹版本

`FORMAT_FINGERPRINT_VERSION` 表示序列化指纹协议的版本，而不是 Provider 数据格式版本。以下变化通常需要提升版本：

- 指纹字段的语义发生变化
- JSON 路径或值类型的记录规则变化
- SQLite Schema 观察规则变化
- 比较语义发生不兼容变化

Provider 新增一个字段本身不需要提升指纹版本，它会作为普通结构漂移由基线比较发现。

## 已知限制

| 限制                         | 影响                                                                             |
| ---------------------------- | -------------------------------------------------------------------------------- |
| 只采样最新代表性 JSONL       | 不能证明所有历史 artifact 都与当前 Adapter 兼容                                  |
| JSONL 使用方向性比较         | 当前样本未出现旧字段时，无法判断上游是否已经永久删除该字段                       |
| 只保留受控 discriminator     | 未被 Provider 策略选中的字符串枚举变化不会单独报告，但对应路径和值类型仍会被观察 |
| Unknown 是统一模型的合法降级 | 不能把所有 Unknown Record 或 Event 一律判为 Adapter 不兼容                       |
| 临时隔离依赖硬链接           | 极少数不支持硬链接或跨文件系统环境会明确失败，不会退化为复制敏感 transcript      |

格式探针提供的是“被检查代表性数据的结构和 Adapter 兼容性证据”，不是对 Provider 全部历史数据和全部语义覆盖率的证明。
