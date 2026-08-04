use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// Identifies one coding-agent data provider.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    /// Creates a provider identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identifies one concrete local data-source instance.
///
/// Two installations of the same coding agent have different source IDs, so
/// their records cannot collide.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceId(String);

impl SourceId {
    /// Creates a source identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identifies one normalized record within a concrete source.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordId(String);

impl RecordId {
    /// Creates a record identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Creates an identifier scoped by its source, semantic kind, and
    /// provider-local identity.
    pub fn scoped(source: &SourceId, kind: &str, local: impl AsRef<str>) -> Self {
        Self(format!("{}:{kind}:{}", source.as_str(), local.as_ref()))
    }

    /// Returns whether this identifier belongs to the given concrete source.
    pub fn is_scoped_to(&self, source: &SourceId) -> bool {
        self.0
            .strip_prefix(source.as_str())
            .is_some_and(|suffix| suffix.starts_with(':'))
    }
}

/// Describes a provider and its concrete source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInfo {
    /// Stable provider identifier.
    pub id: ProviderId,
    /// Human-readable provider name.
    pub name: &'static str,
    /// Stable identifier for this local source instance.
    pub source: SourceId,
}

/// A UTC instant represented as Unix milliseconds.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Creates a timestamp from Unix milliseconds.
    pub const fn from_millis(value: i64) -> Self {
        Self(value)
    }

    /// Creates a timestamp from Unix seconds.
    pub const fn from_seconds(value: i64) -> Self {
        Self(value.saturating_mul(1000))
    }

    /// Returns Unix milliseconds.
    pub const fn as_millis(self) -> i64 {
        self.0
    }
}

/// Indicates how complete a normalized record is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DataQuality {
    /// The provider read the authoritative local index for this record.
    Complete,
    /// The provider reconstructed a useful subset from an event stream.
    Partial,
}

/// Locates a record inside its original artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocation {
    /// A record in a local database.
    DatabaseRecord {
        /// Provider-owned key used to locate the record.
        key: String,
    },
    /// A line in a JSONL artifact.
    JsonLine {
        /// One-based line number.
        line: u64,
        /// Byte offset at which the line starts, when available.
        byte_start: Option<u64>,
        /// Byte offset immediately after the line, when available.
        byte_end: Option<u64>,
    },
    /// The whole artifact.
    WholeFile,
}

/// Points to the original local artifact from which a record was read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Concrete coding-agent source that owns the artifact.
    pub source: SourceId,
    /// Local artifact path.
    pub path: PathBuf,
    /// Position inside the artifact.
    pub location: SourceLocation,
}

impl SourceRef {
    /// Points to an entire source artifact.
    pub fn whole_file(source: SourceId, path: impl Into<PathBuf>) -> Self {
        Self {
            source,
            path: path.into(),
            location: SourceLocation::WholeFile,
        }
    }

    /// Points to one provider-owned database record.
    pub fn database_record(
        source: SourceId,
        path: impl Into<PathBuf>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            source,
            path: path.into(),
            location: SourceLocation::DatabaseRecord { key: key.into() },
        }
    }

    /// Points to one line in a JSONL artifact.
    pub fn json_line(
        source: SourceId,
        path: impl Into<PathBuf>,
        line: u64,
        byte_start: Option<u64>,
        byte_end: Option<u64>,
    ) -> Self {
        Self {
            source,
            path: path.into(),
            location: SourceLocation::JsonLine {
                line,
                byte_start,
                byte_end,
            },
        }
    }
}

/// Preserves provider-native input for forward compatibility.
///
/// Consumers should prefer [`Record::data`]. The original value exists so
/// unknown fields are not lost while a provider catches up with a new format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OriginalData {
    /// Provider-owned format label.
    pub format: String,
    /// Original JSON-compatible value.
    pub value: Value,
}

/// Kind of relationship from the containing session to a referenced session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SessionRelationKind {
    /// The session was forked from an earlier session.
    Fork,
    /// The session was created as a child or subagent of the referenced session.
    Child,
    /// The session continues the referenced session in a new provider-owned container.
    Continuation,
    /// A provider-native relationship with no normalized equivalent yet.
    Other(String),
}

/// A normalized relationship to another session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionRelation {
    /// How the containing session relates to the referenced session.
    pub kind: SessionRelationKind,
    /// Related session record.
    pub session: RecordId,
}

impl SessionRelation {
    /// Creates a relationship to another normalized session.
    pub fn new(kind: SessionRelationKind, session: RecordId) -> Self {
        Self { kind, session }
    }
}

/// Provider storage strategy for one session's durable history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum HistoryMode {
    /// The transcript is self-contained and may include legacy projections.
    Legacy,
    /// The transcript uses logical ordinals and can reference an ancestor
    /// rollout prefix.
    Paginated,
    /// A provider-native history mode not normalized yet.
    Other(String),
}

/// Exclusive logical position in another session's history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryPosition {
    /// Session that owns the referenced physical history.
    pub session: RecordId,
    /// First logical ordinal not included from the referenced session.
    pub end_ordinal_exclusive: u64,
    /// Byte offset immediately after the last included source record.
    pub end_byte_offset: u64,
}

/// One physical session range contributing to a logical session history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistorySegment {
    /// Session whose transcript owns this range.
    pub session: RecordId,
    /// First logical ordinal read from the owning session.
    pub start_ordinal: u64,
    /// First logical ordinal not included from this session, or no cutoff for
    /// the final segment.
    pub end_ordinal_exclusive: Option<u64>,
}

/// Durable history and lineage metadata for one session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionHistory {
    /// Storage strategy used by the provider.
    pub mode: HistoryMode,
    /// Frozen inherited prefix, when the session references an ancestor.
    pub base: Option<HistoryPosition>,
    /// First logical ordinal projected as this subagent's own activity.
    pub own_start_ordinal: Option<u64>,
    /// Initial provider context-window identity.
    pub context_window_id: Option<String>,
    /// Root-to-leaf physical ranges that materialize the logical history.
    ///
    /// An empty value means the provider does not use lineage or the adapter
    /// could not safely resolve every referenced segment.
    #[serde(default)]
    pub lineage: Vec<HistorySegment>,
}

