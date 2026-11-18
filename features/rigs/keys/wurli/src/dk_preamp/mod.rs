//! DK preamp — feature-toggled between legacy (default) and melange solvers.
//!
//! Default: hand-written 8-node MNA solver. Fast, fully validated, matches
//! the melange solver within ±0.18 dB on gain across all R_ldr and within
//! ±0.15 dB per-1/3-octave on harmonic content (see v0.5.2 A/B). Same
//! tremolo range (6.10 dB), same THD (0.79%), same H2/H3 character to four
//! decimal places. Ships as default for 1.6-7× CPU win.
//!
//! `--features melange-preamp`: melange-generated 12-node M=3 solver with
//! Sherman-Morrison pot correction and shadow-pump cancellation. Retained
//! for full-precision DC-pump characterization studies; not normally needed
//! at runtime.

#[cfg(not(feature = "melange-preamp"))]
pub use crate::dk_preamp_legacy::DkPreamp;

#[cfg(feature = "melange-preamp")]
mod melange_adapter;
#[cfg(feature = "melange-preamp")]
pub use melange_adapter::DkPreamp;
