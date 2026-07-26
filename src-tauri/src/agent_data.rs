use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use coding_agent_data::providers::codex::CodexProvider;
use coding_agent_data::{
    AgentDataProvider, ChangeBatch, Checkpoint, DataChange, WatchOptions,
    WatchableAgentDataProvider,
};

use crate::data_paths;

pub struct AgentDataMonitor {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl AgentDataMonitor {
    pub fn start() -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("harness-lens-agent-data".to_owned())
            .spawn(move || run(worker_stop))?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for AgentDataMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(stop: Arc<AtomicBool>) {
    let provider = match CodexProvider::discover() {
        Ok(provider) => provider,
        Err(_) => {
            log::warn!("Codex data monitoring is unavailable");
            return;
        }
    };
    log::info!("starting Codex data monitoring");

    let checkpoint_path = data_paths::agent_data_checkpoint_path();
    let checkpoint = load_checkpoint(&checkpoint_path);
    let mut batch = match provider.scan(checkpoint.as_ref()) {
        Ok(batch) => batch,
        Err(error) if checkpoint.is_some() => {
            log::warn!(
                "the saved Codex checkpoint could not be used ({error}); starting a fresh scan"
            );
            match provider.scan(None) {
                Ok(batch) => batch,
                Err(error) => {
                    log::error!("failed to scan Codex data: {error}");
                    return;
                }
            }
        }
        Err(error) => {
            log::error!("failed to scan Codex data: {error}");
            return;
        }
    };
    log_batch("initial", &batch);
    save_checkpoint(&checkpoint_path, &batch.checkpoint);

    while batch.has_more && !stop.load(Ordering::Acquire) {
        batch = match provider.scan(Some(&batch.checkpoint)) {
            Ok(batch) => batch,
            Err(error) => {
                log::error!("failed to continue the Codex data scan: {error}");
                return;
            }
        };
        log_batch("initial", &batch);
        save_checkpoint(&checkpoint_path, &batch.checkpoint);
    }
    if stop.load(Ordering::Acquire) {
        return;
    }

    let watcher = match provider.watch(batch.checkpoint, WatchOptions::default()) {
        Ok(watcher) => watcher,
        Err(_) => {
            log::error!("failed to watch Codex data");
            return;
        }
    };
    while !stop.load(Ordering::Acquire) {
        match watcher.recv_timeout(Duration::from_millis(250)) {
            Ok(Some(batch)) => {
                log_batch("incremental", &batch);
                save_checkpoint(&checkpoint_path, &batch.checkpoint);
            }
            Ok(None) => {}
            Err(coding_agent_data::Error::WatcherStopped) => {
                if !stop.load(Ordering::Acquire) {
                    log::error!("Codex data monitoring stopped unexpectedly");
                }
                return;
            }
            Err(_) => log::warn!("Codex data monitoring will retry after a transient error"),
        }
    }
}

fn load_checkpoint(path: &Path) -> Option<Checkpoint> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!("failed to read the Codex checkpoint: {error}");
            return None;
        }
    };
    match Checkpoint::from_json(&contents) {
        Ok(checkpoint) => Some(checkpoint),
        Err(error) => {
            log::warn!("the saved Codex checkpoint is invalid and will be rebuilt: {error}");
            None
        }
    }
}

fn save_checkpoint(path: &Path, checkpoint: &Checkpoint) {
    if let Err(error) = write_checkpoint(path, checkpoint) {
        log::warn!("failed to save the Codex checkpoint: {error}");
    }
}

fn write_checkpoint(path: &Path, checkpoint: &Checkpoint) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "checkpoint path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let contents = checkpoint.to_json().map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn log_batch(phase: &str, batch: &ChangeBatch) {
    let mut upserts = 0;
    let mut deletes = 0;
    let mut resets = 0;
    for change in &batch.changes {
        match change {
            DataChange::Upsert { .. } => upserts += 1,
            DataChange::Delete { .. } => deletes += 1,
            DataChange::ResetSource { .. } | DataChange::RemoveSource { .. } => resets += 1,
            _ => {}
        }
    }
    log::info!(
        "Codex data {phase} batch: upserts={upserts}, deletes={deletes}, source_resets={resets}, diagnostics={}, has_more={}",
        batch.diagnostics.len(),
        batch.has_more
    );
    for diagnostic in batch.diagnostics.iter().take(20) {
        log::warn!(
            "Codex data diagnostic [{}]: {}",
            diagnostic.code,
            diagnostic.message
        );
    }
    if batch.diagnostics.len() > 20 {
        log::warn!(
            "{} additional Codex data diagnostics were omitted from the log",
            batch.diagnostics.len() - 20
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{load_checkpoint, write_checkpoint};
    use coding_agent_data::Checkpoint;
    use std::fs;

    #[test]
    fn checkpoint_file_round_trip_and_corruption_recovery() {
        let directory = std::env::temp_dir().join(format!(
            "harness-lens-agent-data-test-{}",
            std::process::id()
        ));
        let path = directory.join("checkpoint.json");
        let checkpoint =
            Checkpoint::from_json(r#"{"provider":"codex","state":{"version":1}}"#).unwrap();

        write_checkpoint(&path, &checkpoint).unwrap();
        assert_eq!(
            load_checkpoint(&path).unwrap().to_json().unwrap(),
            checkpoint.to_json().unwrap()
        );

        fs::write(&path, "{invalid json").unwrap();
        assert!(load_checkpoint(&path).is_none());

        fs::remove_dir_all(directory).unwrap();
    }
}
