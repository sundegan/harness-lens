use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::providers::shared::{checkpoint as shared_checkpoint, tool::ObservedTool};
use crate::{Checkpoint, ProviderInfo, RecordId, Result, Session, Timestamp};

use super::token_usage::UsageAccountingState;
use super::usage_attribution::UsageFingerprint;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct CodexCheckpoint {
    #[serde(default)]
    pub database: Option<StateDatabaseSignature>,
    pub threads: BTreeMap<String, ThreadState>,
    pub rollouts: BTreeMap<PathBuf, RolloutState>,
    /// Lightweight file catalog used to bypass rollout reconstruction when
    /// neither the index nor any rollout metadata changed.
    #[serde(default)]
    pub rollout_catalog: Option<BTreeMap<PathBuf, RolloutFileMetadata>>,
    /// Whether every tracked rollout has a persisted usage-attribution index.
    ///
    /// Checkpoints created before attribution support rebuild their rollout
    /// records once instead of silently retaining duplicated deltas.
    #[serde(default)]
    pub usage_attribution_ready: bool,
    /// Usage identities retained while a reset rollout is rebuilt in bounded
    /// batches. Keeping the old index prevents unrelated copies from being
    /// promoted before the canonical rollout reaches them again.
    #[serde(default)]
    pub pending_usage_rebuilds: BTreeMap<PathBuf, BTreeSet<UsageFingerprint>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(super) struct StateDatabaseSignature {
    pub database: FileSignature,
    pub wal: Option<FileSignature>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ThreadState {
    pub fingerprint: u64,
    pub transcript: Option<PathBuf>,
    #[serde(default)]
    pub session: Option<Session>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RolloutContext {
    pub session_external_id: Option<String>,
    pub session: Option<RecordId>,
    /// Session metadata observed directly in this rollout.
    #[serde(default)]
    pub rollout_session: Option<Session>,
    /// Latest session view after merging the local index and rollout evidence.
    ///
    /// This is a scan-local cache. The index snapshot and `rollout_session`
    /// are already persisted separately, so serializing the merged copy would
    /// duplicate large instruction payloads in every checkpoint.
    #[serde(skip)]
    pub session_snapshot: Option<Session>,
    pub current_turn: Option<RecordId>,
    #[serde(default)]
    pub current_turn_external_id: Option<String>,
    /// Whether the current turn was inferred from legacy message boundaries.
    #[serde(default)]
    pub current_turn_inferred: bool,
    #[serde(default)]
    pub current_turn_started_at: Option<Timestamp>,
    #[serde(default)]
    pub current_turn_trace_id: Option<String>,
    #[serde(default)]
    pub current_turn_model_context_window: Option<i64>,
    #[serde(default)]
    pub current_turn_time_to_first_token_ms: Option<i64>,
    /// Latest model selected by turn context or thread settings.
    #[serde(default)]
    pub current_model: Option<String>,
    /// Latest model provider selected by turn context or thread settings.
    #[serde(default)]
    pub current_model_provider: Option<String>,
    /// Latest service tier selected by thread settings.
    #[serde(default)]
    pub current_service_tier: Option<String>,
    /// Provider-local state for turning Codex cumulative token observations
    /// into conservative additive deltas.
    #[serde(default)]
    pub usage_accounting: UsageAccountingState,
    /// Progress while rejecting token history copied into a forked rollout.
    #[serde(default)]
    pub usage_replay: UsageReplayState,
    /// Unique additive usage candidates observed in this rollout and whether
    /// this physical copy currently owns their aggregate attribution.
    #[serde(default)]
    pub usage_attribution: BTreeMap<UsageFingerprint, bool>,
    /// Calls with a rich terminal result in the current turn. Codex may emit
    /// one or more less informative output projections for the same call.
    #[serde(default)]
    pub terminal_tool_results: BTreeSet<String>,
    /// Tool identities retained until a correlated result is observed.
    #[serde(default)]
    pub tool_calls: BTreeMap<String, ObservedTool>,
    /// Legacy presentation records waiting for a canonical response-item
    /// projection of the same semantic kind.
    #[serde(default)]
    pub pending_legacy_presentations: BTreeMap<String, Vec<String>>,
    /// Canonical response-item projections observed before their legacy
    /// presentation counterpart.
    #[serde(default)]
    pub unmatched_canonical_presentations: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(super) enum UsageReplayState {
    /// The rollout metadata has not been classified yet.
    #[default]
    Uninitialized,
    /// Leading usage is still being compared with the parent rollout.
    MatchingParent {
        /// Number of parent usage entries already matched.
        index: usize,
    },
    /// A provider-native boundary identified an exact inherited prefix.
    SkippingInherited {
        /// Number of inherited usage deltas still to skip.
        remaining: usize,
    },
    /// The parent stream could not anchor the replay, so a rewritten burst is
    /// being skipped until the first real pause.
    SkippingRewrittenBurst {
        /// Timestamp of the most recently skipped usage entry.
        previous: Timestamp,
        /// Number of usage deltas already skipped by the fallback.
        #[serde(default)]
        skipped: usize,
    },
    /// The rollout is not a fork or has advanced past inherited usage.
    Done,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub(super) enum RolloutState {
    Plain {
        offset: u64,
        line: u64,
        tail_fingerprint: u64,
        context: RolloutContext,
    },
    Compressed {
        signature: FileSignature,
        line: u64,
        complete: bool,
        context: RolloutContext,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(super) struct FileSignature {
    pub len: u64,
    pub modified_nanos: u64,
    #[serde(default)]
    pub tail_fingerprint: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(super) struct RolloutFileMetadata {
    pub len: u64,
    pub modified_nanos: u64,
}

impl RolloutState {
    pub fn context(&self) -> &RolloutContext {
        match self {
            Self::Plain { context, .. } | Self::Compressed { context, .. } => context,
        }
    }
}

pub(super) fn decode(
    info: &ProviderInfo,
    checkpoint: Option<&Checkpoint>,
) -> Result<CodexCheckpoint> {
    shared_checkpoint::decode(info, checkpoint)
}

pub(super) fn encode(info: &ProviderInfo, state: CodexCheckpoint) -> Result<Checkpoint> {
    shared_checkpoint::encode(info, state)
}
