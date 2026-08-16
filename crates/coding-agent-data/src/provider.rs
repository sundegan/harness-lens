use crate::{Batch, Checkpoint, ProviderInfo, Result};

/// A best-effort progress snapshot for one source scan.
///
/// Providers report file and line progress without doing a second full source
/// pass only for presentation. `estimated_total_lines` is therefore an
/// estimate, not a source contract.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanProgress {
    /// Number of source files discovered in the current snapshot.
    pub total_files: u64,
    /// Number of source files fully consumed by the checkpoint.
    pub processed_files: u64,
    /// Number of JSONL records consumed by the checkpoint.
    pub processed_lines: u64,
    /// Estimated number of JSONL records in the current source snapshot.
    pub estimated_total_lines: Option<u64>,
    /// Display-safe name of the file currently being consumed.
    pub current_file: Option<String>,
    /// One-based line currently being consumed in `current_file`.
    pub current_line: u64,
}

/// Whether a provider's durable local source contains a capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SourceCoverage {
    /// The source durably stores observations for this capability.
    Persisted,
    /// The source durably stores only part of the capability, such as terminal
    /// results without the complete runtime lifecycle.
    PartiallyPersisted,
    /// The runtime may expose the capability, but its inspected source does
    /// not durably store those observations.
    NotPersisted,
    /// The capability does not apply to this provider or source.
    NotApplicable,
    /// Persistence behavior has not been established from authoritative
    /// evidence.
    Unknown,
}

/// How completely an adapter maps a source capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AdapterCoverage {
    /// Durable source data is represented by provider-neutral semantic types.
    Normalized,
    /// Some durable observations are represented by provider-neutral semantic
    /// types, while other observations remain raw, use a less specific type,
    /// or are not retained.
    PartiallyNormalized,
    /// Durable source data is retained only through original or unknown data.
    RawOnly,
    /// Durable source data exists but is not retained by this adapter.
    Unsupported,
    /// No adapter mapping is meaningful because the source does not persist
    /// the capability or the capability does not apply.
    NotApplicable,
    /// Adapter behavior has not been established because the source format or
    /// capability has not been verified.
    Unknown,
}

/// Standard capability labels declared by the built-in providers.
///
/// Source coverage and adapter coverage are separate: a runtime feature may be
/// absent from durable storage, and durable source data may still be only
/// partially normalized. Built-in providers declare every label exactly once
/// so consumers can compare their coverage without treating an omitted row as
/// an implicit coverage state.
pub const STANDARD_CAPABILITIES: &[&str] = &[
    "session",
    "session_relation",
    "session_history",
    "message",
    "reasoning",
    "plan",
    "tool_execution",
    "file_change",
    "agent_invocation",
    "model_invocation",
    "task_artifact",
    "usage",
    "rate_limit",
    "compaction",
    "input_queue",
    "user_input_request",
    "hooks",
    "execution_context",
    "mode_change",
    "notice",
    "world_state",
    "goals",
    "approval",
    "retry",
    "model_reroute",
    "rollback",
    "fork_invocation_boundary",
    "streaming_delta",
    "os_file_audit",
    "unknown_provider_data",
];

/// Source and adapter coverage for one stable capability label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityCoverage {
    /// Stable provider-neutral capability label.
    pub capability: &'static str,
    /// Whether the provider's inspected source persists the capability.
    pub source: SourceCoverage,
    /// How the adapter handles the persisted source data.
    pub adapter: AdapterCoverage,
}

impl CapabilityCoverage {
    /// Creates a static capability declaration.
    pub const fn new(
        capability: &'static str,
        source: SourceCoverage,
        adapter: AdapterCoverage,
    ) -> Self {
        Self {
            capability,
            source,
            adapter,
        }
    }

    /// Returns whether the source and adapter states form a meaningful pair.
    ///
    /// A capability that is absent from durable storage cannot have an adapter
    /// mapping. Unknown source behavior must not be presented as a known
    /// adapter result. A known durable source may still have unknown adapter
    /// coverage until its mapping has been audited.
    pub const fn is_consistent(&self) -> bool {
        match self.source {
            SourceCoverage::NotPersisted | SourceCoverage::NotApplicable => {
                matches!(self.adapter, AdapterCoverage::NotApplicable)
            }
            SourceCoverage::Unknown => matches!(self.adapter, AdapterCoverage::Unknown),
            SourceCoverage::Persisted | SourceCoverage::PartiallyPersisted => {
                !matches!(self.adapter, AdapterCoverage::NotApplicable)
            }
        }
    }
}

/// Reads one coding-agent data source.
pub trait Provider: Send + Sync {
    /// Returns provider and source identity.
    fn info(&self) -> &ProviderInfo;

    /// Declares durable-source and adapter coverage.
    ///
    /// This intentionally distinguishes runtime capabilities from data that
    /// is actually available in the provider's inspected local source.
    /// Built-in providers return one declaration for each
    /// [`STANDARD_CAPABILITIES`] label.
    fn coverage(&self) -> &'static [CapabilityCoverage] {
        &[]
    }

    /// Starts or continues an incremental scan.
    ///
    /// Pass `None` to start from the beginning. When [`Batch::has_more`] is
    /// true, call `scan` again with the returned checkpoint.
    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<Batch>;
}

/// Provides a lightweight progress snapshot for a provider scan.
pub trait ScanProgressProvider: Provider {
    /// Computes progress from the source catalog and an optional checkpoint.
    ///
    /// Implementations should avoid parsing the source records here. This
    /// method is intended for UI/status updates before and between batches.
    fn scan_progress(&self, checkpoint: Option<&Checkpoint>) -> Result<ScanProgress>;
}

/// Extends a provider with live change monitoring.
pub trait WatchProvider: Provider {
    /// Starts monitoring after an already applied checkpoint.
    fn watch(&self, checkpoint: Checkpoint) -> Result<crate::Subscription>;
}
