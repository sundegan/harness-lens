mod checkpoint;
mod normalize;
mod source;
mod transcript;
#[cfg(feature = "claude-code-watch")]
mod watch;

use crate::providers::shared::identity::source_id_for_paths;
use crate::{
    AdapterCoverage, Batch, CapabilityCoverage, Checkpoint, Provider, ProviderId, ProviderInfo,
    Result, SourceCoverage,
};
#[cfg(feature = "claude-code-watch")]
use crate::{Subscription, WatchProvider};

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
        "tool_execution",
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
        "usage",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    ),
    CapabilityCoverage::new(
        "rate_limit",
        SourceCoverage::Persisted,
        AdapterCoverage::RawOnly,
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
        "hooks",
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
        SourceCoverage::Persisted,
        AdapterCoverage::RawOnly,
    ),
    CapabilityCoverage::new(
        "streaming_delta",
        SourceCoverage::Persisted,
        AdapterCoverage::RawOnly,
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

#[cfg(feature = "claude-code-watch")]
impl WatchProvider for ClaudeCodeProvider {
    fn watch(&self, checkpoint: Checkpoint) -> Result<Subscription> {
        watch::subscribe(self.clone(), checkpoint)
    }
}
