use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use notify::RecommendedWatcher;
#[cfg(feature = "codex")]
use std::path::Path;

use crate::{ChangeBatch, Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WatchOptions {
    pub debounce: Duration,
    pub reconcile_interval: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(300),
            reconcile_interval: Duration::from_secs(30),
        }
    }
}

/// Owns the native filesystem watcher and receives normalized change batches.
pub struct DataWatcher {
    receiver: Receiver<Result<ChangeBatch>>,
    watcher: Option<RecommendedWatcher>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DataWatcher {
    #[cfg(feature = "codex")]
    pub(crate) fn new(
        receiver: Receiver<Result<ChangeBatch>>,
        watcher: RecommendedWatcher,
        stop: Arc<AtomicBool>,
        worker: JoinHandle<()>,
    ) -> Self {
        Self {
            receiver,
            watcher: Some(watcher),
            stop,
            worker: Some(worker),
        }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<ChangeBatch>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(batch) => batch.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::WatcherStopped),
        }
    }
}

impl Drop for DataWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.watcher.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(feature = "codex")]
pub(crate) fn is_same_or_descendant(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}
