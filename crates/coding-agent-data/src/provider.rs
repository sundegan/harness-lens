use crate::{ChangeBatch, Checkpoint, ProviderId, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProviderCapability {
    Discover,
    Snapshot,
    Incremental,
    Watch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub name: &'static str,
    pub capabilities: &'static [ProviderCapability],
}

/// Reads one coding-agent data source without exposing its private storage
/// schema to consumers.
pub trait AgentDataProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    /// Returns all records for `None`, or only changes after an existing
    /// provider checkpoint.
    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<ChangeBatch>;
}

#[cfg(feature = "watch")]
/// Extends a provider with debounced filesystem watching and reconciliation.
pub trait WatchableAgentDataProvider: AgentDataProvider {
    fn watch(
        &self,
        checkpoint: Checkpoint,
        options: crate::WatchOptions,
    ) -> Result<crate::DataWatcher>;
}
