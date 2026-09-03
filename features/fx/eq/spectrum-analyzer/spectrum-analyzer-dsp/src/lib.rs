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

// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 97 findings in this crate, held under `expect` rather than fixed one by one.
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
    clippy::assigning_clones,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::manual_slice_fill,
    clippy::missing_const_for_fn,
    clippy::non_std_lazy_statics,
    clippy::struct_excessive_bools,
    clippy::suboptimal_flops,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

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
