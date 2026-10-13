//! Spectrum analyzer feature facade.
//!
//! Single import root for the `spectrum-analyzer` feature. Enable cargo features
//! to pull in the slices you need:
//!
//! - `dsp` (default): the realtime analysis engine — re-exported as [`dsp`].
//! - `ui`: vello painters + Dioxus settings panel — re-exported as [`ui`].

#[cfg(feature = "dsp")]
pub use spectrum_analyzer_dsp as dsp;

#[cfg(feature = "ui")]
pub use spectrum_analyzer_ui as ui;

// Convenience re-exports of the most-used engine types at the crate root.
#[cfg(feature = "dsp")]
pub use spectrum_analyzer_dsp::{
    Analyzer, AnalyzerSettings, AnalyzerSnapshot, AudioFeed, InstanceId, Range, Resolution, Speed,
};
