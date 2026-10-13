//! Main-thread task dispatch.
//!
//! Replaces `TaskSupport` / `Global` / `MainTaskMiddleware` from reaper-high
//! with a simpler, self-contained dispatcher that uses standard channels.
//!
//! Any thread can queue a closure to run on the REAPER main thread. The main
//! thread processes the queue during the timer callback.

use std::sync::mpsc;

/// A boxed closure that runs on the main thread.
type MainThreadTask = Box<dyn FnOnce() + Send + 'static>;

/// Dispatcher for queuing work to the REAPER main thread.
///
/// Clone-friendly (sender is cheap to clone). All clones share the same queue.
#[derive(Clone)]
pub struct MainThreadDispatcher {
    sender: mpsc::Sender<MainThreadTask>,
}

/// Receiver side — held by the main thread to process queued tasks.
///
/// Wrapped in `Mutex` to satisfy `Sync` (required for static storage).
/// Only the main thread calls `process()`, so there's no contention.
pub struct MainThreadReceiver {
    receiver: std::sync::Mutex<mpsc::Receiver<MainThreadTask>>,
}

impl MainThreadDispatcher {
    /// Create a new dispatcher + receiver pair.
    pub fn new() -> (Self, MainThreadReceiver) {
        let (sender, receiver) = mpsc::channel();
        (
            Self { sender },
            MainThreadReceiver {
                receiver: std::sync::Mutex::new(receiver),
            },
        )
    }

    /// Queue a fire-and-forget closure to run on the main thread.
    ///
    /// Returns immediately. The closure runs during the next timer callback.
    pub fn do_on_main_thread(&self, f: impl FnOnce() + Send + 'static) {
        let _ = self.sender.send(Box::new(f));
    }

    /// Queue a closure and block until the result is available.
    ///
    /// The closure runs on the main thread during the next timer callback.
    /// The calling thread blocks until the result is ready.
    ///
    /// **Do not call from the main thread** — this will deadlock.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let tempo = dispatcher.on_main_thread_sync(|| {
    ///     Reaper::get().current_project().tempo().bpm().get()
    /// }).unwrap_or(120.0);
    /// ```
    pub fn on_main_thread_sync<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, mpsc::RecvError> {
        let (tx, rx) = mpsc::channel();
        self.do_on_main_thread(move || {
            let _ = tx.send(f());
        });
        rx.recv()
    }
}

impl MainThreadReceiver {
    /// Process all pending main-thread tasks.
    ///
    /// Call this from the REAPER timer callback. Non-blocking — processes
    /// whatever is in the queue and returns immediately.
    pub fn process(&self) {
        let receiver = self.receiver.lock().expect("main thread receiver poisoned");
        while let Ok(task) = receiver.try_recv() {
            task();
        }
    }
}
