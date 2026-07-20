//! Drum trigger DSP engine — transient detection and sample triggering.
//!
//! Imported from the legacy FTS-Trigger engine (Plugins/FTS-Trigger,
//! `trigger-dsp`), with its `fts-dsp` dependency mapped to `audiocore-dsp`
//! (the same crate, renamed in the monorepo merge) and its `eq-dsp` bandpass
//! sidechain on the in-tree `eq-dsp`.
//!
//! # Features
//!
//! - Onset detection: time-domain peak/RMS envelope state machine (LSP-style
//!   four-state with confirmation windows), plus six FFT onset detection
//!   functions (spectral flux, SuperFlux, HFC, complex domain, rectified
//!   complex domain, modified KL divergence)
//! - Velocity extraction with configurable curves (linear/log/exp/fixed)
//! - Retrigger prevention with configurable minimum interval
//! - Sidechain HPF/LPF filtering to isolate drum frequency ranges (`eq-dsp`)
//! - Sample playback engine with velocity layers + round-robin
//! - Multi-band detection (Linkwitz-Riley split, implicit kick/snare/tom/
//!   cymbal classification), HPSS bleed rejection, spectral fingerprint
//!   matching, transient shaping preprocessor
//!
//! # Platform note
//!
//! Unlike the sibling time-domain fx cores, this crate is **not** `no_std`:
//! it is an FFT engine (rustfft, heap-allocated spectra) — the same
//! platform-rules exemption as `reverb-dsp`. The time-domain detection path
//! (`detector` in `PeakEnvelope` mode, `chain::TriggerChain::detect_tick`)
//! still allocates nothing per sample; FFT modes allocate only at
//! construction / `update()`, plus rustfft's per-hop planner scratch.
//!
//! # Modules
//!
//! - [`detector`] — Onset / transient detection (state machine + ODFs)
//! - [`spectral_flux`] — FFT onset detection functions
//! - [`velocity`] — Energy-to-velocity mapping
//! - [`sampler`] — Sample playback with round-robin support
//! - [`chain`] — [`TriggerChain`] composable processing chain
//! - [`multiband`] — per-band detection / classification
//! - [`hpss`] — harmonic/percussive separation preprocessor
//! - [`fingerprint`] — spectral fingerprint bleed rejection
//! - [`transient_shape`] — transient-enhancing preprocessor

pub mod chain;
pub mod detector;
pub mod fingerprint;
pub mod hpss;
pub mod multiband;
pub mod sampler;
pub mod spectral_flux;
pub mod transient_shape;
pub mod velocity;

pub use chain::TriggerChain;
pub use detector::{DetectAlgorithm, DetectMode, TriggerDetector};
pub use sampler::{MixMode, Sample, Sampler, VelocityLayer};
pub use velocity::{VelocityCurve, VelocityMapper};

// Re-exported so shells can size buffers / report latency without a direct
// audiocore-dsp dep.
pub use audiocore_dsp::{AudioConfig, Processor};