/// A normalized coding-agent session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Session {
    /// Identifier used by the coding agent inside this source.
    pub external_id: String,
    /// Best available display title.
    pub title: Option<String>,
    /// Working directory associated with the session.
    pub cwd: Option<PathBuf>,
    /// Primary transcript artifact, when known.
    pub transcript: Option<PathBuf>,
    /// Session creation time.
    pub created_at: Option<Timestamp>,
    /// Most recent session update time.
    pub updated_at: Option<Timestamp>,
    /// Latest provider-reported session total.
    ///
    /// This is a cumulative snapshot and is not additive across sessions.
    pub total_tokens: Option<i64>,
    /// Whether the provider reports the session as archived.
    pub archived: bool,
    /// Latest model name, when available.
    pub model: Option<String>,
    /// Model service or vendor, when reported.
    #[serde(default)]
    pub model_provider: Option<String>,
    /// Coding-agent version that produced the session.
    #[serde(default)]
    pub agent_version: Option<String>,
    /// Agent display name or nickname, when reported.
    #[serde(default)]
    pub agent_name: Option<String>,
    /// Agent role, when reported.
    #[serde(default)]
    pub agent_role: Option<String>,
    /// Git branch associated with the session.
    pub git_branch: Option<String>,
    /// Git commit associated with the session.
    #[serde(default)]
    pub git_commit: Option<String>,
    /// Git remote URL associated with the session.
    #[serde(default)]
    pub git_remote_url: Option<String>,
    /// Relationships to parent or otherwise related sessions.
    #[serde(default)]
    pub relations: Vec<SessionRelation>,
    /// Durable history mode and lineage metadata.
    #[serde(default)]
    pub history: Option<SessionHistory>,
    /// Namespaced provider metadata that does not have a stable
    /// provider-neutral field.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_attributes: BTreeMap<String, Value>,
    /// Completeness of this normalized session.
    pub quality: DataQuality,
}

impl Session {
    /// Creates a partial session with only its provider-native identifier.
    pub fn new(external_id: impl Into<String>) -> Self {
        Self {
            external_id: external_id.into(),
            title: None,
            cwd: None,
            transcript: None,
            created_at: None,
            updated_at: None,
            total_tokens: None,
            archived: false,
            model: None,
            model_provider: None,
            agent_version: None,
            agent_name: None,
            agent_role: None,
            git_branch: None,
            git_commit: None,
            git_remote_url: None,
            relations: Vec::new(),
            history: None,
            provider_attributes: BTreeMap::new(),
            quality: DataQuality::Partial,
        }
    }
}

/// Current lifecycle state of a turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    /// The turn has started and has no terminal event yet.
    InProgress,
    /// The turn completed successfully.
    Completed,
    /// The turn completed with an error.
    Failed,
    /// The turn was cancelled before normal completion.
    Cancelled,
    /// The turn was interrupted and may be resumable.
    Interrupted,
    /// The provider reported a state that has no normalized equivalent yet.
    Unknown,
}

/// Explains why a turn stopped.
///
/// See [ACP v2 stop reasons](https://agentclientprotocol.com/protocol/v2/prompt-lifecycle#stop-reasons).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The agent completed the requested work normally.
    EndTurn,
    /// The model reached its token limit.
    MaxTokens,
    /// The agent reached its turn-request limit.
    MaxTurnRequests,
    /// The provider-defined stop sequence was generated.
    StopSequence,
    /// The model paused after requesting one or more tools.
    ToolUse,
    /// The model paused and expects the caller to continue the request.
    PauseTurn,
    /// The model reached its context-window limit.
    ContextWindowExceeded,
    /// The model refused the request.
    Refusal,
    /// The user or client cancelled the turn.
    Cancelled,
    /// Execution was interrupted but was not explicitly cancelled.
    Interrupted,
    /// Execution stopped because of an error.
    Failed,
    /// A provider-native reason that has no normalized equivalent yet.
    Other(String),
}

/// A normalized coding-agent turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    /// Provider-native turn identifier, when present.
    pub external_id: Option<String>,
    /// Current lifecycle state.
    pub status: TurnStatus,
    /// Turn start time.
    pub started_at: Option<Timestamp>,
    /// Turn completion time.
    pub completed_at: Option<Timestamp>,
    /// Provider-reported or derived duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Terminal error message, when available.
    pub error: Option<String>,
    /// Normalized terminal reason, when the provider reports or implies one.
    pub stop_reason: Option<StopReason>,
    /// Provider trace identifier, when persisted.
    #[serde(default)]
    pub trace_id: Option<String>,
    /// Model context-window size associated with the turn, when reported.
    #[serde(default)]
    pub model_context_window: Option<i64>,
    /// Time from turn start until the first model token, in milliseconds.
    #[serde(default)]
    pub time_to_first_token_ms: Option<i64>,
}

/// Normalized token counters.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Total tokens.
    pub total: i64,
    /// Input tokens, when available.
    pub input: Option<i64>,
    /// Input tokens written to a provider prompt cache, when reported separately.
    pub cache_creation_input: Option<i64>,
    /// Input tokens written to the short-lived five-minute prompt cache.
    #[serde(default)]
    pub cache_creation_ephemeral_5m_input: Option<i64>,
    /// Input tokens written to the one-hour prompt cache.
    #[serde(default)]
    pub cache_creation_ephemeral_1h_input: Option<i64>,
    /// Cached input tokens, when available.
    pub cached_input: Option<i64>,
    /// Output tokens, when available.
    pub output: Option<i64>,
    /// Reasoning output tokens, when available.
    pub reasoning_output: Option<i64>,
}

/// A provider-reported monetary cost.
///
/// The decimal amount is stored as text so normalization never introduces
/// binary floating-point rounding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    /// Decimal amount in the stated currency.
    pub amount: String,
    /// ISO 4217 currency code, such as `USD`.
    pub currency: String,
}

/// A provider usage observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Model service or vendor attributed to this observation.
    #[serde(default)]
    pub model_provider: Option<String>,
    /// Model identifier attributed to this observation.
    #[serde(default)]
    pub model: Option<String>,
    /// Provider service tier attributed to this observation.
    #[serde(default)]
    pub service_tier: Option<String>,
    /// Provider request identifier, when reported.
    #[serde(default)]
    pub request_id: Option<String>,
    /// Provider model-invocation or message identifier, when reported.
    #[serde(default)]
    pub invocation_id: Option<String>,
    /// Provider-reported cumulative usage for the session, when available.
    ///
    /// Consumers replace this snapshot; they never add cumulative values.
    pub cumulative: Option<TokenUsage>,
    /// Additive usage attributed only to the current observation, when available.
    ///
    /// Replayed observations copied into a fork have no delta.
    pub delta: Option<TokenUsage>,
    /// Monetary cost associated with this observation, when reported.
    pub cost: Option<Cost>,
}

/// One rolling rate-limit window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RateLimitWindow {
    /// Provider-neutral window label, such as `primary` or `secondary`.
    pub name: String,
    /// Percentage of the window already consumed.
    pub used_percent: Option<f64>,
    /// Rolling-window duration in minutes, when reported.
    pub window_minutes: Option<i64>,
    /// Time at which the window resets.
    pub resets_at: Option<Timestamp>,
}

/// Remaining provider credits.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreditBalance {
    /// Whether the provider reports that credits are available.
    pub has_credits: Option<bool>,
    /// Whether credit consumption is unlimited.
    pub unlimited: Option<bool>,
    /// Provider-formatted balance, when available.
    pub balance: Option<String>,
}

/// A provider-enforced monetary or organizational spending limit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SpendLimit {
    /// Provider-formatted limit.
    pub limit: Option<String>,
    /// Provider-formatted amount already used.
    pub used: Option<String>,
    /// Percentage of the limit remaining.
    pub remaining_percent: Option<i32>,
    /// Time at which the limit resets.
    pub resets_at: Option<Timestamp>,
}

