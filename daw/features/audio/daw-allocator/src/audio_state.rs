//! Lock-free global audio state.
//!
//! Inspired by Helgobox's `GlobalAudioState`
//! (<https://github.com/helgoboss/helgobox>, `global_audio_state.rs`).
//!
//! All fields are atomics — safe to read from any thread without locking.
//! The RT audio thread calls `advance()` once per audio block to update state.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Lock-free audio state accessible from any thread.
///
/// Updated by the RT audio thread each block. Read by UI, RPC handlers, etc.
pub struct AudioState {
    /// Monotonically increasing block counter (wraps at u64::MAX).
    block_count: AtomicU64,
    /// Current audio block size in samples.
    block_size: AtomicU32,
    /// Current sample rate in Hz (stored as integer, e.g. 44100, 48000).
    sample_rate: AtomicU32,
}

impl AudioState {
    /// Create with default values (zero).
    pub const fn new() -> Self {
        Self {
            block_count: AtomicU64::new(0),
            block_size: AtomicU32::new(0),
            sample_rate: AtomicU32::new(0),
        }
    }

    /// Called by the RT audio thread at the start of each audio block.
    /// Returns the previous block count.
    pub fn advance(&self, block_size: u32, sample_rate: u32) -> u64 {
        let prev = self.block_count.fetch_add(1, Ordering::Relaxed);
        self.block_size.store(block_size, Ordering::Relaxed);
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        prev
    }

    /// Current block count (monotonically increasing).
    pub fn block_count(&self) -> u64 {
        self.block_count.load(Ordering::Relaxed)
    }

    /// Current block size in samples.
    pub fn block_size(&self) -> u32 {
        self.block_size.load(Ordering::Relaxed)
    }

    /// Current sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    /// Audio latency in seconds (block_size / sample_rate).
    pub fn latency_secs(&self) -> f64 {
        let bs = self.block_size() as f64;
        let sr = self.sample_rate() as f64;
        if sr > 0.0 { bs / sr } else { 0.0 }
    }
}

impl Default for AudioState {
    fn default() -> Self {
        Self::new()
    }
}
