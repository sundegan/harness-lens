use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crate::{Batch, Error, Result};

/// Receives normalized provider changes until cancelled or dropped.
///
/// The public handle is independent of the provider's monitoring mechanism.
/// A provider may use filesystem notifications, polling, or another private
/// transport.
pub struct Subscription {
    receiver: Receiver<Result<Batch>>,
    cancel: Option<Box<dyn FnOnce() + Send>>,
}

impl Subscription {
    /// Creates a subscription from a batch receiver and cancellation callback.
    ///
    /// Provider adapters can use this constructor with filesystem
    /// notifications, polling workers, or another private monitoring
    /// mechanism.
    pub fn new(receiver: Receiver<Result<Batch>>, cancel: impl FnOnce() + Send + 'static) -> Self {
        Self {
            receiver,
            cancel: Some(Box::new(cancel)),
        }
    }

    /// Waits for the next batch until `timeout`.
    ///
    /// Returns `Ok(None)` when the timeout expires without changes.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<Batch>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(batch) => batch.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::SubscriptionClosed),
        }
    }

    /// Cancels monitoring and waits for provider-owned resources to stop.
    pub fn cancel(mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}
