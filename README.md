<div align="center">

<p><a href="README.md"><strong>English</strong></a> | <a href="README_ZH.md">中文</a></p>

<img src="assets/harness-lens-icon-source.png" width="104" alt="HarnessLens">

# HarnessLens

### Agent observability and effectiveness analytics

If this project helps you, please consider giving it a Star [⭐](https://github.com/sundegan/harness-lens).

<p>
  <img src="https://img.shields.io/badge/Focus-Harness%20Observability-0ea5e9?style=flat-square" alt="Focus: Harness Observability">
  <img src="https://img.shields.io/badge/Evidence-Traceable-10b981?style=flat-square" alt="Evidence: Traceable">
  <img src="https://img.shields.io/badge/Goal-Harness%20Evolution-f59e0b?style=flat-square" alt="Goal: Harness Evolution">
</p>

</div>

## Why HarnessLens

Teams are already using skills, MCP tools, project knowledge bases, and
development workflows, but lack observability tooling to surface problems and
improvement opportunities in their harness engineering. This makes it difficult
to answer:

- Which capabilities are used frequently, and which remain unused?
- Which skills or MCP tools are slow, token-intensive, failure-prone, or repeatedly retried?
- Which requirements or invocation combinations deviate materially from their own cost baselines?
- How long does a requirement spend in requirements analysis, technical design, coding, or code review, and how many attempts and instances of additional human input does it require?
- When an anomaly occurs, which sessions, tool calls, and errors form the evidence trail?
- After changing a skill, MCP tool, project knowledge base, or workflow, does real usage improve?

HarnessLens uses raw agent execution facts to analyze how teams use existing
workflows, skills, MCP tools, and agents in real development. It identifies
high-cost, low-success, high-intervention, and rework patterns, providing clear
data to guide harness improvements.

## What It Analyzes

| Surface | Questions HarnessLens helps answer |
| --- | --- |
| Skills | How do invocation frequency, duration, status, retries, and version differences compare? |
| MCP tools | Are call volume, duration, failures, output size, or context cost abnormal? |
| LLM models | Do capability usage patterns differ across models? Does switching models introduce regressions? |
| Workflows | Which development stages show high cost, failure, or repeated execution? How do duration, attempts, and additional human input vary across requirements analysis, technical design, coding, and code review? |
| Harness versions | Do changes to skills, MCP tools, knowledge bases, or workflows produce verifiable differences? |

## Local Coding-Agent Data Foundation

HarnessLens includes [`coding-agent-data`](./crates/coding-agent-data), a Rust
data-access library for desktop applications, CLIs, analytics tools, and
history viewers. It maps local data from different coding agents into a unified
API and data model, allowing applications to browse, index, synchronize,
analyze, and visualize that data without understanding each agent's private
data locations, storage formats, or schemas.

Each provider discovers and parses the local data sources for its agent, while
applications use normalized records to build their own storage, indexing,
synchronization, redaction, and presentation capabilities. The library
supports source discovery and parsing, checkpoint-based incremental scans, and
filesystem change monitoring with debounce and periodic reconciliation. It
currently supports Codex `state_5.sqlite`, active and archived rollout
`.jsonl`, compressed `.jsonl.zst`, and Claude Code main and subagent transcript
`.jsonl` files.

See the crate's [overview and API documentation](./crates/coding-agent-data/README.md).

<div align="center">

Making AI coding harness improvements observable and measurable.

</div>
