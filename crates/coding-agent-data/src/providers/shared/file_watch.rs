use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

use crate::{Batch, Checkpoint, Error, Provider, Result, Subscription};

const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) struct FileWatchOptions {
    pub debounce: Duration,
    pub reconcile_interval: Duration,
}

pub(crate) struct WatchRoot {
    pub path: PathBuf,
    pub mode: RecursiveMode,
}

pub(crate) fn subscribe<P, F>(
    provider: P,
    checkpoint: Checkpoint,
    roots: Vec<WatchRoot>,
    options: FileWatchOptions,
    relevant: F,
    worker_name: &'static str,
) -> Result<Subscription>
where
    P: Provider + Send + 'static,
    F: Fn(&P, &Path) -> bool + Send + 'static,
{
    let (event_sender, event_receiver) = mpsc::sync_channel(256);
    let mut watcher = notify::recommended_watcher(move |event| {
        // Reconciliation guarantees eventual consistency, so dropping a
        // duplicate event is safer than allowing an event storm to grow an
        // unbounded queue.
        let _ = event_sender.try_send(event);
    })
    .map_err(subscription_error)?;
    for root in &roots {
        watcher
            .watch(&root.path, root.mode)
            .map_err(subscription_error)?;
    }

    let (batch_sender, batch_receiver) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker_path = roots
        .first()
        .map(|root| root.path.clone())
        .unwrap_or_default();
    let worker = thread::Builder::new()
        .name(worker_name.to_owned())
        .spawn(move || {
            let _watcher = watcher;
            let mut checkpoint = checkpoint;
            if !scan_until_caught_up(&provider, &mut checkpoint, &batch_sender, &worker_stop) {
                return;
            }
            let mut last_scan = Instant::now();

            while !worker_stop.load(Ordering::Acquire) {
                match event_receiver.recv_timeout(WORKER_POLL_INTERVAL) {
                    Ok(Ok(event)) if is_relevant_event(&provider, &event, &relevant) => {
                        drain_debounce_window(
                            &provider,
                            &event_receiver,
                            &options,
                            &relevant,
                            &batch_sender,
                            &worker_stop,
                        );
                        if !scan_until_caught_up(
                            &provider,
                            &mut checkpoint,
                            &batch_sender,
                            &worker_stop,
                        ) {
                            break;
                        }
                        last_scan = Instant::now();
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        if !send_result(&batch_sender, Err(subscription_error(error)), &worker_stop)
                        {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if last_scan.elapsed() >= options.reconcile_interval {
                            if !scan_until_caught_up(
                                &provider,
                                &mut checkpoint,
                                &batch_sender,
                                &worker_stop,
                            ) {
                                break;
                            }
                            last_scan = Instant::now();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .map_err(|error| Error::io("start a provider subscription", worker_path, error))?;

    Ok(Subscription::new(batch_receiver, move || {
        stop.store(true, Ordering::Release);
        let _ = worker.join();
    }))
}

fn is_relevant_event<P, F>(provider: &P, event: &Event, relevant: &F) -> bool
where
    F: Fn(&P, &Path) -> bool,
{
    event.paths.iter().any(|path| relevant(provider, path))
}

fn drain_debounce_window<P, F>(
    provider: &P,
    receiver: &mpsc::Receiver<notify::Result<Event>>,
    options: &FileWatchOptions,
    relevant: &F,
    sender: &mpsc::SyncSender<Result<Batch>>,
    stop: &AtomicBool,
) where
    F: Fn(&P, &Path) -> bool,
{
    let started = Instant::now();
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let Some(remaining) = options.debounce.checked_sub(started.elapsed()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        let wait = remaining.min(WORKER_POLL_INTERVAL);
        match receiver.recv_timeout(wait) {
            Ok(Ok(event)) if is_relevant_event(provider, &event, relevant) => {}
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                let _ = send_result(sender, Err(subscription_error(error)), stop);
            }
            Err(mpsc::RecvTimeoutError::Timeout) if wait < remaining => {}
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn scan_until_caught_up(
    provider: &impl Provider,
    checkpoint: &mut Checkpoint,
    sender: &mpsc::SyncSender<Result<Batch>>,
    stop: &AtomicBool,
) -> bool {
    let mut sent_incomplete_batch = false;
    loop {
        match provider.scan(Some(checkpoint)) {
            Ok(batch) => {
                let has_more = batch.has_more;
                let next_checkpoint = batch.checkpoint.clone();
                let has_content = !batch.changes.is_empty() || !batch.diagnostics.is_empty();
                if has_content || (!has_more && sent_incomplete_batch) {
                    if !send_result(sender, Ok(batch), stop) {
                        return false;
                    }
                    sent_incomplete_batch = has_more;
                }
                *checkpoint = next_checkpoint;
                if !has_more {
                    return true;
                }
            }
            Err(error) => return send_result(sender, Err(error), stop),
        }
    }
}

fn send_result(
    sender: &mpsc::SyncSender<Result<Batch>>,
    mut result: Result<Batch>,
    stop: &AtomicBool,
) -> bool {
    loop {
        match sender.try_send(result) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Full(pending)) => {
                if stop.load(Ordering::Acquire) {
                    return false;
                }
                result = pending;
                thread::sleep(Duration::from_millis(25));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn subscription_error(error: notify::Error) -> Error {
    Error::Subscription(error.to_string())
}

pub(crate) fn is_same_or_descendant(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use crate::{
        Batch, Change, Checkpoint, Error, Provider, ProviderId, ProviderInfo, RecordId, Result,
        SourceId,
    };

    use super::{scan_until_caught_up, send_result};

    struct BatchProvider {
        info: ProviderInfo,
        batches: Mutex<VecDeque<Batch>>,
    }

    impl Provider for BatchProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        fn scan(&self, _checkpoint: Option<&Checkpoint>) -> Result<Batch> {
            self.batches
                .lock()
                .expect("batch queue should be available")
                .pop_front()
                .ok_or_else(|| Error::InvalidBatch("unexpected extra scan".to_owned()))
        }
    }

    fn provider_info() -> ProviderInfo {
        ProviderInfo {
            id: ProviderId::new("test"),
            name: "Test",
            source: SourceId::new("test-source"),
        }
    }

    fn batch(info: &ProviderInfo, changes: Vec<Change>, has_more: bool) -> Batch {
        Batch::new(
            changes,
            Checkpoint::from_state(info, &serde_json::json!({})).unwrap(),
            Vec::new(),
            has_more,
        )
    }

    #[test]
    fn backpressure_wait_can_be_cancelled() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        sender
            .send(Err(Error::Subscription("first".to_owned())))
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            send_result(
                &sender,
                Err(Error::Subscription("second".to_owned())),
                &worker_stop,
            )
        });

        thread::sleep(Duration::from_millis(50));
        stop.store(true, Ordering::Release);

        assert!(!worker.join().unwrap());
    }

    #[test]
    fn caught_up_empty_batch_closes_an_emitted_incomplete_sequence() {
        let info = provider_info();
        let first = batch(
            &info,
            vec![Change::Delete(RecordId::scoped(
                &info.source,
                "record",
                "one",
            ))],
            true,
        );
        let terminal = batch(&info, Vec::new(), false);
        let provider = BatchProvider {
            info: info.clone(),
            batches: Mutex::new(VecDeque::from([first, terminal.clone()])),
        };
        let mut checkpoint = Checkpoint::from_state(&info, &serde_json::json!({})).unwrap();
        let (sender, receiver) = mpsc::sync_channel(2);

        assert!(scan_until_caught_up(
            &provider,
            &mut checkpoint,
            &sender,
            &AtomicBool::new(false),
        ));
        assert!(receiver.recv().unwrap().unwrap().has_more);
        assert_eq!(receiver.recv().unwrap().unwrap(), terminal);
    }

    #[test]
    fn idle_empty_reconciliation_is_not_emitted() {
        let info = provider_info();
        let provider = BatchProvider {
            info: info.clone(),
            batches: Mutex::new(VecDeque::from([batch(&info, Vec::new(), false)])),
        };
        let mut checkpoint = Checkpoint::from_state(&info, &serde_json::json!({})).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);

        assert!(scan_until_caught_up(
            &provider,
            &mut checkpoint,
            &sender,
            &AtomicBool::new(false),
        ));
        assert!(receiver.try_recv().is_err());
    }
}