/// Why a provider reports that a usage limit has been reached.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RateLimitReason {
    /// A rolling request or token limit was reached.
    RateLimit,
    /// Available credits were depleted.
    CreditsDepleted,
    /// An account or workspace usage limit was reached.
    UsageLimit,
    /// A provider-native reason without a normalized equivalent.
    Other(String),
}

/// Account scope associated with a reached usage limit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum RateLimitScope {
    /// Workspace owner scope.
    WorkspaceOwner,
    /// Workspace member scope.
    WorkspaceMember,
    /// A provider-native scope without a normalized equivalent.
    Other(String),
}

/// A provider-reported usage-limit snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RateLimit {
    /// Provider-native limit identifier.
    pub external_id: Option<String>,
    /// Human-readable limit name.
    pub name: Option<String>,
    /// Rolling usage windows.
    pub windows: Vec<RateLimitWindow>,
    /// Remaining credits, when reported.
    pub credits: Option<CreditBalance>,
    /// Individual or organization spending limit, when reported.
    pub spend_limit: Option<SpendLimit>,
    /// Whether the provider reports that spending control blocked execution.
    pub spend_control_reached: Option<bool>,
    /// Provider plan or account tier.
    pub plan: Option<String>,
    /// Normalized reason the limit was reached.
    pub reached_reason: Option<RateLimitReason>,
    /// Account scope associated with the reached reason.
    pub reached_scope: Option<RateLimitScope>,
}

/// Identifies who caused an item to exist.
///
/// This is intentionally separate from [`MessageRole`]. For example, a tool
/// result can be stored inside a provider-native `user` message while still
/// being an environment-produced item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// A human user or client acting on the user's behalf.
    User,
    /// The primary coding agent or one of its subagents.
    Agent,
    /// System instructions or runtime control logic.
    System,
    /// The execution environment outside the model and agent.
    Environment,
    /// A tool or tool server.
    Tool,
    /// A provider-native actor that has no normalized equivalent yet.
    Other(String),
}

/// A stable order within one source artifact.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ItemSequence {
    /// Monotonic provider or artifact position.
    pub position: u64,
    /// Order of a derived item at the same position.
    pub part: u32,
    /// Provider logical ordinal, distinct from the physical artifact line.
    #[serde(default)]
    pub logical_ordinal: Option<u64>,
}

impl ItemSequence {
    /// Creates a source-local item sequence.
    pub const fn new(position: u64, part: u32) -> Self {
        Self {
            position,
            part,
            logical_ordinal: None,
        }
    }

    /// Creates a source-local sequence carrying a provider logical ordinal.
    pub const fn with_logical_ordinal(
        position: u64,
        part: u32,
        logical_ordinal: Option<u64>,
    ) -> Self {
        Self {
            position,
            part,
            logical_ordinal,
        }
    }

    /// Returns the same provider position with a different derived-part order.
    pub const fn with_part(self, part: u32) -> Self {
        Self { part, ..self }
    }
}

/// A normalized message role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// System instructions.
    System,
    /// Developer-authored instructions with precedence below system messages.
    Developer,
    /// Human input.
    User,
    /// Model or agent output.
    Assistant,
    /// Tool output represented as a conversation message.
    Tool,
    /// A provider-native role that has no normalized equivalent yet.
    Other(String),
}

/// The presentation phase of an assistant message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum MessagePhase {
    /// Intermediate progress or commentary.
    Commentary,
    /// The final answer for a turn.
    FinalAnswer,
    /// A provider-native phase that has no normalized equivalent yet.
    Other(String),
}

/// Intended recipient of a content block.
///
/// See [ACP v2 content annotations](https://agentclientprotocol.com/protocol/v2/schema#annotations).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ContentAudience {
    /// Content intended for the human user.
    User,
    /// Content intended for the assistant.
    Assistant,
    /// A provider-native, custom, or future audience value.
    Other(String),
}

/// Relative content importance constrained to the inclusive `0..=1` range.
///
/// See [ACP v2 content annotations](https://agentclientprotocol.com/protocol/v2/schema#annotations).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ContentPriority(f64);

impl ContentPriority {
    /// Creates a valid priority.
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(Self(value))
    }

    /// Returns the priority as a floating-point value.
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for ContentPriority {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| {
            <D::Error as serde::de::Error>::custom(
                "content priority must be finite and within 0..=1",
            )
        })
    }
}

/// Display and routing hints attached to a content block.
///
/// See [ACP v2 content annotations](https://agentclientprotocol.com/protocol/v2/schema#annotations).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ContentAnnotations {
    /// Intended recipients.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience: Vec<ContentAudience>,
    /// Last modification time of the underlying resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<Timestamp>,
    /// Relative importance in the inclusive range from zero to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<ContentPriority>,
}

/// Theme for which a resource icon was designed.
///
/// See [ACP v2 resource links](https://agentclientprotocol.com/protocol/v2/content#resource-link).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ContentIconTheme {
    /// Icon intended for a light background.
    Light,
    /// Icon intended for a dark background.
    Dark,
    /// A provider-native, custom, or future theme value.
    Other(String),
}

/// One display icon associated with a linked resource.
///
/// See [ACP v2 resource links](https://agentclientprotocol.com/protocol/v2/content#resource-link).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ContentIcon {
    /// URI of the icon resource.
    pub src: String,
    /// Media type, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Supported sizes such as `48x48` or `any`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sizes: Vec<String>,
    /// Intended display theme, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ContentIconTheme>,
}

impl ContentIcon {
    /// Creates an icon with only its required source URI.
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            mime_type: None,
            sizes: Vec::new(),
            theme: None,
        }
    }
}

/// One typed block of message or tool-result content.
///
/// See [ACP v2 content blocks](https://agentclientprotocol.com/protocol/v2/content).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// UTF-8 text.
    Text {
        /// Text content.
        text: String,
        /// Display and routing hints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<ContentAnnotations>,
    },
    /// Image content stored inline or referenced by URI.
    Image {
        /// Media type, when known.
        mime_type: Option<String>,
        /// URI or local reference, when present.
        uri: Option<String>,
        /// Provider-encoded inline data, when present.
        data: Option<String>,
        /// Display and routing hints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<ContentAnnotations>,
    },
    /// Audio content stored inline or referenced by URI.
    Audio {
        /// Media type, when known.
        mime_type: Option<String>,
        /// URI or local reference, when present.
        uri: Option<String>,
        /// Provider-encoded inline data, when present.
        data: Option<String>,
        /// Display and routing hints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<ContentAnnotations>,
    },
    /// An embedded resource.
    Resource {
        /// Resource URI.
        uri: String,
        /// Media type, when known.
        mime_type: Option<String>,
        /// Text representation, when present.
        text: Option<String>,
        /// Provider-encoded binary representation, when present.
        data: Option<String>,
        /// Display and routing hints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<ContentAnnotations>,
    },
    /// A link to a resource that can be fetched separately.
    ResourceLink {
        /// Resource URI.
        uri: String,
        /// Display name, when present.
        name: Option<String>,
        /// Display title, when present.
        title: Option<String>,
        /// Human-readable description, when present.
        description: Option<String>,
        /// Media type, when known.
        mime_type: Option<String>,
        /// Resource size in bytes, when known.
        size: Option<u64>,
        /// Icons suitable for displaying the resource.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        icons: Vec<ContentIcon>,
        /// Display and routing hints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        annotations: Option<ContentAnnotations>,
    },
    /// Provider-native, custom, or future content not normalized yet.
    Unknown {
        /// Best available provider-native type name.
        kind: Option<String>,
        /// Original block value.
        value: Value,
    },
}

