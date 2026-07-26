//! Consistent, read-only access to local coding-agent data for desktop
//! applications, CLIs, analytics tools, history viewers, and other Rust
//! consumers.
//!
//! The crate lets consumers build local browsing, indexing, synchronization,
//! analysis, and visualization features without having to adapt separately to
//! each agent's data locations, storage formats, and schemas. It discovers local
//! data sources and exposes their records through a common change model, opaque
//! incremental checkpoints, and optional live watchers.

mod error;
mod model;
mod provider;
#[cfg(feature = "watch")]
mod watch;

pub mod providers;

pub use error::{Error, Result};
pub use model::{
    ChangeBatch, Checkpoint, DataChange, DataKind, DataRecord, DataTimestamp, Diagnostic,
    DiagnosticSeverity, ProviderId, RecordKey, SourceLocation, SourceRef,
};
#[cfg(feature = "watch")]
pub use provider::WatchableAgentDataProvider;
pub use provider::{AgentDataProvider, ProviderCapability, ProviderDescriptor};
#[cfg(feature = "watch")]
pub use watch::{DataWatcher, WatchOptions};
