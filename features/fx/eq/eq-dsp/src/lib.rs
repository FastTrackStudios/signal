//! Typed equalizer configuration, validated filter designs and real-time DSP.
//!
//! Build an [`EqConfig`], validate it with [`EqConfig::prepare`], and give the
//! immutable [`PreparedEq`] to an [`EqProcessor`]. Prepared updates are borrowed
//! at a block boundary; their owner controls reclamation off the audio thread.
//! For a single filter, use [`Filter::prepare`] and [`FilterProcessor`].
//!
//! ```
//! use eq_dsp::{BandConfig, CutSlope, EqConfig, EqProcessor, ProcessSpec};
//! # fn main() -> Result<(), eq_dsp::Error> {
//! let mut config = EqConfig::new();
//! config.add_band(BandConfig::bell(3_000.0, 2.5, 0.8))?;
//! config.add_band(BandConfig::high_pass(80.0, CutSlope::DbPerOctave(24.0)))?;
//! let prepared = config.prepare(ProcessSpec::new(48_000.0, 512)?)?;
//! let mut processor = EqProcessor::new(&prepared)?;
//! let mut left = [0.0; 512];
//! let mut right = [0.0; 512];
//! processor.process_stereo(&mut left, &mut right)?;
//! # Ok(())
//! # }
//! ```
//!
//! [`host`] contains plugin/preset parameter encoding adapters.
//! [`hardware`] exposes model-specific controls and validated cascade export.

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

// ── The layers, outermost first ──────────────────────────────────────────
//
// The split that matters is `design` against `runtime`: design is arithmetic
// that happens when a parameter changes and may be as expensive as it likes,
// runtime is arithmetic that happens per sample and may not allocate, lock or
// branch unpredictably. Everything else follows from that one line.

/// Engine implementation; applications use typed configuration.
#[doc(hidden)]
pub mod engine;

/// Expert coefficient design. Supported bounded orders use inline scratch.
pub mod design;
/// Level detection and the dynamic/spectral/transient bands built on it.
pub mod dynamics;
/// Processing primitives. Prefer `FilterProcessor` and `EqProcessor`.
#[doc(hidden)]
pub mod runtime;

/// Fixed response curves modelled from named analogue units.
pub mod hardware;
/// Textbook filter mathematics, with no Pro-Q in it.
pub mod math;

pub use design::FilterType;

mod error;
pub(crate) mod filter;
pub use error::Error;
pub use filter::{BiquadCoefficients, FilterProcessor, PreparedFilter};

mod config;
pub(crate) mod inline;
pub use config::{
    Ballistics, BandConfig, BandId, Character, CutSlope, DetectorSource, Dynamics, DynamicsMode,
    EqConfig, Filter, Listen, Output, Pan, ProcessSpec, Steepness, Stream, Threshold, Transient,
};
pub use runtime::band::Placement;
pub(crate) mod prepared;
pub use prepared::{EqProcessor, PreparedEq, StereoResponse};

pub mod host;
pub mod model;
pub mod pultec_color;

pub use math::zpk::Complex;
