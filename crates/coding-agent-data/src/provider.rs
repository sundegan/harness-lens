use crate::{Batch, Checkpoint, ProviderInfo, Result};

/// Whether a provider's durable local source contains a capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SourceCoverage {
    /// The source durably stores observations for this capability.
    Persisted,
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
    /// Durable source data is retained only through original or unknown data.
    RawOnly,
    /// Durable source data exists but is not retained by this adapter.
    Unsupported,
    /// No adapter mapping is meaningful because the source does not persist
    /// the capability or the capability does not apply.
    NotApplicable,
}

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
}

/// Reads one coding-agent data source.
pub trait Provider: Send + Sync {
    /// Returns provider and source identity.
    fn info(&self) -> &ProviderInfo;

    /// Declares durable-source and adapter coverage.
    ///
    /// This intentionally distinguishes runtime capabilities from data that
    /// is actually available in the provider's inspected local source.
    fn coverage(&self) -> &'static [CapabilityCoverage] {
        &[]
    }

    /// Starts or continues an incremental scan.
    ///
    /// Pass `None` to start from the beginning. When [`Batch::has_more`] is
    /// true, call `scan` again with the returned checkpoint.
    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<Batch>;
}

/// Extends a provider with live change monitoring.
pub trait WatchProvider: Provider {
    /// Starts monitoring after an already applied checkpoint.
    fn watch(&self, checkpoint: Checkpoint) -> Result<crate::Subscription>;
}
