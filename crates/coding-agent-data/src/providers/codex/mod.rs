mod checkpoint;
mod inheritance;
mod lineage;
mod normalize;
mod replay;
mod rollout;
mod source;
mod state_db;
mod token_usage;
mod usage_attribution;
#[cfg(feature = "codex-watch")]
mod watch;

use crate::providers::shared::identity::source_id_for_paths;
use crate::{
    AdapterCoverage, Batch, CapabilityCoverage, Checkpoint, Provider, ProviderId, ProviderInfo,
    Result, SourceCoverage,
};
#[cfg(feature = "codex-watch")]
use crate::{Subscription, WatchProvider};

pub use source::CodexSource;
#[cfg(feature = "codex-watch")]
pub use watch::CodexWatchOptions;

/// Stable identifier for the Codex provider.
pub const PROVIDER_ID: &str = "codex";

/// Coverage of Codex's durable local sources, rather than every runtime event.
///
/// Runtime-event persistence follows Codex's
/// [rollout persistence policy](https://github.com/openai/codex/blob/main/codex-rs/rollout/src/policy.rs).
/// Adapter coverage below describes how this crate maps the resulting durable
/// records into the provider-neutral model.
const COVERAGE: &[CapabilityCoverage] = &[
    CapabilityCoverage::new(
        "session",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "session_relation",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "session_history",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "message",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "reasoning",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "plan",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "tool_execution",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "file_change",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "agent_invocation",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "model_invocation",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "task_artifact",
        SourceCoverage::NotApplicable,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "usage",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "rate_limit",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "compaction",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "input_queue",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "user_input_request",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "hooks",
        SourceCoverage::PartiallyPersisted,
        AdapterCoverage::PartiallyNormalized,
    ),
    CapabilityCoverage::new(
        "execution_context",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "mode_change",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "notice",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "world_state",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "goals",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "approval",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "retry",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "model_reroute",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "rollback",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "fork_invocation_boundary",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "streaming_delta",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "os_file_audit",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "unknown_provider_data",
        SourceCoverage::Persisted,
        AdapterCoverage::RawOnly,
    ),
];

/// Read-only access to one local Codex data source.
#[derive(Clone, Debug)]
pub struct CodexProvider {
    source: CodexSource,
    info: ProviderInfo,
    limits: rollout::ScanLimits,
    #[cfg(feature = "codex-watch")]
    watch_options: CodexWatchOptions,
}

impl CodexProvider {
    /// Discovers Codex from `CODEX_HOME`, `CODEX_SQLITE_HOME`, and Codex
    /// configuration, falling back to `~/.codex`.
    pub fn discover() -> Result<Self> {
        Ok(Self::new(CodexSource::discover()?))
    }

    /// Creates a provider for explicit Codex paths.
    pub fn new(source: CodexSource) -> Self {
        let source_id =
            source_id_for_paths(PROVIDER_ID, &[source.codex_home(), source.sqlite_home()]);
        Self {
            source,
            info: ProviderInfo {
                id: ProviderId::new(PROVIDER_ID),
                name: "Codex",
                source: source_id,
            },
            limits: rollout::ScanLimits::default(),
            #[cfg(feature = "codex-watch")]
            watch_options: CodexWatchOptions::default(),
        }
    }

    /// Returns the resolved Codex source paths.
    pub fn source(&self) -> &CodexSource {
        &self.source
    }

    #[cfg(feature = "codex-watch")]
    /// Replaces filesystem debounce and reconciliation timing.
    pub fn with_watch_options(mut self, options: CodexWatchOptions) -> Self {
        self.watch_options = options;
        self
    }
}

impl Provider for CodexProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    fn coverage(&self) -> &'static [CapabilityCoverage] {
        COVERAGE
    }

    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<Batch> {
        let mut diagnostics = Vec::new();
        let mut state = checkpoint::decode(&self.info, checkpoint)?;
        let mut changes = Vec::new();
        let index = state_db::scan(
            &self.source,
            &self.info,
            &mut state,
            &mut changes,
            &mut diagnostics,
        )?;
        let has_more = rollout::scan(
            &self.source,
            &self.info,
            &self.limits,
            &index,
            &mut state,
            &mut changes,
            &mut diagnostics,
        )?;
        let batch = Batch::new(
            changes,
            checkpoint::encode(&self.info, state)?,
            diagnostics,
            has_more,
        );
        batch.validate_for(&self.info)?;
        Ok(batch)
    }
}

#[cfg(feature = "codex-watch")]
impl WatchProvider for CodexProvider {
    fn watch(&self, checkpoint: Checkpoint) -> Result<Subscription> {
        watch::subscribe(self.clone(), checkpoint)
    }
}
