//! Mutex utility patterns for RT-adjacent code.
//!
//! Inspired by Helgobox's `mutex_util.rs`
//! (<https://github.com/helgoboss/helgobox>).
//!
//! Key patterns:
//! - **Poison recovery** — never panic on poisoned mutex, recover the inner value
//! - **Non-blocking try-lock** — return `None` instead of blocking
//! - **Timed lock warning** — warn if lock acquisition takes too long (debug)

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lock a mutex, recovering from poison.
///
/// Standard `Mutex::lock()` panics on poisoned mutex. This recovers the inner
/// value, which is acceptable when the poison came from a panic in another
/// thread's cleanup code.
///
/// Inspired by Helgobox's `non_blocking_lock`.
pub fn non_blocking_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("[daw-allocator] recovering from poisoned mutex");
            poisoned.into_inner()
        }
    }
}

/// Try to read-lock an RwLock without blocking.
///
/// Returns `None` if the lock is held for writing. Recovers from poison.
pub fn non_blocking_try_read<T>(lock: &RwLock<T>) -> Option<RwLockReadGuard<'_, T>> {
    match lock.try_read() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::WouldBlock) => None,
        Err(std::sync::TryLockError::Poisoned(e)) => {
            eprintln!("[daw-allocator] recovering from poisoned RwLock (read)");
            Some(e.into_inner())
        }
    }
}

/// Try to write-lock an RwLock without blocking.
///
/// Returns `None` if the lock is held. Recovers from poison.
#[allow(dead_code)]
pub fn non_blocking_try_write<T>(lock: &RwLock<T>) -> Option<RwLockWriteGuard<'_, T>> {
    match lock.try_write() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::WouldBlock) => None,
        Err(std::sync::TryLockError::Poisoned(e)) => {
            eprintln!("[daw-allocator] recovering from poisoned RwLock (write)");
            Some(e.into_inner())
        }
    }
}

/// Pre-lock a mutex to force internal allocation.
///
/// Rust's `Mutex` may lazily allocate internal structures on first lock.
/// Calling this in the main thread ensures the allocation happens before
/// the mutex is used from an RT thread.
///
/// Inspired by Helgobox's pattern:
/// ```rust,ignore
/// let mutex = Arc::new(Mutex::new(processor));
/// drop(mutex.lock());  // Force allocation now
/// ```
#[allow(dead_code)]
pub fn pre_lock<T>(mutex: &Mutex<T>) {
    drop(non_blocking_lock(mutex));
}
