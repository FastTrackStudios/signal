//! Test-only allocator that also checks reclamation on the callback.
#![expect(
    unsafe_code,
    reason = "test allocator forwards pointers and layouts unchanged to the system allocator"
)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static FREES: Cell<usize> = const { Cell::new(0) };
}
pub struct Allocator;
static INNER: dsp_golden::CountingAlloc<System> = dsp_golden::CountingAlloc::new(System);
// SAFETY: all allocation operations forward their exact pointer/layout arguments.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { INNER.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { INNER.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        unsafe { INNER.realloc(p, layout, size) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if ARMED.get() {
            FREES.set(FREES.get().saturating_add(1));
        }
        unsafe { INNER.dealloc(p, layout) }
    }
}
pub fn assert_realtime<T>(body: impl FnOnce() -> T) -> T {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            ARMED.set(false);
        }
    }
    FREES.set(0);
    ARMED.set(true);
    let guard = Guard;
    let result = dsp_golden::assert_no_alloc(body);
    drop(guard);
    assert_eq!(FREES.get(), 0, "callback deallocated heap storage");
    result
}
