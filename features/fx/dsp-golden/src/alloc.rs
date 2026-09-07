//! The allocation guard.
//!
//! `clippy::disallowed_methods` catches a `Mutex::lock` written in plain sight.
//! It cannot see a `Vec::push` three call frames down inside a helper that
//! looked pure, or a `collect()` hidden behind an iterator chain, or the
//! reallocation a `format!` in a debug branch performs on the audio callback
//! once every few thousand blocks. Those are exactly the ones that ship.
//!
//! So the linter guards the source and this guards the behaviour: a counting
//! global allocator, armed only around the code under test, that reports how
//! many allocations actually happened while a `process()` ran.
//!
//! Install it once per test binary:
//!
//! ```ignore
//! dsp_golden::install_counting_allocator!();
//!
//! #[test]
//! fn processing_allocates_nothing() {
//!     let mut engine = Engine::new(48_000.0);
//!     let mut block = vec![0.0; 128];
//!     engine.process(&mut block);            // warm up: first block may size buffers
//!     dsp_golden::assert_no_alloc(|| engine.process(&mut block));
//! }
//! ```
//!
//! Note the warm-up. A DSP node is allowed to allocate in `new()` and
//! `reset()` — the rule is that the *steady state* allocates nothing — so the
//! guard is armed after the node has reached it. A node that needs more than
//! one warm-up block to stop allocating has a latent sizing bug worth finding.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
}

/// A global allocator that counts allocations made while the guard is armed.
///
/// Wraps any other allocator; use [`crate::install_counting_allocator!`] to
/// install it over the system allocator.
#[derive(Debug, Clone, Copy)]
pub struct CountingAlloc<A> {
    inner: A,
}

impl<A> CountingAlloc<A> {
    /// Wrap `inner`.
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

fn record(size: usize) {
    if ARMED.get() {
        COUNT.set(COUNT.get().saturating_add(1));
        BYTES.set(
            BYTES
                .get()
                .saturating_add(u64::try_from(size).unwrap_or(u64::MAX)),
        );
    }
}

// SAFETY: every method forwards to the wrapped allocator unchanged; the only
// added work is incrementing thread-local `Cell` counters, which allocate
// nothing and register no destructor (both cells are `Copy` with const init),
// so no re-entrancy into the allocator is possible.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAlloc<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { self.inner.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.inner.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { self.inner.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { self.inner.alloc_zeroed(layout) }
    }
}

/// What the guard observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocReport {
    /// Number of allocation or reallocation calls.
    pub allocations: u64,
    /// Bytes requested across them.
    pub bytes: u64,
}

impl AllocReport {
    /// Whether the guarded code was allocation-free.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.allocations == 0
    }
}

impl core::fmt::Display for AllocReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} allocation(s), {} bytes",
            self.allocations, self.bytes
        )
    }
}

/// Run `body` with the guard armed and report what it allocated.
///
/// Nesting is not supported and not needed: arm around the narrowest piece of
/// realtime code you can, which is the block-processing call itself.
pub fn measure_alloc<T>(body: impl FnOnce() -> T) -> (T, AllocReport) {
    COUNT.set(0);
    BYTES.set(0);
    ARMED.set(true);
    let value = body();
    ARMED.set(false);
    (
        value,
        AllocReport {
            allocations: COUNT.get(),
            bytes: BYTES.get(),
        },
    )
}

/// Run `body` and panic if it allocated.
///
/// # Panics
///
/// When the guarded code performs any allocation or reallocation.
pub fn assert_no_alloc<T>(body: impl FnOnce() -> T) -> T {
    let (value, report) = measure_alloc(body);
    assert!(
        report.is_clean(),
        "realtime path allocated: {report}.\n  \
         An allocation on an audio callback is an unbounded wait for the system allocator's \
         lock — it does not show up as a wrong sample, it shows up as a click under load.\n  \
         Pre-allocate the buffer in `new()`/`reset()` and reuse it; if the size depends on a \
         parameter, size for the maximum at construction and track the live length separately."
    );
    value
}

/// Install [`CountingAlloc`] over the system allocator for this test binary.
#[macro_export]
macro_rules! install_counting_allocator {
    () => {
        #[global_allocator]
        static DSP_GOLDEN_ALLOC: $crate::CountingAlloc<::std::alloc::System> =
            $crate::CountingAlloc::new(::std::alloc::System);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // The counter is only meaningful when this allocator is actually installed,
    // which the crate's own test binary does here.
    crate::install_counting_allocator!();

    #[test]
    fn arithmetic_on_a_preallocated_buffer_is_clean() {
        let mut buf = [0.0_f32; 512];
        let ((), report) = measure_alloc(|| {
            for (n, slot) in buf.iter_mut().enumerate() {
                *slot = dsp_core::num::count_to_f32(n) * 0.5;
            }
        });
        assert!(report.is_clean(), "{report}");
    }

    #[test]
    fn growing_a_vector_is_caught() {
        let (_, report) = measure_alloc(|| {
            let mut v: Vec<f32> = Vec::new();
            for n in 0..64 {
                v.push(dsp_core::num::count_to_f32(n));
            }
            v
        });
        assert!(!report.is_clean(), "a growing Vec must be detected");
        assert!(report.bytes > 0);
    }

    #[test]
    fn the_guard_disarms_itself() {
        let ((), first) = measure_alloc(|| ());
        assert!(first.is_clean());
        let mut escapee: Vec<f32> = Vec::new();
        escapee.extend((0..64).map(dsp_core::num::count_to_f32));
        assert!(escapee.iter().sum::<f32>() > 0.0);
        let ((), second) = measure_alloc(|| ());
        assert!(
            second.is_clean(),
            "work outside the guard must not be counted: {second}"
        );
    }
}