impl ContentBlock {
    /// Creates a text content block.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text {
            text: value.into(),
            annotations: None,
        }
    }
}

/// A normalized conversation message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Normalized conversation role.
    pub role: MessageRole,
    /// Optional presentation phase.
    pub phase: Option<MessagePhase>,
    /// Ordered message content.
    pub content: Vec<ContentBlock>,
}

/// Model reasoning that a provider made available to consumers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reasoning {
    /// Provider-produced reasoning summaries.
    pub summary: Vec<String>,
    /// Ordered visible reasoning content.
    pub content: Vec<ContentBlock>,
    /// Availability of the provider's detailed reasoning content.
    #[serde(default)]
    pub visibility: ReasoningVisibility,
}

/// Availability of detailed model reasoning.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ReasoningVisibility {
    /// Detailed reasoning content is available when the provider persisted it.
    #[default]
    Visible,
    /// The provider persisted a redacted reasoning marker.
    Redacted,
    /// The provider persisted encrypted reasoning that this crate does not decrypt.
    Encrypted,
}

/// Current lifecycle state of a plan step.
///
/// See [ACP v2 agent plans](https://agentclientprotocol.com/protocol/v2/agent-plan).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    /// The step has not started.
    Pending,
    /// The step is currently being worked on.
    InProgress,
    /// The step completed.
    Completed,
    /// The step was cancelled.
    Cancelled,
    /// The step failed.
    Failed,
    /// The step was intentionally skipped.
    Skipped,
    /// The provider reported a state that has no normalized equivalent yet.
    Unknown,
}

/// Relative priority of a plan step.
///
/// See [ACP v2 agent plans](https://agentclientprotocol.com/protocol/v2/agent-plan).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum PlanStepPriority {
    /// High priority.
    High,
    /// Medium priority.
    Medium,
    /// Low priority.
    Low,
}

/// One step in an agent plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlanStep {
    /// Human-readable step text.
    pub text: String,
    /// Current lifecycle state.
    pub status: PlanStepStatus,
    /// Relative priority, when reported.
    pub priority: Option<PlanStepPriority>,
}

/// A normalized agent plan.
///
/// See [ACP v2 agent plans](https://agentclientprotocol.com/protocol/v2/agent-plan).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// Free-form plan text, when the provider does not expose structured steps.
    pub text: Option<String>,
    /// Ordered structured steps, when available.
    pub steps: Vec<PlanStep>,
}

/// Broad category of a tool operation.
///
/// See [ACP v2 tool kinds](https://agentclientprotocol.com/protocol/v2/tool-calls#reporting).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Read files or data.
    Read,
    /// Modify files or content.
    Edit,
    /// Remove files or data.
    Delete,
    /// Move or rename files or data.
    Move,
    /// Search for local or remote information.
    Search,
    /// Execute commands or code.
    Execute,
    /// Perform explicit reasoning or planning.
    Think,
    /// Retrieve external data.
    Fetch,
    /// Change the agent's operating mode.
    SwitchMode,
    /// A tool that does not fit a standard category.
    Other,
}

/// Current lifecycle state of a tool invocation.
///
/// See [ACP v2 tool-call status](https://agentclientprotocol.com/protocol/v2/tool-calls#status).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    /// The invocation is waiting to start.
    Pending,
    /// The invocation is waiting for user authorization.
    AwaitingApproval,
    /// The tool is executing.
    InProgress,
    /// The tool completed successfully.
    Completed,
    /// The tool failed.
    Failed,
    /// The invocation was cancelled.
    Cancelled,
    /// The user or policy declined execution.
    Declined,
    /// The provider reported a state that has no normalized equivalent yet.
    Unknown,
}

/// A file location affected or inspected by a tool.
///
/// See [ACP v2 tool-call locations](https://agentclientprotocol.com/protocol/v2/tool-calls#following-the-agent).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolLocation {
    /// File path.
    pub path: PathBuf,
    /// One-based line number, when known.
    pub line: Option<u64>,
}

/// A normalized tool invocation request.
///
/// See [ACP v2 tool calls](https://agentclientprotocol.com/protocol/v2/tool-calls).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-native call identifier.
    pub call_id: String,
    /// Tool name.
    pub name: String,
    /// Tool server, namespace, or plugin name, when reported separately.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Human-readable operation title, when reported.
    pub title: Option<String>,
    /// Broad operation category.
    pub kind: ToolKind,
    /// Invocation state at the time of this observation.
    pub status: ToolStatus,
    /// Tool input represented as JSON-compatible data.
    pub input: Value,
    /// Files reported as affected or inspected.
    pub locations: Vec<ToolLocation>,
}

/// A normalized tool invocation result.
///
/// See [ACP v2 tool-call updates](https://agentclientprotocol.com/protocol/v2/tool-calls#updating).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Provider-native call identifier.
    pub call_id: String,
    /// Tool name, when repeated by the provider.
    pub name: Option<String>,
    /// Structured or provider-native tool output.
    pub output: Value,
    /// Ordered displayable content.
    pub content: Vec<ContentBlock>,
    /// Terminal invocation state.
    pub status: ToolStatus,
    /// Provider-reported error message, when available.
    pub error: Option<String>,
    /// Provider-reported or derived duration in milliseconds.
    pub duration_ms: Option<i64>,
}

/// Kind of user authorization option.
///
/// See [ACP v2 permission requests](https://agentclientprotocol.com/protocol/v2/tool-calls#requesting-permission).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOptionKind {
    /// Allow this invocation once.
    AllowOnce,
    /// Allow this and remember the decision.
    AllowAlways,
    /// Reject this invocation once.
    RejectOnce,
    /// Reject this and remember the decision.
    RejectAlways,
    /// A provider-native option with no normalized equivalent yet.
    Other,
}

/// One option offered by an approval request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovalOption {
    /// Provider-native option identifier.
    pub id: String,
    /// Human-readable option label.
    pub name: String,
    /// Behavioral category.
    pub kind: ApprovalOptionKind,
}

