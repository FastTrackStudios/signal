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
//!   functions (spectral flux, `SuperFlux`, HFC, complex domain, rectified
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

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// findings in this crate, held under `expect` rather than fixed one by one.
//
// These are the judgment lints — casts, indexing and integer arithmetic in
// per-sample math. The correct rewrite for each depends on whether the code
// runs on an audio callback, so editing them individually would be thousands
// of unreviewable changes to code with no characterization tests behind it.
// The plan is to restructure these algorithms into idiomatic Rust (typed
// sample indices, iterators over raw indexing, checked conversions at the
// boundary) against a golden-master harness that proves the output is
// unchanged — which removes whole classes of these at once instead of
// suppressing them.
//
// This is `allow`, not `expect`, and that is a deliberate compromise: `lib`
// and `lib test` are separate compilations, so a lint can fire in one and be
// unfulfilled in the other, and no single crate-root `expect` list satisfies
// both — it oscillates. The cost is that this block does NOT delete itself
// when the rewrite lands; it has to be removed by hand, and it will silently
// keep hiding new violations until then. Shrink it as crates are rewritten.
//
// The realtime guard and every panic lint stay DENIED here — deliberately not
// in this list. `unwrap`, `expect`, `panic`, and the disallowed-methods
// realtime guard still fail the build in this crate.
#![allow(
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::option_if_let_else,
    clippy::similar_names,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

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
