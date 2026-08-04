use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;

use crate::providers::shared::file_watch::{
    is_same_or_descendant, subscribe as subscribe_files, FileWatchOptions, WatchRoot,
};
use crate::{Checkpoint, Result, Subscription};

use super::ClaudeCodeProvider;

/// Claude Code filesystem monitoring timing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaudeCodeWatchOptions {
    /// Coalescing window for related filesystem events.
    pub debounce: Duration,
    /// Maximum interval between full incremental reconciliations.
    pub reconcile_interval: Duration,
}

impl Default for ClaudeCodeWatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(300),
            reconcile_interval: Duration::from_secs(30),
        }
    }
}

pub(super) fn subscribe(
    provider: ClaudeCodeProvider,
    checkpoint: Checkpoint,
) -> Result<Subscription> {
    let options = provider.watch_options;
    let root = provider.source().config_dir().to_path_buf();
    subscribe_files(
        provider,
        checkpoint,
        vec![WatchRoot {
            path: root,
            mode: RecursiveMode::Recursive,
        }],
        FileWatchOptions {
            debounce: options.debounce,
            reconcile_interval: options.reconcile_interval,
        },
        relevant_path,
        "coding-agent-data-claude-code",
    )
}

fn relevant_path(provider: &ClaudeCodeProvider, path: &Path) -> bool {
    is_same_or_descendant(path, &provider.source().projects_dir())
}