/// A request for user authorization.
///
/// See [ACP v2 permission requests](https://agentclientprotocol.com/protocol/v2/tool-calls#requesting-permission).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Provider-native request identifier.
    pub request_id: String,
    /// Related tool invocation, when applicable.
    pub call_id: Option<String>,
    /// Ordered available decisions.
    pub options: Vec<ApprovalOption>,
}

/// Outcome of an approval request.
///
/// See [ACP v2 permission outcomes](https://agentclientprotocol.com/protocol/v2/tool-calls#requesting-permission).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    /// The user or policy selected an option.
    Selected {
        /// Selected provider-native option identifier.
        option_id: String,
    },
    /// The request was cancelled without a selection.
    Cancelled,
    /// A provider-native outcome that has no normalized equivalent yet.
    Other(String),
}

/// The resolved decision for an approval request.
///
/// See [ACP v2 permission outcomes](https://agentclientprotocol.com/protocol/v2/tool-calls#requesting-permission).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApprovalDecision {
    /// Provider-native request identifier.
    pub request_id: String,
    /// Resolution outcome.
    pub outcome: ApprovalOutcome,
}

/// Current lifecycle state of one model invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ModelInvocationStatus {
    /// The invocation has not started.
    Pending,
    /// The model is producing output.
    InProgress,
    /// The invocation completed.
    Completed,
    /// The invocation failed.
    Failed,
    /// The invocation was cancelled.
    Cancelled,
    /// The invocation was interrupted and may be resumable.
    Interrupted,
    /// The provider reported a state that has no normalized equivalent yet.
    Unknown,
}

/// One request to a language model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelInvocation {
    /// Provider-native invocation identifier, when present.
    pub invocation_id: Option<String>,
    /// Model service or vendor, when reported.
    pub provider: Option<String>,
    /// Provider service tier used for this request, when reported.
    #[serde(default)]
    pub service_tier: Option<String>,
    /// Model identifier, when reported.
    pub model: Option<String>,
    /// Current lifecycle state.
    pub status: ModelInvocationStatus,
    /// Token usage attributed to this invocation, when available.
    pub usage: Option<TokenUsage>,
    /// Monetary cost attributed to this invocation, when available.
    pub cost: Option<Cost>,
    /// Terminal reason, when available.
    pub stop_reason: Option<StopReason>,
    /// Provider-reported or derived duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Terminal error message, when available.
    pub error: Option<String>,
}

/// Kind of collaboration between coding agents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum AgentOperation {
    /// Invoke an already-addressable agent.
    Invoke,
    /// Start a child or peer agent.
    Spawn,
    /// Delegate work to another agent.
    Delegate,
    /// Transfer responsibility and context to another agent.
    Handoff,
    /// Send additional input to an existing agent.
    SendInput,
    /// Wait for another agent.
    Wait,
    /// Resume a previously paused agent.
    Resume,
    /// Close or stop another agent.
    Close,
    /// A provider-native collaboration operation.
    Other(String),
}

/// Current lifecycle state of an agent invocation.
///
/// See [A2A task lifecycle](https://a2a-protocol.org/latest/topics/life-of-a-task/).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum AgentInvocationStatus {
    /// The invocation is waiting to start.
    Pending,
    /// The agent is working.
    InProgress,
    /// The agent needs more user or caller input.
    InputRequired,
    /// The agent needs authorization.
    AuthorizationRequired,
    /// The agent completed successfully.
    Completed,
    /// The agent failed.
    Failed,
    /// The invocation was cancelled.
    Cancelled,
    /// The invocation was rejected before execution.
    Rejected,
    /// The invocation was interrupted and may be resumable.
    Interrupted,
    /// The provider reported a state that has no normalized equivalent yet.
    Unknown,
}

/// Policy controlling when tool execution requires approval.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Only trusted operations can run without approval.
    Untrusted,
    /// Ask before operations that require elevated access.
    OnRequest,
    /// Ask only after a sandboxed operation fails.
    OnFailure,
    /// Never ask for approval.
    Never,
    /// A provider-native policy with no normalized equivalent yet.
    Other(String),
}

/// File-system and process isolation applied to an execution context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SandboxPolicy {
    /// Reads are allowed but writes are blocked.
    ReadOnly,
    /// Writes are limited to configured workspace roots.
    WorkspaceWrite,
    /// The execution has unrestricted local access.
    FullAccess,
    /// A provider-native policy with no normalized equivalent yet.
    Other(String),
}

/// Execution settings observed for a session or turn.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ExecutionContext {
    /// Active working directory.
    pub cwd: Option<PathBuf>,
    /// Workspace roots available to the agent.
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    /// Active model identifier.
    pub model: Option<String>,
    /// Active model service or vendor.
    pub model_provider: Option<String>,
    /// Provider service tier active for this context.
    #[serde(default)]
    pub service_tier: Option<String>,
    /// Active model context-window size.
    pub model_context_window: Option<i64>,
    /// Provider-native reasoning-effort label.
    pub reasoning_effort: Option<String>,
    /// Provider-native reasoning-summary mode.
    #[serde(default)]
    pub reasoning_summary: Option<String>,
    /// Provider-native model personality.
    #[serde(default)]
    pub personality: Option<String>,
    /// Date reported to the agent.
    pub current_date: Option<String>,
    /// Time zone reported to the agent.
    pub timezone: Option<String>,
    /// Active approval policy.
    pub approval_policy: Option<ApprovalPolicy>,
    /// Actor responsible for reviewing approval requests.
    #[serde(default)]
    pub approvals_reviewer: Option<String>,
    /// Active sandbox policy.
    pub sandbox_policy: Option<SandboxPolicy>,
    /// Complete provider-native permission profile.
    #[serde(default)]
    pub permission_profile: Option<Value>,
    /// Provider-selected active permission-profile descriptor.
    #[serde(default)]
    pub active_permission_profile: Option<Value>,
    /// Provider-native collaboration mode label.
    pub collaboration_mode: Option<String>,
    /// Additional provider-specific execution settings that have stable
    /// analytical value but no provider-neutral field yet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_attributes: BTreeMap<String, Value>,
}

/// How an execution mode changed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ModeChangeKind {
    /// The agent entered a temporary mode.
    Entered,
    /// The agent exited a temporary mode.
    Exited,
    /// The user or agent selected a mode.
    Selected,
    /// A provider-native transition with no normalized equivalent yet.
    Other(String),
}

/// A change to the agent's active execution mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModeChange {
    /// Provider-reported mode identifier.
    pub mode: String,
    /// Kind of transition.
    pub kind: ModeChangeKind,
    /// Human-readable explanation, when reported.
    pub description: Option<String>,
}

/// Severity of a provider-reported execution notice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    /// Informational execution state.
    Info,
    /// Recoverable warning.
    Warning,
    /// Execution error.
    Error,
}

/// A user-visible notice reported while the agent was running.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    /// Notice severity.
    pub level: NoticeLevel,
    /// Provider-reported machine-readable code, when available.
    pub code: Option<String>,
    /// Human-readable message.
    pub message: String,
}

