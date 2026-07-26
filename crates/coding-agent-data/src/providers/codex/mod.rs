mod discovery;
mod scanner;
#[cfg(feature = "watch")]
mod watcher;

use std::path::{Path, PathBuf};

use crate::{
    AgentDataProvider, ChangeBatch, Checkpoint, ProviderCapability, ProviderDescriptor, ProviderId,
    Result,
};
#[cfg(feature = "watch")]
use crate::{DataWatcher, WatchOptions, WatchableAgentDataProvider};

pub const PROVIDER_ID: &str = "codex";

const CAPABILITIES: &[ProviderCapability] = &[
    ProviderCapability::Discover,
    ProviderCapability::Snapshot,
    ProviderCapability::Incremental,
    #[cfg(feature = "watch")]
    ProviderCapability::Watch,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSource {
    codex_home: PathBuf,
    sqlite_home: PathBuf,
}

impl CodexSource {
    pub fn discover() -> Result<Self> {
        discovery::discover()
    }

    pub fn from_paths(codex_home: impl Into<PathBuf>, sqlite_home: impl Into<PathBuf>) -> Self {
        Self {
            codex_home: normalize_source_path(codex_home.into()),
            sqlite_home: normalize_source_path(sqlite_home.into()),
        }
    }

    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn sqlite_home(&self) -> &Path {
        &self.sqlite_home
    }

    pub fn state_database_path(&self) -> PathBuf {
        self.sqlite_home.join("state_5.sqlite")
    }
}

fn normalize_source_path(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|current| current.join(&path))
            .unwrap_or(path)
    };
    absolute.canonicalize().unwrap_or(absolute)
}

#[derive(Clone, Debug)]
pub struct CodexProvider {
    source: CodexSource,
    limits: scanner::ScanLimits,
}

impl CodexProvider {
    pub fn discover() -> Result<Self> {
        Ok(Self::new(CodexSource::discover()?))
    }

    pub fn new(source: CodexSource) -> Self {
        Self {
            source,
            limits: scanner::ScanLimits::default(),
        }
    }

    pub fn source(&self) -> &CodexSource {
        &self.source
    }
}

impl AgentDataProvider for CodexProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: ProviderId::new(PROVIDER_ID),
            name: "Codex",
            capabilities: CAPABILITIES,
        }
    }

    fn scan(&self, checkpoint: Option<&Checkpoint>) -> Result<ChangeBatch> {
        scanner::scan(&self.source, &self.limits, checkpoint)
    }
}

#[cfg(feature = "watch")]
impl WatchableAgentDataProvider for CodexProvider {
    fn watch(&self, checkpoint: Checkpoint, options: WatchOptions) -> Result<DataWatcher> {
        watcher::watch(self.clone(), checkpoint, options)
    }
}

#[cfg(test)]
mod tests {
    use super::CodexSource;

    #[test]
    fn explicit_source_paths_are_normalized_to_absolute_paths() {
        let source = CodexSource::from_paths(".", ".");

        assert!(source.codex_home().is_absolute());
        assert!(source.sqlite_home().is_absolute());
    }
}
