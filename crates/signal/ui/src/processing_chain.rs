//! UI-side processing-chain state (gain-reduction meter).
//!
//! This is **not** an audio I/O primitive — it's a small piece of shared UI
//! state that meter components read via Dioxus context. It runs as a
//! pass-through placeholder while the standalone EQ and compressor crates are
//! out of the app's active build; audio I/O itself lives in `daw`.
//!
//! Moved here from the retired `signal-audio` crate (whose real I/O —
//! hardware MIDI input — now lives in `daw-midi-io`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Shared processing-chain state.
///
/// `PartialEq` uses `Arc` pointer equality so Dioxus prop diffing works.
#[derive(Clone)]
pub struct ProcessingChain {
    /// Current gain reduction stored as f32 bits in an `AtomicU32`. Kept for UI
    /// compatibility while dynamics processing is disabled.
    pub gr_db: Arc<AtomicU32>,
}

impl PartialEq for ProcessingChain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.gr_db, &other.gr_db)
    }
}

impl ProcessingChain {
    /// Create a new chain. The sample rate is accepted for API compatibility.
    pub fn new(_sample_rate: f64) -> Self {
        Self {
            gr_db: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Read the current gain reduction in dB (positive = reducing).
    pub fn gain_reduction_db(&self) -> f32 {
        f32::from_bits(self.gr_db.load(Ordering::Relaxed))
    }

    /// Process a stereo buffer (pass-through while dynamics are disabled).
    pub fn process(&self, _left: &mut [f64], _right: &mut [f64]) {
        self.gr_db.store(0.0f32.to_bits(), Ordering::Relaxed);
    }
}