/// Terminal status of one hook execution or aggregate hook run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    /// The hook completed without persisted failure evidence.
    Completed,
    /// At least one hook failed.
    Failed,
    /// The hook prevented the agent from continuing.
    Blocked,
    /// The provider did not expose a conclusive state.
    Unknown,
}

/// A persisted hook-execution result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HookResult {
    /// Hook lifecycle event, such as `stop`.
    pub event: Option<String>,
    /// Provider entrypoint or hook group.
    pub entrypoint: Option<String>,
    /// Related tool invocation, when present.
    pub tool_call_id: Option<String>,
    /// Number of hooks represented by this result.
    pub count: Option<u64>,
    /// Terminal hook state.
    pub status: HookStatus,
    /// Whether hook policy prevented the agent from continuing.
    pub prevented_continuation: Option<bool>,
    /// Provider-reported stop reason.
    pub stop_reason: Option<String>,
    /// Structured informational hook results.
    pub infos: Vec<Value>,
    /// Structured hook errors.
    pub errors: Vec<Value>,
    /// Additional context produced by the hooks.
    pub context: Vec<ContentBlock>,
}

/// A deliverable produced by an inter-agent task.
///
/// See [A2A artifacts](https://a2a-protocol.org/latest/topics/key-concepts/#artifacts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskArtifact {
    /// Provider-native artifact identifier, when present.
    pub artifact_id: Option<String>,
    /// Human-readable artifact name, when present.
    pub name: Option<String>,
    /// Human-readable artifact description, when present.
    pub description: Option<String>,
    /// Ordered artifact content.
    pub parts: Vec<ContentBlock>,
    /// Provider-neutral structured metadata, when present.
    pub metadata: Option<Value>,
}

/// One invocation of a child or peer agent.
///
/// See [A2A tasks](https://a2a-protocol.org/latest/topics/key-concepts/#task).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentInvocation {
    /// Provider-native invocation identifier.
    pub invocation_id: String,
    /// Inter-agent context identifier, when reported.
    pub context_id: Option<String>,
    /// Durable inter-agent task identifier, when reported.
    pub task_id: Option<String>,
    /// Collaboration operation.
    pub operation: AgentOperation,
    /// Sender agent or thread identifier, when reported.
    pub sender_id: Option<String>,
    /// Receiver agent or thread identifiers.
    pub receiver_ids: Vec<String>,
    /// Child session exposed by the provider, when known.
    pub child_session: Option<RecordId>,
    /// Current lifecycle state.
    pub status: AgentInvocationStatus,
    /// Prompt or input sent to the other agent, when available.
    pub input: Option<Value>,
    /// Result returned by the other agent, when available.
    pub output: Option<Value>,
    /// Task deliverables returned by the other agent.
    pub artifacts: Vec<TaskArtifact>,
    /// Terminal error message, when available.
    pub error: Option<String>,
}

/// Kind of file-system change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    /// Create a file.
    Create,
    /// Write a file when the source does not reveal whether it was created or replaced.
    Write,
    /// Update an existing file.
    Update,
    /// Delete a file.
    Delete,
    /// Move or rename a file.
    Move,
}

/// One normalized file change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileChange {
    /// Changed path after the operation.
    pub path: PathBuf,
    /// Path before a move or rename.
    pub old_path: Option<PathBuf>,
    /// Kind of change.
    pub kind: FileChangeKind,
    /// Unified diff or provider-native patch text, when available.
    pub diff: Option<String>,
    /// Current lifecycle state.
    pub status: ToolStatus,
}

/// Persisted provider world-state snapshot or patch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    /// Whether this observation replaces the whole state rather than applying
    /// a patch.
    pub full: bool,
    /// Provider-neutral JSON state. Provider-private fields remain namespaced
    /// inside this value.
    pub state: Value,
}

/// Lifecycle state of a durable agent goal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    /// Goal is actively being pursued.
    Active,
    /// Goal is intentionally paused.
    Paused,
    /// Goal cannot progress without an external change.
    Blocked,
    /// Provider usage limits stopped execution.
    UsageLimited,
    /// Configured budget stopped execution.
    BudgetLimited,
    /// Goal completed.
    Complete,
    /// Provider-native state with no normalized equivalent.
    Other(String),
}

/// Durable objective and budget state maintained by an agent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    /// Human-readable objective.
    pub objective: String,
    /// Current lifecycle state.
    pub status: GoalStatus,
    /// Configured token budget.
    pub token_budget: Option<i64>,
    /// Tokens consumed while pursuing the goal.
    pub tokens_used: i64,
    /// Wall time consumed while pursuing the goal.
    pub time_used_seconds: i64,
    /// Goal creation time.
    pub created_at: Option<Timestamp>,
    /// Most recent goal update time.
    pub updated_at: Option<Timestamp>,
}

/// Boundary metadata used when selecting turns inherited by an agent fork.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForkTurnBoundary {
    /// Whether this delivery starts a new logical fork turn.
    pub trigger_turn: bool,
}

/// Provider-neutral operation applied to pending user input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum QueueOperation {
    /// Add input to the queue.
    Enqueue,
    /// Consume the next queued input.
    Dequeue,
    /// Remove one queued input without consuming it.
    Remove,
    /// Consume or clear every queued input.
    PopAll,
    /// A provider-native queue operation not normalized yet.
    Other(String),
}

/// One durable mutation of a coding agent's pending-input queue.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputQueue {
    /// Queue mutation performed by the provider.
    pub operation: QueueOperation,
    /// Queued text or provider-encoded notification.
    pub content: Option<String>,
    /// Background task identity extracted from the queue payload.
    pub task_id: Option<String>,
    /// Tool invocation associated with the background task.
    pub tool_call_id: Option<String>,
    /// Provider-native task category.
    pub task_type: Option<String>,
    /// Provider-native task status.
    pub status: Option<String>,
}

/// A context-window compaction performed during a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextCompaction {
    /// Provider-produced replacement summary, when available.
    pub summary: Option<String>,
    /// Whether compaction was automatically triggered, when reported.
    pub automatic: Option<bool>,
    /// Token count before compaction, when reported.
    pub tokens_before: Option<i64>,
    /// Token count after compaction, when reported.
    pub tokens_after: Option<i64>,
    /// Provider replacement history installed after compaction.
    #[serde(default)]
    pub replacement_history: Option<Vec<Value>>,
    /// Monotonic context-window number.
    #[serde(default)]
    pub window_number: Option<u64>,
    /// First context-window identity in the thread.
    #[serde(default)]
    pub first_window_id: Option<String>,
    /// Immediately previous context-window identity.
    #[serde(default)]
    pub previous_window_id: Option<String>,
    /// Current context-window identity.
    #[serde(default)]
    pub window_id: Option<String>,
}

/// A provider retry observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Retry {
    /// One-based attempt number, when known.
    pub attempt: Option<u32>,
    /// Human-readable retry reason.
    pub reason: Option<String>,
    /// Scheduled retry delay in milliseconds.
    pub delay_ms: Option<u64>,
}

