use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;

use crate::providers::shared::file_watch::{
    is_same_or_descendant, subscribe as subscribe_files, FileWatchOptions, WatchRoot,
};
use crate::{Checkpoint, Result, Subscription};

use super::CodexProvider;

/// Codex filesystem monitoring timing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodexWatchOptions {
    /// Coalescing window for related filesystem events.
    pub debounce: Duration,
    /// Maximum interval between full incremental reconciliations.
    pub reconcile_interval: Duration,
}

impl Default for CodexWatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(300),
            reconcile_interval: Duration::from_secs(30),
        }
    }
}

pub(super) fn subscribe(provider: CodexProvider, checkpoint: Checkpoint) -> Result<Subscription> {
    let mut roots = vec![WatchRoot {
        path: provider.source().codex_home().to_path_buf(),
        mode: RecursiveMode::Recursive,
    }];
    if !is_same_or_descendant(
        provider.source().sqlite_home(),
        provider.source().codex_home(),
    ) && provider.source().sqlite_home().is_dir()
    {
        roots.push(WatchRoot {
            path: provider.source().sqlite_home().to_path_buf(),
            mode: RecursiveMode::NonRecursive,
        });
    }
    let options = provider.watch_options;
    subscribe_files(
        provider,
        checkpoint,
        roots,
        FileWatchOptions {
            debounce: options.debounce,
            reconcile_interval: options.reconcile_interval,
        },
        relevant_path,
        "coding-agent-data-codex",
    )
}

fn relevant_path(provider: &CodexProvider, path: &Path) -> bool {
    let state_database = provider.source().state_database();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let state_name = state_database
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state_5.sqlite");
    path == state_database
        || file_name == format!("{state_name}-wal")
        || file_name == format!("{state_name}-shm")
        || is_same_or_descendant(path, &provider.source().active_sessions())
        || is_same_or_descendant(path, &provider.source().archived_sessions())
}
