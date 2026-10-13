//! Model-specific compressor controls and reusable DSP components.
//!
//! [`CompressorConfig`] prepares a [`Model`] for bounded stereo processing.
//! [`components`] exposes replaceable detector, gain-law, envelope and coloration
//! stages. [`la2a`] composes the measured Gray gain law and [`opto::OptoCell`].
//!
//! ```
//! use comp_dsp::{CompressorConfig, La2aControls, Model, ProcessSpec};
//! # fn main() -> Result<(), comp_dsp::Error> {
//! let prepared = CompressorConfig::new(Model::La2aGray(La2aControls {
//!     peak_reduction: 0.67, gain: 0.286,
//! })).prepare(ProcessSpec::new(48_000.0, 512)?)?;
//! let mut processor = prepared.processor();
//! processor.process_stereo(&mut [0.0; 512], &mut [0.0; 512])?;
//! # Ok(())
//! # }
//! ```

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

pub mod components;
pub mod model;
pub use model::{
    CompressorConfig, CompressorProcessor, Error, GenericControls, La2aControls, Model,
    PreparedCompressor, ProcessSpec,
};

pub mod biquad;
pub mod chain;
pub mod detector;
pub mod gain_curve;
pub mod hermite;
pub mod la2a;
pub mod multiband;
pub mod opto;
pub mod smoother;
pub mod styles;

pub use biquad::{Biquad, design_highpass_biquad, design_lowpass_biquad};
pub use chain::CompChain;
pub use detector::Detector;
pub use gain_curve::GainCurve;
pub use hermite::{HermiteCubicSmoother, StateFuncHypothesis};
pub use multiband::{CompressionBand, MultiBandCompressor};
pub use smoother::GainReductionSmoother;
pub use styles::{CompressionStyle, StyleCoefficients};

pub mod pro_c;
pub use pro_c::{CHANNELS, ProC3Compressor};
