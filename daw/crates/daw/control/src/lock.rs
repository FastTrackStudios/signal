//! Lock-poisoning recovery extension traits.
//!
//! `Mutex::lock().unwrap()` and `RwLock::write().unwrap()` panic when the
//! lock has been poisoned by a prior in-section panic. In a long-running
//! REAPER process that's catastrophic — one transient panic in (say)
//! `fx.rs` cascades into a poisoned cache that crashes every subsequent
//! caller. These extension traits recover the inner data, log a
//! warning, and let callers continue.
//!
//! Use them in service code instead of `.unwrap()` / `.expect()` on
//! lock guards. See [issue #23](https://github.com/FastTrackStudios/daw/issues/23).
//!
//! ```no_run
//! use std::sync::{Mutex, RwLock};
//! use daw_control::lock::{LockExt, RwLockExt};
//!
//! let cache: Mutex<Vec<u32>> = Mutex::new(Vec::new());
//! let mut guard = cache.lock_recoverable("fx::cache");
//! guard.push(42);
//!
//! let bag: RwLock<Vec<u32>> = RwLock::new(Vec::new());
//! let _read = bag.read_recoverable("ext_state.read");
//! let mut write = bag.write_recoverable("ext_state.write");
//! write.push(7);
//! ```
//!
//! ## Why a trait extension rather than free functions
//!
//! Same call shape as `.lock().unwrap()` — drop-in replacement under
//! mechanical sweep. Plus consistent telemetry: every recovery logs a
//! `tracing::warn!` keyed on `ctx` so we can grep for repeated
//! poisonings during debug.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Extension methods on [`Mutex`] for poisoning-tolerant locking.
pub trait LockExt<T> {
    /// Acquire the lock, recovering from poisoning by extracting the
    /// inner state. `ctx` is included in the warning log so callers can
    /// grep for repeated poisonings; pick something stable like
    /// `"fx::cache"`.
    fn lock_recoverable(&self, ctx: &'static str) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_recoverable(&self, ctx: &'static str) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(ctx, "recovering from poisoned mutex");
            poisoned.into_inner()
        })
    }
}

/// Extension methods on [`RwLock`] for poisoning-tolerant locking.
pub trait RwLockExt<T> {
    /// Acquire a read lock, recovering from poisoning.
    fn read_recoverable(&self, ctx: &'static str) -> RwLockReadGuard<'_, T>;
    /// Acquire a write lock, recovering from poisoning.
    fn write_recoverable(&self, ctx: &'static str) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_recoverable(&self, ctx: &'static str) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| {
            tracing::warn!(ctx, "recovering from poisoned RwLock (read)");
            poisoned.into_inner()
        })
    }
    fn write_recoverable(&self, ctx: &'static str) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| {
            tracing::warn!(ctx, "recovering from poisoned RwLock (write)");
            poisoned.into_inner()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::AssertUnwindSafe;
    use std::sync::Arc;

    #[test]
    fn mutex_recovers_after_poisoning() {
        let m: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(vec![1, 2, 3]));
        let m_clone = m.clone();
        let _ = std::thread::spawn(move || {
            let _guard = m_clone.lock().unwrap();
            panic!("poisoning the mutex on purpose");
        })
        .join();
        let guard = m.lock_recoverable("test");
        assert_eq!(*guard, vec![1, 2, 3]);
    }

    #[test]
    fn rwlock_recovers_on_write() {
        let m: Arc<RwLock<u32>> = Arc::new(RwLock::new(7));
        let m_clone = m.clone();
        let _ = std::thread::spawn(move || {
            let mut g = m_clone.write().unwrap();
            *g = 99;
            // Force a real panic that will poison the lock when we drop.
            // `AssertUnwindSafe` here is just to mark the closure for catch_unwind.
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {}));
            panic!("really poisoning now");
        })
        .join();
        let mut guard = m.write_recoverable("test");
        *guard = 42;
        assert_eq!(*guard, 42);
    }

    #[test]
    fn rwlock_recovers_on_read() {
        let m: Arc<RwLock<u32>> = Arc::new(RwLock::new(7));
        let m_clone = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m_clone.write().unwrap();
            panic!("poisoning");
        })
        .join();
        let guard = m.read_recoverable("test");
        assert_eq!(*guard, 7);
    }
}
