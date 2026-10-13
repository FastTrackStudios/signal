//! RT-aware global allocator.
//!
//! A variation of Helgobox's `HelgobossAllocator`
//! (<https://github.com/helgoboss/helgobox>, `allocator/src/lib.rs`).
//!
//! Key behavior:
//! - Allocations always go to `System` (with debug assertion check)
//! - Deallocations on RT threads are offloaded to a background thread
//! - If the dealloc channel is full, falls back to synchronous
//! - Foreign (C/FFI) values can also be offloaded

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::mpsc::{SyncSender, TrySendError};

use crate::runtime::RtDetector;

/// Commands sent to the async deallocation thread.
#[derive(Debug)]
pub(crate) enum DeallocCommand {
    /// Stop the deallocation thread.
    Stop,
    /// Deallocate a Rust allocation.
    Dealloc { ptr: *mut u8, layout: Layout },
    /// Deallocate a foreign (C) value via a provided function pointer.
    DeallocForeign {
        value: *mut c_void,
        deallocate: unsafe extern "C" fn(value: *mut c_void),
    },
}

// Safety: pointers are only dereferenced on the deallocation thread.
unsafe impl Send for DeallocCommand {}

/// Internal state initialized by `FtsRuntime::init`.
pub(crate) struct AsyncDeallocState {
    pub(crate) sender: SyncSender<DeallocCommand>,
    pub(crate) rt_detector: Box<dyn RtDetector>,
}

/// RT-aware global allocator for REAPER extensions.
///
/// Install as `#[global_allocator]` at the crate root. Call `FtsRuntime::init`
/// to enable RT detection and async deallocation.
///
/// # Credits
///
/// Inspired by Helgobox's `HelgobossAllocator`
/// (<https://github.com/helgoboss/helgobox>).
pub struct FtsAllocator {
    async_state: OnceLock<AsyncDeallocState>,
}

impl FtsAllocator {
    /// Create a new allocator. No RT detection until `FtsRuntime::init`.
    pub const fn new() -> Self {
        Self {
            async_state: OnceLock::new(),
        }
    }

    /// Initialize async deallocation. Called by `FtsRuntime::init`.
    pub(crate) fn init_async(
        &self,
        capacity: usize,
        rt_detector: Box<dyn RtDetector>,
    ) -> std::sync::mpsc::Receiver<DeallocCommand> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(capacity);
        let state = AsyncDeallocState {
            sender,
            rt_detector,
        };
        if self.async_state.set(state).is_err() {
            panic!("FtsAllocator::init_async called more than once");
        }
        receiver
    }

    /// Offload a foreign (C/FFI) deallocation.
    ///
    /// If on an RT thread, sent to the background deallocator thread.
    /// Otherwise executed immediately.
    ///
    /// # Safety
    ///
    /// `value` must be a valid pointer that `deallocate` can free.
    pub unsafe fn dealloc_foreign(
        &self,
        deallocate: unsafe extern "C" fn(value: *mut c_void),
        value: *mut c_void,
    ) {
        self.dealloc_internal(
            || unsafe { deallocate(value) },
            || DeallocCommand::DeallocForeign { value, deallocate },
        );
    }

    /// Send stop signal to the deallocation thread.
    pub fn stop_async_deallocation(&self) {
        if let Some(state) = self.async_state.get() {
            let _ = state.sender.try_send(DeallocCommand::Stop);
        }
    }

    /// Core deallocation: offload if RT, else synchronous.
    fn dealloc_internal(
        &self,
        sync_dealloc: impl FnOnce(),
        make_cmd: impl FnOnce() -> DeallocCommand,
    ) {
        let Some(state) = self.async_state.get() else {
            // Not initialized — synchronous.
            #[cfg(debug_assertions)]
            self.check_violation();
            sync_dealloc();
            return;
        };

        if state.rt_detector.is_rt_thread() {
            // RT thread — offload deallocation.
            match state.sender.try_send(make_cmd()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    // Channel full — synchronous fallback (regrettable but safe).
                    #[cfg(debug_assertions)]
                    self.check_violation();
                    sync_dealloc();
                }
                Err(TrySendError::Disconnected(_)) => {
                    // Shutdown.
                    sync_dealloc();
                }
            }
        } else {
            #[cfg(debug_assertions)]
            self.check_violation();
            sync_dealloc();
        }
    }

    #[cfg(debug_assertions)]
    fn check_violation(&self) {
        if crate::guards::is_allocation_forbidden() {
            crate::guards::record_violation();
        }
    }
}

impl Default for FtsAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for FtsAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(debug_assertions)]
        self.check_violation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc_internal(
            || unsafe { System.dealloc(ptr, layout) },
            || DeallocCommand::Dealloc { ptr, layout },
        );
    }
}
