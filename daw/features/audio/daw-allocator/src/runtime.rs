//! DAW runtime — unified entry point for allocator, task dispatch, and audio state.
//!
//! Inspired by Helgobox's `BackboneShell` initialization sequence and
//! `Global` singleton (<https://github.com/helgoboss/helgobox>).

use std::alloc::GlobalAlloc;
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};

use crate::allocator::{DeallocCommand, FtsAllocator};
use crate::audio_state::AudioState;
use crate::main_thread::{MainThreadDispatcher, MainThreadReceiver};

/// Configuration for the DAW runtime.
pub struct FtsRuntimeConfig {
    /// Capacity of the async deallocation channel.
    /// When full, deallocation falls back to synchronous.
    /// Helgobox uses 10,000. Default: 10,000.
    pub dealloc_channel_capacity: usize,

    /// RT thread detector. Typically wraps REAPER's `IsInRealTimeAudio()`.
    pub rt_detector: Box<dyn RtDetector>,
}

/// Trait for detecting whether the current thread is a real-time audio thread.
///
/// The canonical implementation wraps REAPER's `IsInRealTimeAudio()` C function
/// pointer — zero overhead, always accurate, no thread registration needed.
///
/// # Example: REAPER integration
///
/// ```rust,ignore
/// use daw_allocator::RtDetector;
///
/// struct ReaperRtDetector {
///     is_in_rt_audio: unsafe extern "C" fn() -> i32,
/// }
///
/// impl RtDetector for ReaperRtDetector {
///     fn is_rt_thread(&self) -> bool {
///         unsafe { (self.is_in_rt_audio)() != 0 }
///     }
/// }
///
/// // Get from REAPER's function pointers:
/// let detect = ReaperRtDetector {
///     is_in_rt_audio: reaper.low().pointers().IsInRealTimeAudio.unwrap(),
/// };
/// ```
pub trait RtDetector: Send + Sync {
    /// Returns `true` if the current thread is a real-time audio thread.
    fn is_rt_thread(&self) -> bool;
}

/// Global runtime instance.
static RUNTIME: OnceLock<FtsRuntime> = OnceLock::new();

/// Global audio state (lock-free atomics).
pub static AUDIO_STATE: AudioState = AudioState::new();

/// The unified DAW runtime.
///
/// Provides main-thread dispatch, coordinates with the global allocator,
/// and exposes the lock-free audio state.
pub struct FtsRuntime {
    dispatcher: MainThreadDispatcher,
    receiver: MainThreadReceiver,
    /// Handle to the deallocation thread (kept alive for process lifetime).
    #[allow(dead_code)]
    dealloc_thread: Option<JoinHandle<()>>,
}

impl FtsRuntime {
    /// Initialize the runtime. Call once during extension startup.
    ///
    /// 1. Creates the main-thread dispatcher
    /// 2. Initializes the global allocator's async deallocation
    /// 3. Spawns the background deallocation thread ("daw-deallocator")
    pub fn init(allocator: &FtsAllocator, config: FtsRuntimeConfig) -> &'static FtsRuntime {
        RUNTIME.get_or_init(|| {
            let (dispatcher, receiver) = MainThreadDispatcher::new();

            // Initialize the allocator's async deallocation with the RT detector
            let dealloc_rx =
                allocator.init_async(config.dealloc_channel_capacity, config.rt_detector);

            // Spawn the deallocation thread (Helgobox: "Helgobox deallocator")
            let dealloc_thread = thread::Builder::new()
                .name("daw-deallocator".to_string())
                .spawn(move || {
                    while let Ok(cmd) = dealloc_rx.recv() {
                        match cmd {
                            DeallocCommand::Stop => break,
                            DeallocCommand::Dealloc { ptr, layout } => {
                                unsafe { std::alloc::System.dealloc(ptr, layout) };
                            }
                            DeallocCommand::DeallocForeign { value, deallocate } => {
                                unsafe { deallocate(value) };
                            }
                        }
                    }
                })
                .expect("failed to spawn daw-deallocator thread");

            FtsRuntime {
                dispatcher,
                receiver,
                dealloc_thread: Some(dealloc_thread),
            }
        })
    }

    /// Get the runtime instance. Panics if not initialized.
    pub fn get() -> &'static FtsRuntime {
        RUNTIME
            .get()
            .expect("FtsRuntime not initialized — call FtsRuntime::init() first")
    }

    /// Get the runtime instance if initialized, or `None`.
    pub fn try_get() -> Option<&'static FtsRuntime> {
        RUNTIME.get()
    }

    /// Get the main-thread dispatcher. Clone it to send from any thread.
    pub fn dispatcher(&self) -> &MainThreadDispatcher {
        &self.dispatcher
    }

    /// Process pending main-thread tasks. Call from the timer callback.
    pub fn process_main_thread_tasks(&self) {
        self.receiver.process();
    }

    /// Access the global lock-free audio state.
    pub fn audio_state(&self) -> &'static AudioState {
        &AUDIO_STATE
    }
}
