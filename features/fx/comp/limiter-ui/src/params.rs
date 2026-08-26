//! nice_plug parameter definitions and shared UI state.
//!
//! Lives in `limiter-ui` (not `limiter-plugin`) so [`crate::control_view`] can
//! render against the param tree without a circular dep — same split as
//! `comp-ui::params` and `eq-ui::params`.

use audiocore_core::prelude::*;
use fts_plug_ui::prelude::{PeakMeter, WaveRing};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

/// Audio-thread → UI metering.
pub struct LimiterUiState {
    pub params: Arc<LimiterParams>,
    /// Current gain reduction in dB (positive = reducing).
    pub gain_reduction_db: atomic_float::AtomicF32,
    pub input: PeakMeter,
    pub output: PeakMeter,
    pub sample_rate: AtomicU32,
    /// Per-block gain reduction (dB, positive) for the scrolling GR trace.
    pub gr_wave: WaveRing,
    /// Per-block output peaks (linear) for the waveform behind the trace.
    pub output_wave: WaveRing,
}

impl LimiterUiState {
    /// How many pushes fill a history ring — handy for tests that want to
    /// flush the whole window to a known value.
    pub const WAVE_LEN_HINT: usize = fts_plug_ui::feed::WAVE_HISTORY_LEN;

    pub fn new(params: Arc<LimiterParams>) -> Self {
        Self {
            params,
            gain_reduction_db: atomic_float::AtomicF32::new(0.0),
            input: PeakMeter::new(),
            output: PeakMeter::new(),
            sample_rate: AtomicU32::new(48_000),
            gr_wave: WaveRing::new(),
            output_wave: WaveRing::new(),
        }
    }
}

/// Brickwall limiter parameters.
///
/// Moved here verbatim from the plugin shell — the ids and their order are
/// unchanged, so existing host state still loads.
#[derive(Params)]
pub struct LimiterParams {
    /// Gain into the limiter — drives the signal against the ceiling.
    #[id = "in_gain"]
    pub input_gain: FloatParam,
    /// Output ceiling; no sample exceeds this level.
    #[id = "ceiling"]
    pub ceiling: FloatParam,
    /// Gain-reduction recovery time (attack is instantaneous / brickwall).
    #[id = "release"]
    pub release_ms: FloatParam,
    /// Ceiling-stage morph: 0 = transparent hard clip (ADClip-style golden
    /// ratio), 1 = full ClipSoftly sine shaping (rounder, adds harmonics).
    #[id = "character"]
    pub character: FloatParam,
    /// True-peak mode: the gain computer detects INTER-SAMPLE peaks
    /// (4x oversampled estimate), so the ceiling holds in dBTP terms —
    /// what streaming loudness normalization actually measures.
    #[id = "true_peak"]
    pub true_peak: BoolParam,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            input_gain: FloatParam::new(
                "Input",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            ceiling: FloatParam::new(
                "Ceiling",
                -0.3,
                FloatRange::Linear {
                    min: -20.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            release_ms: FloatParam::new(
                "Release",
                100.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 500.0,
                    factor: 0.5,
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            character: FloatParam::new("Character", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_unit("%"),
            true_peak: BoolParam::new("True Peak", true),
        }
    }
}
