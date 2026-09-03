//! Reverb DSP engine — comprehensive reverb processor with 12 algorithm types.
//!
//! # Architecture
//!
//! Signal flow: Input → Input HP/LP → Pre-Delay → Algorithm → Output EQ → Width → Mix
//!
//! # Algorithms
//!
//! **Phase 1 — Classic:**
//! - [`algorithms::room`] — FDN + early reflections
//! - [`algorithms::hall`] — Large FDN with modulated allpass diffusion
//! - [`algorithms::plate`] — Dattorro tank topology (1997 paper)
//! - [`algorithms::spring`] — Waveguide + allpass dispersion
//!
//! **Phase 2 — Extended:**
//! - [`algorithms::cloud`] — CloudSeed-style ambient (multitap → diffuser → parallel delays)
//! - [`algorithms::bloom`] — Multi-diffusion feeding FDN tank
//! - [`algorithms::shimmer`] — Pitch-shifted feedback reverb
//! - [`algorithms::chorale`] — Formant-filtered pitch-shifted reverb
//! - [`algorithms::magneto`] — Multi-head tape delay + progressive diffusion
//! - [`algorithms::nonlinear`] — Shaped envelopes (reverse, gate, swoosh, ramp)
//! - [`algorithms::swell`] — Envelope-controlled reverb buildup
//! - [`algorithms::reflections`] — Geometric early reflections

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 2791 findings in this crate, held under `expect` rather than fixed one by one.
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
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::indexing_slicing,
    clippy::items_after_statements,
    clippy::manual_slice_fill,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::string_slice,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

pub mod algorithm;
pub mod algorithms;
pub mod chain;
pub mod dual;
pub mod ir;
pub mod primitives;

pub use algorithm::{
    AlgorithmParams, AlgorithmType, BloomParams, ChamberColor, ChamberParams, ChoirVoice,
    ChoraleParams, ChoraleResonance, ChoraleVowel, CloudParams, ConvolutionModParams, HallParams,
    ImpulseDirection, ImpulseParams, ImpulseTail, IrSlot, MagnetoHeads, MagnetoParams,
    MagnetoSpacing, NlShape, NonLinearParams, ReverbAlgorithm, ReverbVoice, ShimmerFeedbackMode,
    ShimmerParams, SpringDwell, SpringParams, SwellType,
};
pub use chain::{InfiniteMode, ReverbChain};
pub use dual::{DualReverb, DualRouting};
