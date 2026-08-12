//! Read and normalize local coding-agent data without depending on
//! provider-private storage formats.
//!
//! Provider adapters discover supported sources and emit ordered [`Batch`]
//! values containing source-scoped [`Record`] changes, recoverable diagnostics,
//! and an opaque [`Checkpoint`]. Consumers own persistence, aggregation,
//! search, redaction, and presentation.
//!
//! # Example
//!
//! ```no_run
//! use coding_agent_data::{Checkpoint, Provider, Result};
//!
//! fn scan_all(provider: &impl Provider) -> Result<Checkpoint> {
//!     let mut checkpoint = None;
//!     loop {
//!         let batch = provider.scan(checkpoint.as_ref())?;
//!         for change in &batch.changes {
//!             println!("{change:?}");
//!         }
//!
//!         let has_more = batch.has_more;
//!         let next = batch.checkpoint;
//!         if !has_more {
//!             return Ok(next);
//!         }
//!         checkpoint = Some(next);
//!     }
//! # }
//! ```
//!
//! Pass the returned checkpoint to the next [`Provider::scan`] call. Continue
//! while [`Batch::has_more`] is true. Providers that implement
//! [`WatchProvider`] reconcile live source changes through the same scan
//! contract.

#![warn(missing_docs)]

mod error;
mod model;
mod provider;
mod subscription;

pub(crate) const DEFAULT_MAX_JSON_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Provider-neutral contracts for structural format inspection.
#[cfg(feature = "format-probe")]
pub mod format_probe;

/// Built-in coding-agent providers.
pub mod providers;

pub use error::{Error, Result};
pub use model::{
    Actor, AgentInvocation, AgentInvocationStatus, AgentOperation, ApprovalDecision,
    ApprovalOption, ApprovalOptionKind, ApprovalOutcome, ApprovalPolicy, ApprovalRequest, Batch,
    Change, Checkpoint, ContentAnnotations, ContentAudience, ContentBlock, ContentIcon,
    ContentIconTheme, ContentPriority, ContextCompaction, Cost, CreditBalance, DataQuality,
    Diagnostic, DiagnosticSeverity, Event, EventData, EventSequence, ExecutionContext, FileChange,
    FileChangeKind, ForkInvocationBoundary, Goal, GoalStatus, HistoryMode, HistoryPosition,
    HistorySegment, HookResult, HookStatus, InputQueueMutation, Message, MessagePhase, MessageRole,
    ModeChange, ModeChangeKind, ModelInvocation, ModelInvocationStatus, Notice, NoticeLevel,
    OriginalData, Plan, PlanStep, PlanStepPriority, PlanStepStatus, ProviderId, ProviderInfo,
    QueueOperation, RateLimit, RateLimitReason, RateLimitScope, RateLimitWindow, Reasoning,
    ReasoningVisibility, Record, RecordData, RecordId, Retry, Rollback, SandboxPolicy, Session,
    SessionHistory, SessionRelation, SessionRelationKind, SourceId, SourceLocation, SourceRef,
    SpendLimit, StopReason, TaskArtifact, Timestamp, TokenUsage, ToolCall, ToolKind, ToolLocation,
    ToolResult, ToolStatus, UnknownEvent, UnknownRecord, UsageReport, WorldState,
};
pub use provider::{
    AdapterCoverage, CapabilityCoverage, Provider, SourceCoverage, WatchProvider,
    STANDARD_CAPABILITIES,
};
pub use subscription::Subscription;
