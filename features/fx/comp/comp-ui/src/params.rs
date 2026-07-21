//! nice_plug parameter definitions and shared UI state.
//!
//! Lives in `comp-ui` (not `comp-plugin`) so the [`crate::control_view`]
//! component can render against the param tree without forcing a circular
//! dep — same split as `eq-ui::params`.

use atomic_float::AtomicF32;
use audiocore_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Samples kept in each waveform history ring (one per processed block —
/// ~2.7 s at 512-sample blocks / 48 kHz).
pub const WAVE_HISTORY_LEN: usize = 256;

/// Lock-free single-writer history ring for the graph's rolling traces.
///
/// The audio thread [`push`](WaveRing::push)es one value per block (no
/// allocation, relaxed atomics); the UI thread
/// [`snapshot`](WaveRing::snapshot)s the whole window oldest → newest. A
/// torn read across the head is at worst one stale sample — invisible in a
/// scrolling waveform — so no synchronization beyond the atomics is needed.
pub struct WaveRing {
    buf: [AtomicF32; WAVE_HISTORY_LEN],
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
            buf: std::array::from_fn(|_| AtomicF32::new(0.0)),
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

/// Audio-thread → UI metering data.
pub struct CompUiState {
    pub params: Arc<CompParams>,
    /// Current gain reduction in dB (positive = reducing).
    pub gain_reduction_db: AtomicF32,
    pub input_peak_db: AtomicF32,
    pub output_peak_db: AtomicF32,
    pub sample_rate: AtomicF32,
    /// Per-block input peaks (linear 0..1) for the graph's waveform fill.
    pub input_wave: WaveRing,
    /// Per-block gain reduction (dB, positive) for the graph's GR overlay.
    pub gr_wave: WaveRing,
}

impl CompUiState {
    pub fn new(params: Arc<CompParams>) -> Self {
        Self {
            params,
            gain_reduction_db: AtomicF32::new(0.0),
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            sample_rate: AtomicF32::new(48_000.0),
            input_wave: WaveRing::new(),
            gr_wave: WaveRing::new(),
        }
    }
}

/// The classic parameter set only — threshold / ratio / attack / release /
/// knee / makeup / mix, plus the chain's stereo-link amount. The engine's
/// extended surface (styles, character/drive, expander, upward comp,
/// sidechain EQ, lookahead, multiband) is deliberately not exposed here.
#[derive(Params)]
pub struct CompParams {
    /// Level above which compression starts.
    #[id = "threshold"]
    pub threshold_db: FloatParam,
    /// Compression ratio (1:1 = off).
    #[id = "ratio"]
    pub ratio: FloatParam,
    /// Attack time.
    #[id = "attack"]
    pub attack_ms: FloatParam,
    /// Release time.
    #[id = "release"]
    pub release_ms: FloatParam,
    /// Soft-knee width around the threshold (0 = hard knee).
    #[id = "knee"]
    pub knee_db: FloatParam,
    /// Makeup (output) gain applied after compression.
    #[id = "makeup"]
    pub makeup_db: FloatParam,
    /// Parallel (dry/wet) mix — the engine's `fold` parameter.
    #[id = "mix"]
    pub mix: FloatParam,
    /// Stereo detector link (1 = fully linked max of both channels).
    #[id = "link"]
    pub stereo_link: FloatParam,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                -20.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            ratio: FloatParam::new(
                "Ratio",
                4.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            attack_ms: FloatParam::new(
                "Attack",
                3.0,
                FloatRange::Skewed {
                    min: 0.005,
                    max: 300.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            release_ms: FloatParam::new(
                "Release",
                100.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            knee_db: FloatParam::new(
                "Knee",
                6.0,
                FloatRange::Linear { min: 0.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            makeup_db: FloatParam::new(
                "Makeup",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            stereo_link: FloatParam::new(
                "Stereo Link",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
        }
    }
}