/// A rollback of previously active session context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rollback {
    /// Number of user turns removed from active context, when reported.
    pub turns_removed: Option<u64>,
}

/// An item whose semantics are not yet normalized.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnknownItem {
    /// Best available provider-native type name.
    pub kind: Option<String>,
}

/// Stable item variants ordered inside a session or turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ItemData {
    /// Conversation message.
    Message(Message),
    /// Model reasoning made visible by the provider.
    Reasoning(Reasoning),
    /// Agent plan.
    Plan(Plan),
    /// Tool invocation request or start.
    ToolCall(ToolCall),
    /// Tool invocation result or terminal update.
    ToolResult(ToolResult),
    /// Request for user authorization.
    ApprovalRequest(ApprovalRequest),
    /// Resolved authorization decision.
    ApprovalDecision(ApprovalDecision),
    /// Language-model invocation.
    ModelInvocation(ModelInvocation),
    /// Child or peer agent invocation.
    AgentInvocation(AgentInvocation),
    /// File-system change.
    FileChange(FileChange),
    /// Persisted provider world state.
    WorldState(WorldState),
    /// Durable agent goal and budget state.
    Goal(Goal),
    /// Fork-turn boundary in an inter-agent delivery stream.
    ForkTurnBoundary(ForkTurnBoundary),
    /// Mutation of pending user input.
    InputQueue(InputQueue),
    /// Context-window compaction.
    ContextCompaction(ContextCompaction),
    /// Execution settings in effect for subsequent activity.
    ExecutionContext(ExecutionContext),
    /// Change to the active execution mode.
    ModeChange(ModeChange),
    /// Informational, warning, or error notice emitted during execution.
    Notice(Notice),
    /// Result of one hook or aggregate hook run.
    HookResult(HookResult),
    /// Retry observation.
    Retry(Retry),
    /// Removal of earlier turns from active context.
    Rollback(Rollback),
    /// Provider-native item not yet normalized.
    Unknown(UnknownItem),
}

/// One ordered fact inside a session or turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Item {
    /// Provider-native item identifier, when present.
    pub external_id: Option<String>,
    /// Stable order within the original artifact.
    pub sequence: ItemSequence,
    /// Related earlier item, when the provider exposes a causal parent.
    pub parent: Option<RecordId>,
    /// Original item in another session when this item was inherited by a fork.
    #[serde(default)]
    pub inherited_from: Option<RecordId>,
    /// Actor that caused this item.
    pub actor: Actor,
    /// More specific agent or subagent identifier, when reported.
    pub agent_id: Option<String>,
    /// Typed item data.
    pub data: ItemData,
}

impl Item {
    /// Creates an item with no provider-native identity or relationships.
    pub fn new(sequence: ItemSequence, actor: Actor, data: ItemData) -> Self {
        Self {
            external_id: None,
            sequence,
            parent: None,
            inherited_from: None,
            actor,
            agent_id: None,
            data,
        }
    }
}

/// A record whose semantics are not yet normalized.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnknownRecord {
    /// Best available provider-native type name.
    pub kind: Option<String>,
}

/// Stable normalized record variants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RecordData {
    /// Session metadata.
    Session(Session),
    /// Turn lifecycle state.
    Turn(Turn),
    /// Token usage.
    Usage(Usage),
    /// Provider usage-limit snapshot.
    RateLimit(RateLimit),
    /// One ordered session or turn item.
    Item(Item),
    /// Provider-native data not yet normalized.
    Unknown(UnknownRecord),
}

/// One normalized record emitted by a provider.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Record {
    /// Stable record identifier scoped by [`SourceId`].
    pub id: RecordId,
    /// Concrete local data source that owns the record.
    pub source: SourceId,
    /// Related session record, when this is not itself a session.
    pub session: Option<RecordId>,
    /// Related turn record, when applicable.
    pub turn: Option<RecordId>,
    /// Best normalized timestamp.
    pub timestamp: Option<Timestamp>,
    /// Original artifact location.
    pub origin: SourceRef,
    /// Stable normalized data.
    pub data: RecordData,
    /// Optional provider-native value retained for compatibility.
    pub original: Option<OriginalData>,
}

impl Record {
    /// Creates a record without session, turn, timestamp, or original-data links.
    pub fn new(id: RecordId, source: SourceId, origin: SourceRef, data: RecordData) -> Self {
        Self {
            id,
            source,
            session: None,
            turn: None,
            timestamp: None,
            origin,
            data,
            original: None,
        }
    }
}

/// One incremental change produced by a provider.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Change {
    /// Inserts or replaces one normalized record.
    Upsert(Box<Record>),
    /// Deletes one normalized record.
    Delete(RecordId),
    /// Indicates that records from one artifact must be rebuilt.
    Reset(SourceRef),
    /// Indicates that one artifact disappeared.
    Remove(SourceRef),
}

impl Change {
    /// Creates an upsert without exposing the enum's storage indirection.
    pub fn upsert(record: Record) -> Self {
        Self::Upsert(Box::new(record))
    }
}

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// Informational observation.
    Info,
    /// Recoverable compatibility or data-quality issue.
    Warning,
    /// A provider could not read part of its source.
    Error,
}

/// Structured provider diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Severity.
    pub severity: DiagnosticSeverity,
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable explanation without record payloads.
    pub message: String,
    /// Exact source artifact and position involved, when available.
    pub origin: Option<SourceRef>,
}

impl Diagnostic {
    /// Creates a warning diagnostic.
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: code.into(),
            message: message.into(),
            origin: None,
        }
    }

    /// Attaches the source artifact and position that caused the diagnostic.
    pub fn with_origin(mut self, origin: SourceRef) -> Self {
        self.origin = Some(origin);
        self
    }
}

/// Opaque, serializable incremental provider state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub(crate) provider: ProviderId,
    pub(crate) source: SourceId,
    pub(crate) state: Value,
}

impl Checkpoint {
    /// Creates an opaque checkpoint from provider-private serializable state.
    ///
    /// This is the constructor custom providers use when implementing
    /// [`crate::Provider`]. Consumers should persist the returned checkpoint
    /// without inspecting its private state.
    pub fn from_state(provider: &ProviderInfo, state: &impl Serialize) -> Result<Self> {
        let state = serde_json::to_value(state)
            .map_err(|error| Error::InvalidCheckpoint(error.to_string()))?;
        Ok(Self {
            provider: provider.id.clone(),
            source: provider.source.clone(),
            state,
        })
    }

    /// Returns the provider that created the checkpoint.
    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    /// Returns the concrete source that created the checkpoint.
    pub fn source(&self) -> &SourceId {
        &self.source
    }

