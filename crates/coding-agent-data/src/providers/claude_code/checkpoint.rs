use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::providers::shared::{checkpoint as shared_checkpoint, tool::ObservedTool};
use crate::{Checkpoint, Cost, ProviderInfo, Result, Session, Timestamp, TokenUsage};
use crate::{Record, RecordId};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct ClaudeCheckpoint {
    pub transcripts: BTreeMap<PathBuf, TranscriptState>,
    #[serde(default)]
    pub emitted_usage: BTreeMap<String, EmittedUsage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TranscriptState {
    pub offset: u64,
    pub line: u64,
    pub tail_fingerprint: u64,
    pub summary: Option<SessionSummary>,
    pub emitted_summary_fingerprint: Option<u64>,
    #[serde(default)]
    pub usage: BTreeMap<String, UsageSnapshot>,
    #[serde(default)]
    pub context: TranscriptContext,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct TranscriptContext {
    pub current_invocation: Option<RecordId>,
    pub current_invocation_external_id: Option<String>,
    pub current_invocation_started_at: Option<Timestamp>,
    #[serde(default)]
    pub tool_calls: BTreeMap<String, ObservedTool>,
    /// Background-task identity learned from durable queue notifications,
    /// keyed by the spawning tool call.
    #[serde(default)]
    pub queue_tasks: BTreeMap<String, String>,
    /// Latest merged agent-invocation projection, keyed by tool call, so a
    /// later queue notification can enrich a terminal invocation safely.
    #[serde(default)]
    pub agent_invocations: BTreeMap<String, Record>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct SessionSummary {
    pub project_key: String,
    pub session: Session,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(super) struct UsageSnapshot {
    pub usage: TokenUsage,
    pub complete: bool,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub cost: Option<Cost>,
    #[serde(default)]
    pub sidechain: bool,
    #[serde(default)]
    pub line: u64,
    #[serde(default)]
    pub byte_start: u64,
    #[serde(default)]
    pub byte_end: u64,
    #[serde(default)]
    pub timestamp: Option<Timestamp>,
    #[serde(default)]
    pub invocation: Option<RecordId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(super) struct EmittedUsage {
    pub fingerprint: u64,
    pub path: PathBuf,
}

pub(super) fn decode(
    info: &ProviderInfo,
    checkpoint: Option<&Checkpoint>,
) -> Result<ClaudeCheckpoint> {
    shared_checkpoint::decode(info, checkpoint)
}

pub(super) fn encode(info: &ProviderInfo, state: ClaudeCheckpoint) -> Result<Checkpoint> {
    shared_checkpoint::encode(info, state)
}
