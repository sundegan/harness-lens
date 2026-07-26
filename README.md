<div align="center">

<p><a href="README.md"><strong>English</strong></a> | <a href="README_ZH.md">中文</a></p>

<img src="assets/harness-lens-icon-source.png" width="104" alt="HarnessLens">

# HarnessLens

### Usage effectiveness analytics for AI coding harnesses

HarnessLens analyzes how teams use existing workflows, skills, MCP tools, and agents in real development. It identifies high-cost, low-success, high-intervention, and rework patterns, providing clear data to guide harness improvements.

<p>
  <img src="https://img.shields.io/badge/Focus-Harness%20Observability-0ea5e9?style=flat-square" alt="Focus: Harness Observability">
  <img src="https://img.shields.io/badge/Evidence-Traceable-10b981?style=flat-square" alt="Evidence: Traceable">
  <img src="https://img.shields.io/badge/Goal-Harness%20Evolution-f59e0b?style=flat-square" alt="Goal: Harness Evolution">
</p>

</div>

## Why HarnessLens

Teams are already using skills, MCP tools, project knowledge bases, and development workflows, but lack observability tooling to surface problems and improvement opportunities in their harness engineering. This makes it difficult to answer:

- Which capabilities are used frequently, and which remain unused?
- Which skills or MCP tools are slow, token-intensive, failure-prone, or repeatedly retried?
- Which requirements or invocation combinations deviate materially from their own cost baselines?
- How long does a requirement spend in requirements analysis, technical design, coding, or code review, and how many attempts and instances of additional human input does it require?
- When an anomaly occurs, which sessions, tool calls, and errors form the evidence trail?
- After changing a skill, MCP tool, project knowledge base, or workflow, does real usage improve?

## What It Analyzes

| Surface | Questions HarnessLens helps answer |
| --- | --- |
| Skills | How do invocation frequency, duration, status, retries, and version differences compare? |
| MCP tools | Are call volume, duration, failures, output size, or context cost abnormal? |
| LLM models | Do capability usage patterns differ across models? Does switching models introduce regressions? |
| Workflows | Which development stages show high cost, failure, or repeated execution? How do duration, attempts, and additional human input vary across requirements analysis, technical design, coding, and code review? |
| Harness versions | Do changes to skills, MCP tools, knowledge bases, or workflows produce verifiable differences? |

## Local Coding-Agent Data Foundation

HarnessLens includes [`coding-agent-data`](./crates/coding-agent-data), a
reusable, read-only Rust data-access library for desktop applications, CLIs,
analytics tools, history viewers, and other consumers of local coding-agent
data. Developers can use it to build local browsing, indexing, synchronization,
analysis, and visualization features without having to adapt separately to each
agent's data locations, storage formats, and schemas.

Applications use one consistent API to work with data from different coding
agents. The library provides source discovery, reading and parsing, a common
record/change model, checkpoint-based incremental synchronization, and live
change monitoring. The current implementation supports Codex thread metadata
and active, archived, or compressed rollout events. Other providers and data
kinds can be added without exposing their private storage contracts to consumers.

See the [crate documentation](./crates/coding-agent-data/README.md) and
[architecture](./docs/coding-agent-data.md).

<div align="center">

Making AI coding harness improvements observable and measurable.

</div>
