//! Real-time-aware allocator and runtime for REAPER extensions.
//!
//! Inspired by [Helgobox](https://github.com/helgoboss/helgobox) (`allocator_api.rs`,
//! `backbone_shell.rs`, `channels.rs`, `global.rs`, `tracing_util.rs`).
//!
//! # What This Crate Provides
//!
//! 1. **RT-aware global allocator** — detects when deallocation happens on the
//!    REAPER audio thread (via `IsInRealTimeAudio()` C function pointer) and
//!    offloads it to a dedicated background thread via bounded channel.
//!
//! 2. **Allocation guards** — `assert_no_alloc()` / `permit_alloc()` RAII guards
//!    that panic in debug builds when allocations happen in no-alloc zones.
//!
//! 3. **Main-thread task dispatch** — queue closures to run on the REAPER main
//!    thread during the timer callback.
//!
//! 4. **RT-safe channels** — `SenderToRt` (bounded, allocation-free send) and
//!    `ImportantSender` (bounded + emergency unbounded fallback).
//!
//! 5. **Lock-free audio state** — atomic block count, sample rate, block size
//!    for lock-free access from any thread.
//!
//! 6. **Mutex utilities** — poison recovery, non-blocking try-lock helpers.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────┐     bounded channel      ┌─────────────────────┐
//! │  RT Thread   │ ──── dealloc cmds ──────► │  Deallocator Thread │
//! │  (audio)     │                           │  (background)       │
//! └──────────────┘                           └─────────────────────┘
//!
//! ┌──────────────┐     unbounded channel     ┌─────────────────────┐
//! │  Any Thread  │ ──── main-thread tasks ──►│  Main Thread        │
//! │              │                           │  (timer callback)   │
//! └──────────────┘                           └─────────────────────┘
//!
//! ┌──────────────┐     SenderToRt (bounded)  ┌─────────────────────┐
//! │  Main Thread │ ──── RT commands ────────►│  RT Thread          │
//! │              │                           │  (audio hook)       │
//! └──────────────┘                           └─────────────────────┘
//! ```
//!
//! # Setup
//!
//! ```rust,ignore
//! use daw_allocator::{FtsAllocator, FtsRuntime, FtsRuntimeConfig};
//!
//! // Step 1: Install as global allocator (must be at crate root)
//! #[global_allocator]
//! static ALLOCATOR: FtsAllocator = FtsAllocator::new();
//!
//! // Step 2: Initialize the runtime (during extension startup)
//! let runtime = FtsRuntime::init(&ALLOCATOR, FtsRuntimeConfig {
//!     dealloc_channel_capacity: 10_000,
//!     // Pass REAPER's IsInRealTimeAudio function pointer
//!     is_in_rt_audio: reaper.low().pointers().IsInRealTimeAudio.unwrap(),
//! });
//!
//! // Step 3: Process main-thread tasks in timer callback
//! runtime.process_main_thread_tasks();
//! ```
//!
//! # Credits
//!
//! Directly inspired by Helgobox (<https://github.com/helgoboss/helgobox>):
//! - `HelgobossAllocator` — RT-aware GlobalAlloc with async deallocation
//! - `SenderToRealTimeThread` / `ImportantSenderFromRtToNormalThread` — RT channels
//! - `GlobalAudioState` — lock-free atomic audio state
//! - `non_blocking_lock` / poison recovery — mutex utilities
//! - `AsyncWriter` — RT-safe tracing output

mod allocator;
mod audio_state;
mod channels;
mod guards;
mod main_thread;
mod mutex_util;
mod runtime;

pub use allocator::FtsAllocator;
pub use audio_state::AudioState;
pub use channels::{ImportantSender, SenderToRt};
pub use guards::{assert_no_alloc, permit_alloc, undesired_allocation_count};
pub use main_thread::MainThreadDispatcher;
pub use mutex_util::{non_blocking_lock, non_blocking_try_read};
pub use runtime::{FtsRuntime, FtsRuntimeConfig, RtDetector};
