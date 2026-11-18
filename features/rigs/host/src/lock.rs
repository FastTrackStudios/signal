//! Poison-tolerant locking + panic payload text — the two helpers every rig
//! backend had its own copy of.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Poison-tolerant locking: a panic in one service call must never take the
/// rest of the rig down with it.
///
/// The guarded state is plain data — recovering the inner value is safe, and far
/// better mid-service than footswitches going dead.
pub trait LockExt<T> {
    fn lock_ok(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_ok(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Best-effort text from a caught panic payload.
pub fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string payload>")
}
