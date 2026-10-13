//! Bit depth utilization analyzer.
//!
//! Estimates the effective bit depth actually used in an audio stream by
//! examining the trailing zeros in the 24-bit integer representation of
//! each sample. The more bits that are consistently zero (LSB side), the
//! lower the effective bit depth.

use std::sync::Arc;

use parking_lot::RwLock;

// ── Shared state ──────────────────────────────────────────────────────────────

/// Bit depth analysis state shared with the UI painter.
pub struct BitDepthState {
    /// Estimated effective bit depth (1–24).
    pub bits_used: RwLock<u8>,
}

impl BitDepthState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            bits_used: RwLock::new(24),
        })
    }
}

// ── Bit depth analyzer ────────────────────────────────────────────────────────

/// Effective bit depth analyzer.
///
/// Call [`BitDepthAnalyzer::process`] per audio block from the audio thread.
/// Read the result via the shared [`BitDepthState`] arc.
pub struct BitDepthAnalyzer {
    /// Minimum trailing-zero count observed over the analysis window.
    ///
    /// Fewer trailing zeros = more bits are active = higher effective depth.
    /// We take the *minimum* across a window: any sample that uses a low bit
    /// lifts the effective depth.
    min_trailing_zeros: u32,
    /// Samples processed in the current window.
    window_count: usize,
    /// Window size in samples (~1 s at 48 kHz).
    window_size: usize,

    pub state: Arc<BitDepthState>,
}

impl BitDepthAnalyzer {
    /// Create a new analyzer.
    ///
    /// `sample_rate` is used to set a ~1 s analysis window.
    pub fn new(sample_rate: f32) -> Self {
        let window_size = sample_rate as usize; // ~1 s
        Self {
            min_trailing_zeros: 24,
            window_count: 0,
            window_size,
            state: BitDepthState::new(),
        }
    }

    /// Process a mono block of samples.
    ///
    /// For stereo, call this for each channel (or mix to mono first).
    pub fn process(&mut self, samples: &[f32]) {
        for &s in samples {
            // Convert to 24-bit signed integer representation.
            // f32 full scale maps to ±2^23 = ±8_388_608.
            let as_int = (s * 8_388_607.0).round() as i32;

            // Count trailing zero bits (how many LSBs are 0).
            let trailing = if as_int == 0 {
                // All-zero sample — skip to avoid over-penalizing silence.
                24
            } else {
                as_int.unsigned_abs().trailing_zeros().min(24)
            };

            // Track the minimum: a sample that uses bit N means we have at
            // least N bits of content.
            if trailing < self.min_trailing_zeros {
                self.min_trailing_zeros = trailing;
            }

            self.window_count += 1;
            if self.window_count >= self.window_size {
                self.flush_window();
            }
        }
    }

    fn flush_window(&mut self) {
        // bits_used = 24 − min_trailing_zeros
        let bits = (24u32.saturating_sub(self.min_trailing_zeros)).clamp(1, 24) as u8;
        *self.state.bits_used.write() = bits;

        // Reset for next window
        self.min_trailing_zeros = 24;
        self.window_count = 0;
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        self.min_trailing_zeros = 24;
        self.window_count = 0;
    }
}