    /// Serializes the opaque checkpoint.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|error| Error::InvalidCheckpoint(error.to_string()))
    }

    /// Deserializes an opaque checkpoint.
    pub fn from_json(value: &str) -> Result<Self> {
        serde_json::from_str(value).map_err(|error| Error::InvalidCheckpoint(error.to_string()))
    }

    /// Decodes provider-private state after verifying provider and source
    /// identity.
    ///
    /// Providers own the private state schema. If a crate release changes that
    /// schema incompatibly, callers can discard the checkpoint and perform a
    /// fresh scan.
    pub fn decode_state<T: DeserializeOwned>(&self, provider: &ProviderInfo) -> Result<T> {
        if self.provider != provider.id {
            return Err(Error::InvalidCheckpoint(format!(
                "checkpoint provider {} does not match {}",
                self.provider.as_str(),
                provider.id.as_str()
            )));
        }
        if self.source != provider.source {
            return Err(Error::InvalidCheckpoint(
                "checkpoint belongs to another source".to_owned(),
            ));
        }
        serde_json::from_value(self.state.clone())
            .map_err(|error| Error::InvalidCheckpoint(error.to_string()))
    }
}

/// One bounded incremental provider result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    /// Normalized changes in source order.
    pub changes: Vec<Change>,
    /// Checkpoint to persist after applying this batch.
    pub checkpoint: Checkpoint,
    /// Recoverable diagnostics observed while scanning.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the caller should immediately continue scanning.
    pub has_more: bool,
}

impl Batch {
    /// Creates a batch.
    pub fn new(
        changes: Vec<Change>,
        checkpoint: Checkpoint,
        diagnostics: Vec<Diagnostic>,
        has_more: bool,
    ) -> Self {
        Self {
            changes,
            checkpoint,
            diagnostics,
            has_more,
        }
    }

    /// Verifies source identity and structural invariants for one provider.
    ///
    /// Built-in providers call this before returning a batch. Consumers that
    /// deserialize batches from another process can call it at their trust
    /// boundary as well.
    pub fn validate_for(&self, provider: &ProviderInfo) -> Result<()> {
        if self.checkpoint.provider() != &provider.id {
            return Err(Error::InvalidBatch(format!(
                "checkpoint provider {} does not match {}",
                self.checkpoint.provider().as_str(),
                provider.id.as_str()
            )));
        }
        if self.checkpoint.source() != &provider.source {
            return Err(Error::InvalidBatch(
                "checkpoint belongs to another source".to_owned(),
            ));
        }

        for (index, change) in self.changes.iter().enumerate() {
            match change {
                Change::Upsert(record) => {
                    validate_record(record, provider, index)?;
                }
                Change::Reset(origin) | Change::Remove(origin) => {
                    if origin.source != provider.source {
                        return Err(Error::InvalidBatch(format!(
                            "change {index} artifact belongs to another source"
                        )));
                    }
                }
                Change::Delete(id) => {
                    validate_scoped_id(id, &provider.source, &format!("change {index} delete"))?;
                }
            }
        }

        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if diagnostic
                .origin
                .as_ref()
                .is_some_and(|origin| origin.source != provider.source)
            {
                return Err(Error::InvalidBatch(format!(
                    "diagnostic {index} belongs to another source"
                )));
            }
        }

        Ok(())
    }
}

fn validate_record(record: &Record, provider: &ProviderInfo, index: usize) -> Result<()> {
    validate_scoped_id(
        &record.id,
        &provider.source,
        &format!("change {index} record"),
    )?;
    if record.source != provider.source {
        return Err(Error::InvalidBatch(format!(
            "change {index} record {} belongs to another source",
            record.id.as_str()
        )));
    }
    if record.origin.source != record.source {
        return Err(Error::InvalidBatch(format!(
            "change {index} record {} has a mismatched origin source",
            record.id.as_str()
        )));
    }
    if let Some(session) = record.session.as_ref() {
        validate_scoped_id(
            session,
            &provider.source,
            &format!("change {index} record {} session", record.id.as_str()),
        )?;
    }
    if let Some(turn) = record.turn.as_ref() {
        validate_scoped_id(
            turn,
            &provider.source,
            &format!("change {index} record {} turn", record.id.as_str()),
        )?;
    }
    if record.session.as_ref() == Some(&record.id) || record.turn.as_ref() == Some(&record.id) {
        return Err(Error::InvalidBatch(format!(
            "change {index} record {} links to itself",
            record.id.as_str()
        )));
    }

    match &record.data {
        RecordData::Session(_) if record.session.is_some() || record.turn.is_some() => {
            Err(Error::InvalidBatch(format!(
                "change {index} session record {} has session or turn links",
                record.id.as_str()
            )))
        }
        RecordData::Session(session) => validate_session_links(session, record, provider, index),
        RecordData::Turn(_) if record.turn.is_some() => Err(Error::InvalidBatch(format!(
            "change {index} turn record {} has an unexpected turn link",
            record.id.as_str()
        ))),
        RecordData::Item(item)
            if item.parent.as_ref() == Some(&record.id)
                || item.inherited_from.as_ref() == Some(&record.id) =>
        {
            Err(Error::InvalidBatch(format!(
                "change {index} item record {} has a self-referential item link",
                record.id.as_str()
            )))
        }
        RecordData::Item(item) => validate_item_links(item, record, provider, index),
        _ => Ok(()),
    }
}

fn validate_session_links(
    session: &Session,
    record: &Record,
    provider: &ProviderInfo,
    index: usize,
) -> Result<()> {
    for relation in &session.relations {
        validate_scoped_id(
            &relation.session,
            &provider.source,
            &format!(
                "change {index} session record {} relation",
                record.id.as_str()
            ),
        )?;
    }
    if let Some(history) = session.history.as_ref() {
        if let Some(base) = history.base.as_ref() {
            validate_scoped_id(
                &base.session,
                &provider.source,
                &format!(
                    "change {index} session record {} history base",
                    record.id.as_str()
                ),
            )?;
        }
        for segment in &history.lineage {
            validate_scoped_id(
                &segment.session,
                &provider.source,
                &format!(
                    "change {index} session record {} history lineage",
                    record.id.as_str()
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_item_links(
    item: &Item,
    record: &Record,
    provider: &ProviderInfo,
    index: usize,
) -> Result<()> {
    if let Some(parent) = item.parent.as_ref() {
        validate_scoped_id(
            parent,
            &provider.source,
            &format!("change {index} item record {} parent", record.id.as_str()),
        )?;
    }
    if let Some(inherited_from) = item.inherited_from.as_ref() {
        validate_scoped_id(
            inherited_from,
            &provider.source,
            &format!(
                "change {index} item record {} inherited source",
                record.id.as_str()
            ),
        )?;
    }
    if let ItemData::AgentInvocation(invocation) = &item.data {
        if let Some(child_session) = invocation.child_session.as_ref() {
            validate_scoped_id(
                child_session,
                &provider.source,
                &format!(
                    "change {index} item record {} child session",
                    record.id.as_str()
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_scoped_id(id: &RecordId, source: &SourceId, context: &str) -> Result<()> {
    if id.is_scoped_to(source) {
        Ok(())
    } else {
        Err(Error::InvalidBatch(format!(
            "{context} id {} belongs to another source",
            id.as_str()
        )))
    }
}
