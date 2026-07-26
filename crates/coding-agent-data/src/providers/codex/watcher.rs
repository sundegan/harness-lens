use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

use super::CodexProvider;
use crate::watch::is_same_or_descendant;
use crate::{AgentDataProvider, ChangeBatch, Checkpoint, DataWatcher, Error, Result, WatchOptions};

const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(super) fn watch(
    provider: CodexProvider,
    checkpoint: Checkpoint,
    options: WatchOptions,
) -> Result<DataWatcher> {
    let (event_sender, event_receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = event_sender.send(event);
    })?;

    watcher.watch(provider.source().codex_home(), RecursiveMode::Recursive)?;
    if !is_same_or_descendant(
        provider.source().sqlite_home(),
        provider.source().codex_home(),
    ) && provider.source().sqlite_home().is_dir()
    {
        watcher.watch(provider.source().sqlite_home(), RecursiveMode::NonRecursive)?;
    }

    let (batch_sender, batch_receiver) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker_path = provider.source().codex_home().to_path_buf();
    let worker = thread::Builder::new()
        .name("coding-agent-data-codex-watch".to_owned())
        .spawn(move || {
            let mut checkpoint = checkpoint;
            if !scan_until_caught_up(&provider, &mut checkpoint, &batch_sender) {
                return;
            }
            let mut last_scan = Instant::now();

            while !worker_stop.load(Ordering::Acquire) {
                match event_receiver.recv_timeout(WORKER_POLL_INTERVAL) {
                    Ok(Ok(event)) if is_relevant_event(&provider, &event) => {
                        drain_debounce_window(
                            &provider,
                            &event_receiver,
                            options.debounce,
                            &batch_sender,
                            &worker_stop,
                        );
                        if !scan_until_caught_up(&provider, &mut checkpoint, &batch_sender) {
                            break;
                        }
                        last_scan = Instant::now();
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        if batch_sender.send(Err(Error::Watch(error))).is_err() {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if last_scan.elapsed() >= options.reconcile_interval {
                            if !scan_until_caught_up(&provider, &mut checkpoint, &batch_sender) {
                                break;
                            }
                            last_scan = Instant::now();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .map_err(|error| Error::io("start the Codex watcher worker", worker_path, error))?;

    Ok(DataWatcher::new(batch_receiver, watcher, stop, worker))
}

fn drain_debounce_window(
    provider: &CodexProvider,
    receiver: &mpsc::Receiver<notify::Result<Event>>,
    debounce: Duration,
    sender: &mpsc::Sender<Result<ChangeBatch>>,
    stop: &AtomicBool,
) {
    let started = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let Some(remaining) = debounce.checked_sub(started.elapsed()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        let wait = remaining.min(WORKER_POLL_INTERVAL);
        match receiver.recv_timeout(wait) {
            Ok(Ok(event)) if is_relevant_event(provider, &event) => {}
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                let _ = sender.send(Err(Error::Watch(error)));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if wait < remaining => {}
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn scan_until_caught_up(
    provider: &CodexProvider,
    checkpoint: &mut Checkpoint,
    sender: &mpsc::Sender<Result<ChangeBatch>>,
) -> bool {
    loop {
        match provider.scan(Some(checkpoint)) {
            Ok(batch) => {
                let has_more = batch.has_more;
                *checkpoint = batch.checkpoint.clone();
                if (!batch.changes.is_empty() || !batch.diagnostics.is_empty())
                    && sender.send(Ok(batch)).is_err()
                {
                    return false;
                }
                if !has_more {
                    return true;
                }
            }
            Err(error) => return sender.send(Err(error)).is_ok(),
        }
    }
}

fn is_relevant_event(provider: &CodexProvider, event: &Event) -> bool {
    event
        .paths
        .iter()
        .any(|path| is_relevant_path(provider, path))
}

fn is_relevant_path(provider: &CodexProvider, path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let state_database = provider.source().state_database_path();
    let state_database_name = state_database
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state_5.sqlite");
    if path == state_database
        || file_name == format!("{state_database_name}-wal")
        || file_name == format!("{state_database_name}-shm")
    {
        return true;
    }

    is_same_or_descendant(path, &provider.source().codex_home().join("sessions"))
        || is_same_or_descendant(
            path,
            &provider.source().codex_home().join("archived_sessions"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::codex::CodexSource;

    #[test]
    fn debounce_window_handles_the_largest_duration_when_stopping() {
        let provider = CodexProvider::new(CodexSource::from_paths(".", "."));
        let (_event_sender, event_receiver) = mpsc::channel();
        let (batch_sender, _batch_receiver) = mpsc::channel();
        let stop = AtomicBool::new(true);

        drain_debounce_window(
            &provider,
            &event_receiver,
            Duration::MAX,
            &batch_sender,
            &stop,
        );
    }
}
