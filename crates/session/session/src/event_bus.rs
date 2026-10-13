//! Reusable event broadcasting abstractions.
//!
//! [`EventBus`] wraps `tokio::sync::broadcast` for multi-consumer event streaming.
//! [`WatchBus`] wraps `tokio::sync::watch` for single-latest-value streaming.

use tokio::sync::watch as tokio_watch;
use tokio::sync::{broadcast, watch};

/// Multi-consumer event bus backed by `tokio::sync::broadcast`.
///
/// Each call to [`subscribe`](EventBus::subscribe) creates an independent receiver
/// that will see all subsequent events emitted via [`emit`](EventBus::emit).
pub struct EventBus<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone> EventBus<T> {
    /// Create a new event bus with the given name and channel capacity.
    #[must_use]
    pub fn new(_name: &str, capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Broadcast a value to all current subscribers.
    ///
    /// If there are no active subscribers the value is silently dropped.
    pub fn emit(&self, value: T) {
        let _ = self.tx.send(value);
    }

    /// Create a new receiver that will observe future events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }
}

/// Single-value streaming bus backed by `tokio::sync::watch`.
///
/// Holds the latest value and lets any number of subscribers observe changes.
pub struct WatchBus<T> {
    tx: watch::Sender<T>,
    rx: watch::Receiver<T>,
}

impl<T: Clone> WatchBus<T> {
    /// Create a new watch bus with the given name and initial value.
    pub fn new(_name: &str, initial: T) -> Self {
        let (tx, rx) = watch::channel(initial);
        Self { tx, rx }
    }

    /// Update the stored value, notifying all subscribers.
    pub fn send(&self, value: T) {
        let _ = self.tx.send(value);
    }

    /// Create a new receiver that will see the current and future values.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<T> {
        self.rx.clone()
    }

    /// Borrow the current value.
    #[must_use]
    pub fn borrow(&self) -> tokio_watch::Ref<'_, T> {
        self.rx.borrow()
    }
}
