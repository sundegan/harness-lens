mod checkpoint;
#[cfg(feature = "format-probe")]
mod format_probe;
mod normalize;
mod source;
mod transcript;
#[cfg(feature = "claude-code-watch")]
mod watch;

use crate::providers::shared::identity::source_id_for_paths;
use crate::{
    AdapterCoverage, Batch, CapabilityCoverage, Checkpoint, Provider, ProviderId, ProviderInfo,
    Result, ScanProgress, ScanProgressProvider, SourceCoverage,
};
#[cfg(feature = "claude-code-watch")]
use crate::{Subscription, WatchProvider};

#[cfg(feature = "format-probe")]
pub use format_probe::ClaudeCodeFormatProbe;
pub use source::ClaudeCodeSource;
#[cfg(feature = "claude-code-watch")]
pub use watch::ClaudeCodeWatchOptions;

/// Stable identifier for the Claude Code provider.
pub const PROVIDER_ID: &str = "claude-code";

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
        SourceCoverage::PartiallyPersisted,
        AdapterCoverage::PartiallyNormalized,
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
    CapabilityCoverage::new("plan", SourceCoverage::Unknown, AdapterCoverage::Unknown),
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
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "task_artifact",
        SourceCoverage::Unknown,
        AdapterCoverage::Unknown,
    ),
    CapabilityCoverage::new(
        "usage",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "rate_limit",
        SourceCoverage::Unknown,
        AdapterCoverage::Unknown,
    ),
    CapabilityCoverage::new(
        "compaction",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "input_queue",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "user_input_request",
        SourceCoverage::PartiallyPersisted,
        AdapterCoverage::PartiallyNormalized,
    ),
    CapabilityCoverage::new(
        "hooks",
        SourceCoverage::Persisted,
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
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "world_state",
        SourceCoverage::NotApplicable,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "goals",
        SourceCoverage::NotApplicable,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "approval",
        SourceCoverage::PartiallyPersisted,
        AdapterCoverage::PartiallyNormalized,
    ),
    CapabilityCoverage::new("retry", SourceCoverage::Unknown, AdapterCoverage::Unknown),
    CapabilityCoverage::new(
        "model_reroute",
        SourceCoverage::Unknown,
        AdapterCoverage::Unknown,
    ),
    CapabilityCoverage::new(
        "rollback",
        SourceCoverage::Unknown,
        AdapterCoverage::Unknown,
    ),
    CapabilityCoverage::new(
        "fork_invocation_boundary",
        SourceCoverage::NotApplicable,
        AdapterCoverage::NotApplicable,
    ),
    CapabilityCoverage::new(
        "streaming_delta",
        SourceCoverage::PartiallyPersisted,
        AdapterCoverage::PartiallyNormalized,
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

/// Read-only access to one local Claude Code data source.
#[derive(Clone, Debug)]
pub struct ClaudeCodeProvider {
    source: ClaudeCodeSource,
    info: ProviderInfo,
    limits: transcript::ScanLimits,
    #[cfg(feature = "claude-code-watch")]
    watch_options: ClaudeCodeWatchOptions,
}

impl ClaudeCodeProvider {
    /// Discovers Claude Code from `CLAUDE_CONFIG_DIR`, falling back to
    /// `~/.claude`.
    pub fn discover() -> Result<Self> {
        Ok(Self::new(ClaudeCodeSource::discover()?))
    }

    /// Creates a provider for an explicit Claude Code source.
    pub fn new(source: ClaudeCodeSource) -> Self {
        let source_id = source_id_for_paths(PROVIDER_ID, &[source.config_dir()]);
        Self {
            source,
            info: ProviderInfo {
                id: ProviderId::new(PROVIDER_ID),
                name: "Claude Code",
                source: source_id,
            },
            limits: transcript::ScanLimits::default(),
            #[cfg(feature = "claude-code-watch")]
            watch_options: ClaudeCodeWatchOptions::default(),
        }
    }

    /// Returns the resolved Claude Code source path.
    pub fn source(&self) -> &ClaudeCodeSource {
        &self.source
    }

    #[cfg(feature = "claude-code-watch")]
    /// Replaces filesystem debounce and reconciliation timing.
    pub fn with_watch_options(mut self, options: ClaudeCodeWatchOptions) -> Self {
        self.watch_options = options;
        self
    }
}

impl Provider for ClaudeCodeProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    fn coverage(&self) -> &'static [CapabilityCoverage] {
        COVERAGE
    }

    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<Batch> {
        let batch = transcript::scan(&self.source, &self.info, &self.limits, checkpoint)?;
        batch.validate_for(&self.info)?;
        Ok(batch)
    }
}

impl ScanProgressProvider for ClaudeCodeProvider {
    fn scan_progress(&self, checkpoint: Option<&Checkpoint>) -> Result<ScanProgress> {
        transcript::progress(&self.source, &self.info, checkpoint)
    }
}

#[cfg(feature = "claude-code-watch")]
impl WatchProvider for ClaudeCodeProvider {
    fn watch(&self, checkpoint: Checkpoint) -> Result<Subscription> {
        watch::subscribe(self.clone(), checkpoint)
    }
}
