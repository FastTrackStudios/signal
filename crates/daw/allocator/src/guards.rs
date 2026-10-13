//! Allocation guards for real-time safety.
//!
//! Inspired by Helgobox's `assert_no_alloc` / `permit_alloc` pattern
//! (<https://github.com/helgoboss/helgobox>) and the `assert_no_alloc` crate
//! (<https://github.com/Windfisch/rust-assert-no-alloc>).
//!
//! In debug builds, `assert_no_alloc` sets a thread-local counter that the
//! global allocator checks on every (de)allocation. If an allocation happens
//! while the counter is active (and no `permit_alloc` is in scope), it panics.
//!
//! In release builds, both functions are no-ops with zero overhead.

use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(debug_assertions)]
thread_local! {
    static ALLOC_FORBID_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static ALLOC_PERMIT_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Counter of allocation violations detected across all threads.
static UNDESIRED_ALLOCATION_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Returns the total number of undesired allocations detected since startup.
///
/// Useful for displaying in a status line or diagnostic panel.
pub fn undesired_allocation_count() -> u32 {
    UNDESIRED_ALLOCATION_COUNTER.load(Ordering::Relaxed)
}

/// Increment the violation counter. Called by the allocator when a violation is detected.
#[cfg(debug_assertions)]
pub(crate) fn record_violation() {
    UNDESIRED_ALLOCATION_COUNTER.fetch_add(1, Ordering::Relaxed);
}

/// Check if we're currently in a no-alloc zone without a permit.
///
/// Returns `true` if an allocation right now would be a violation.
#[cfg(debug_assertions)]
pub(crate) fn is_allocation_forbidden() -> bool {
    let forbid = ALLOC_FORBID_COUNT.with(|f| f.get());
    let permit = ALLOC_PERMIT_COUNT.with(|p| p.get());
    forbid > 0 && permit == 0
}

/// Execute `func` in a context where allocations are forbidden (debug only).
///
/// If any allocation or deallocation happens inside `func`, the allocator
/// will panic (debug) or increment the violation counter. Use `permit_alloc`
/// to temporarily lift the restriction for known-safe operations (e.g., logging).
///
/// In release builds, this is a zero-cost passthrough.
///
/// # Example
///
/// ```rust,ignore
/// assert_no_alloc(|| {
///     // Audio processing — no heap operations allowed
///     process_audio_buffer(&mut buffer);
/// });
/// ```
#[cfg(debug_assertions)]
pub fn assert_no_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    struct Guard;
    impl Guard {
        fn new() -> Self {
            ALLOC_FORBID_COUNT.with(|c| c.set(c.get() + 1));
            Self
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            ALLOC_FORBID_COUNT.with(|c| c.set(c.get() - 1));
        }
    }

    let _guard = Guard::new();
    func()
}

#[cfg(not(debug_assertions))]
pub fn assert_no_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    func()
}

/// Temporarily permit allocations inside an `assert_no_alloc` block.
///
/// Use this for known-safe operations like error logging that require
/// allocation but happen inside a larger no-alloc critical section.
///
/// # Example
///
/// ```rust,ignore
/// assert_no_alloc(|| {
///     if error_detected {
///         permit_alloc(|| {
///             tracing::warn!("error in RT context");
///         });
///     }
///     process_samples(&mut buffer);
/// });
/// ```
#[cfg(debug_assertions)]
pub fn permit_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    struct Guard;
    impl Guard {
        fn new() -> Self {
            ALLOC_PERMIT_COUNT.with(|c| c.set(c.get() + 1));
            Self
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            ALLOC_PERMIT_COUNT.with(|c| c.set(c.get() - 1));
        }
    }

    let _guard = Guard::new();
    func()
}

#[cfg(not(debug_assertions))]
pub fn permit_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    func()
}
