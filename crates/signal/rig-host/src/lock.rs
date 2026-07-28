//! Poison-tolerant locking + panic payload text — the two helpers every rig
//! backend had its own copy of.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Poison-tolerant locking: a panic in one service call must never take the
/// rest of the rig down with it (the guarded state is plain data — recovering
/// the inner value is safe, and far better mid-service than footswitches
/// going dead).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_ok_recovers_a_poisoned_mutex() {
        let m = std::sync::Arc::new(Mutex::new(5));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert_eq!(*m.lock_ok(), 5);
    }

    #[test]
    fn panic_message_extracts_str_and_string() {
        let p1: Box<dyn std::any::Any + Send> = Box::new("static text");
        assert_eq!(panic_message(p1.as_ref()), "static text");
        let p2: Box<dyn std::any::Any + Send> = Box::new(String::from("owned"));
        assert_eq!(panic_message(p2.as_ref()), "owned");
        let p3: Box<dyn std::any::Any + Send> = Box::new(42u8);
        assert_eq!(panic_message(p3.as_ref()), "<non-string payload>");
    }
}
