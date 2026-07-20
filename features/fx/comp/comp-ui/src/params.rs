//! nice_plug parameter definitions and shared UI state.
//!
//! Lives in `comp-ui` (not `comp-plugin`) so the [`crate::control_view`]
//! component can render against the param tree without forcing a circular
//! dep — same split as `eq-ui::params`.

use atomic_float::AtomicF32;
use audiocore_core::prelude::*;
use std::sync::Arc;

/// Audio-thread → UI metering data.
pub struct CompUiState {
    pub params: Arc<CompParams>,
    /// Current gain reduction in dB (positive = reducing).
    pub gain_reduction_db: AtomicF32,
    pub input_peak_db: AtomicF32,
    pub output_peak_db: AtomicF32,
    pub sample_rate: AtomicF32,
}

impl CompUiState {
    pub fn new(params: Arc<CompParams>) -> Self {
        Self {
            params,
            gain_reduction_db: AtomicF32::new(0.0),
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            sample_rate: AtomicF32::new(48_000.0),
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
