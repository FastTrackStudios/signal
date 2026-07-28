//! Dynamic-EQ engine: shared detector, auto-threshold histogram, and
//! the per-band dynamic filter.
//!
//! Design (see `spec/eq-suite-plan.md`):
//! - The dynamic model is a **base/target gain crossfade**:
//!   `gain_db(t) = base + d(t)·(target − base)` with the detector output
//!   `d(t) ∈ [0, 1]`. "Dynamic range" (Pro-Q's ring) = `target − base`,
//!   bipolar, so compression and expansion are symmetric.
//! - Dynamic bands run on a Simper state-variable filter, where gain
//!   enters the tick algebraically — per-sample gain modulation is
//!   stable and allocation-free (the reason ZL Equalizer recommends its
//!   SVF structure for dynamics). Static bands keep the MZT cascades.
//! - Clean-room from published math only: Simper's SVF derivations,
//!   RBJ cookbook side filters, and standard level-detection ballistics.

pub mod detector;
pub mod dyn_band;
pub mod histogram;
pub mod svf;
pub mod taper;

pub use detector::{Detector, DetectorParams};
pub use dyn_band::{DynBand, DynBandParams, DynShape, SideMode};
pub use histogram::LoudnessHistogram;
pub use svf::{Svf, SvfShape};
pub use taper::LogMidTaper;
