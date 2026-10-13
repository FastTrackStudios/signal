//! Audio-thread → UI data feeds.
//!
//! The audio thread may not allocate, block, or take a lock, so everything it
//! hands the editor goes through relaxed atomics. Two shapes cover almost
//! every plugin: a scalar (a meter level) and a history window (a scrolling
//! trace), the latter being [`WaveRing`].

use std::sync::atomic::{AtomicUsize, Ordering};

use audiocore_core::prelude::*;
use fts_audio_ui::prelude::*;

/// Samples kept in a history ring — one per processed block, so ~2.7 s at
/// 512-sample blocks / 48 kHz.
pub const WAVE_HISTORY_LEN: usize = 256;

/// Lock-free single-writer history ring for scrolling traces.
///
/// The audio thread [`push`](WaveRing::push)es one value per block (no
/// allocation, relaxed atomics); the UI thread
/// [`snapshot`](WaveRing::snapshot)s the whole window oldest → newest. A torn
/// read across the head is at worst one stale sample — invisible in a
/// scrolling waveform — so no synchronization beyond the atomics is needed.
pub struct WaveRing {
    buf: [atomic_float::AtomicF32; WAVE_HISTORY_LEN],
    /// Next write slot (monotonically increasing, wrapped on use).
    head: AtomicUsize,
}

impl Default for WaveRing {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveRing {
    pub fn new() -> Self {
        Self {
            buf: std::array::from_fn(|_| atomic_float::AtomicF32::new(0.0)),
            head: AtomicUsize::new(0),
        }
    }

    /// Audio thread: append one value. Lock-free, allocation-free.
    pub fn push(&self, v: f32) {
        let i = self.head.load(Ordering::Relaxed);
        self.buf[i % WAVE_HISTORY_LEN].store(v, Ordering::Relaxed);
        self.head.store(i.wrapping_add(1), Ordering::Relaxed);
    }

    /// UI thread: copy the window out, oldest → newest.
    pub fn snapshot(&self) -> Vec<f32> {
        let head = self.head.load(Ordering::Relaxed);
        (0..WAVE_HISTORY_LEN)
            .map(|k| self.buf[(head.wrapping_add(k)) % WAVE_HISTORY_LEN].load(Ordering::Relaxed))
            .collect()
    }
}

/// A peak meter's shared state: the current level in dB, with decay applied on
/// the audio thread so the UI can render whatever frame rate it likes.
pub struct PeakMeter {
    db: atomic_float::AtomicF32,
}

impl Default for PeakMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl PeakMeter {
    /// Floor used for silence — below any useful meter range.
    pub const FLOOR_DB: f32 = -100.0;
    /// Fall-back per block. At ~90 blocks/s this is a ~27 dB/s release, the
    /// usual "fast peak, readable decay" ballistic.
    pub const DECAY_DB: f32 = 0.3;

    pub fn new() -> Self {
        Self {
            db: atomic_float::AtomicF32::new(Self::FLOOR_DB),
        }
    }

    /// Audio thread: feed one block's linear peak. Rises instantly, decays by
    /// [`DECAY_DB`](Self::DECAY_DB) when the block is quieter.
    pub fn push_peak(&self, linear_peak: f32) {
        let db = if linear_peak > 0.0 {
            20.0 * linear_peak.log10()
        } else {
            Self::FLOOR_DB
        };
        let prev = self.db.load(Ordering::Relaxed);
        self.db.store(
            if db > prev { db } else { prev - Self::DECAY_DB },
            Ordering::Relaxed,
        );
    }

    /// UI thread: current level in dB.
    pub fn db(&self) -> f32 {
        self.db.load(Ordering::Relaxed)
    }
}

/// Render a [`PeakMeter`] pair plus a gain-reduction meter — the metering
/// column almost every dynamics editor wants down its right-hand side.
#[component]
pub fn IoGrMeters(input_db: f32, output_db: f32, gain_reduction_db: f32) -> Element {
    rsx! {
        LevelMeterDb { level_db: input_db, label: "IN".to_string(), height: 160.0 }
        GrMeter { gain_reduction_db, height: 160.0 }
        LevelMeterDb { level_db: output_db, label: "OUT".to_string(), height: 160.0 }
    }
}
