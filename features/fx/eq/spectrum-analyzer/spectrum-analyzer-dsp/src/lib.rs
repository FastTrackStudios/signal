//! Realtime FFT spectrum-analysis engine for FTS-EQ.
//!
//! Implements `FabFilter` Pro-Q 4-style analyzer behavior: selectable resolution,
//! release speed, spectral tilt, vertical range, freeze, octave smoothing,
//! peak-hold falloff, pre/post overlay, pre↔post collision detection, and
//! cross-instance spectrum sharing.
//!
//! The audio thread only feeds samples through [`AudioFeed`]; all FFT work runs
//! on the UI thread via [`Analyzer::tick`]. See [`analyzer`] for the threading
//! model.
//!
//! Algorithms are reimplemented from a study of `ZLEqualizer` (`AGPLv3`); no code is
//! copied.

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

pub mod accumulator;
pub mod analyzer;
pub mod collision;
pub mod decayer;
pub mod fft;
pub mod ring;
pub mod settings;
pub mod sharing;
pub mod smoother;
pub mod tilter;
pub mod window;

pub use analyzer::{Analyzer, AnalyzerSnapshot, AudioFeed};
pub use settings::{AnalyzerSettings, MagType, Range, Resolution, SpectrumSlot, Speed, StereoType};
pub use sharing::InstanceId;
pub use window::WindowKind;
