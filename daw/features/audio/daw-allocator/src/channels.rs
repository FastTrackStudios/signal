//! RT-safe channel wrappers.
//!
//! Inspired by Helgobox's `SenderToRealTimeThread` and
//! `ImportantSenderFromRtToNormalThread`
//! (<https://github.com/helgoboss/helgobox>, `channels.rs`).
//!
//! These provide allocation-safe sending patterns for communicating with
//! and from real-time audio threads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

// ── SenderToRt ──────────────────────────────────────────────────────

/// Bounded sender for sending commands TO the RT thread.
///
/// `try_send` never allocates (bounded channel pre-allocates on creation).
/// If the channel is full, the message is dropped with a warning.
///
/// Inspired by Helgobox's `SenderToRealTimeThread`.
pub struct SenderToRt<T> {
    sender: SyncSender<T>,
    name: &'static str,
    complained: AtomicBool,
}

impl<T> SenderToRt<T> {
    /// Create a bounded channel for sending to the RT thread.
    pub fn new(name: &'static str, capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        (
            Self {
                sender,
                name,
                complained: AtomicBool::new(false),
            },
            receiver,
        )
    }

    /// Try to send a message. Never blocks, never allocates.
    ///
    /// Returns `true` if sent, `false` if channel full (message dropped).
    pub fn send(&self, msg: T) -> bool {
        match self.sender.try_send(msg) {
            Ok(()) => {
                self.complained.store(false, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Full(_)) => {
                if !self.complained.swap(true, Ordering::Relaxed) {
                    eprintln!(
                        "[daw-allocator] channel '{}' full, dropping message",
                        self.name
                    );
                }
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

// ── ImportantSender ─────────────────────────────────────────────────

/// Dual-channel sender: bounded primary + unbounded emergency fallback.
///
/// The primary bounded channel never allocates on send. If it's full,
/// the message is sent on an unbounded emergency channel (which may allocate
/// but guarantees delivery).
///
/// Use this for important messages from RT → main thread where dropping
/// is not acceptable (e.g., MIDI events, state change notifications).
///
/// Inspired by Helgobox's `ImportantSenderFromRtToNormalThread`.
pub struct ImportantSender<T> {
    primary: SyncSender<T>,
    emergency: mpsc::Sender<T>,
    name: &'static str,
}

/// Receiver that drains both the primary and emergency channels.
pub struct ImportantReceiver<T> {
    primary: Receiver<T>,
    emergency: Receiver<T>,
}

impl<T> ImportantSender<T> {
    /// Create a dual-channel sender with bounded primary capacity.
    pub fn new(name: &'static str, primary_capacity: usize) -> (Self, ImportantReceiver<T>) {
        let (primary_tx, primary_rx) = mpsc::sync_channel(primary_capacity);
        let (emergency_tx, emergency_rx) = mpsc::channel();
        (
            Self {
                primary: primary_tx,
                emergency: emergency_tx,
                name,
            },
            ImportantReceiver {
                primary: primary_rx,
                emergency: emergency_rx,
            },
        )
    }

    /// Send a message. Tries bounded first, falls back to unbounded.
    ///
    /// Returns `true` if sent via primary (no allocation),
    /// `false` if sent via emergency (may have allocated).
    pub fn send(&self, msg: T) -> bool {
        match self.primary.try_send(msg) {
            Ok(()) => true,
            Err(TrySendError::Full(msg)) => {
                eprintln!(
                    "[daw-allocator] channel '{}' full, using emergency path",
                    self.name
                );
                let _ = self.emergency.send(msg);
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

impl<T> ImportantReceiver<T> {
    /// Try to receive from either channel (primary first, then emergency).
    pub fn try_recv(&self) -> Option<T> {
        self.primary
            .try_recv()
            .ok()
            .or_else(|| self.emergency.try_recv().ok())
    }

    /// Iterate over all available messages (primary first, then emergency).
    pub fn drain(&self) -> impl Iterator<Item = T> + '_ {
        self.primary.try_iter().chain(self.emergency.try_iter())
    }
}
