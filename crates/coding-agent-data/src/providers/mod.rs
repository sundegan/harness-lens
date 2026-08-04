#[cfg(feature = "claude-code")]
/// Claude Code local transcript support.
pub mod claude_code;
#[cfg(feature = "codex")]
/// Codex local state and rollout support.
pub mod codex;
#[cfg(any(feature = "codex", feature = "claude-code"))]
pub(crate) mod shared;
