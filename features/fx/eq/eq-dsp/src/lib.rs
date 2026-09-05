#![expect(
    rustdoc::broken_intra_doc_links,
    reason = "large transliterated codebase may have incomplete intra-doc links"
)]
// The library has a large amount of code transliterated from a reference
// binary; the following style lints fire heavily on that code without
// actually flagging bugs. They are suppressed via item-level attributes
// rather than at the crate level.

//! Filter design and biquad-cascade DSP for the FTS-EQ plugin.
//!
//! Pipeline:
//!   1. Analog prototype (Butterworth pole/zero geometry).
//!   2. Frequency transformation (LP→BP via elliptic functions, LP→BS, bilinear).
//!   3. Per-section synth (universal-synth helper covers shelves, bell, allpass).
//!   4. Bilinear transform + biquad assembly.
//!
//! Filter types ([`design::FilterType`]):
//!   - `Peak` / Bell, `Highpass`, `Lowpass`, `Bandpass`, `Notch`,
//!     `BandPassVariant`, `FlatTilt`, `LowShelf`, `HighShelf`, `TiltShelf`,
//!     `BandShelf`, `Allpass`, `ShelfAlt`.

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]
// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 1252 findings in this crate, held under `expect` rather than fixed one by one.
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
    clippy::branches_sharing_code,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    clippy::eq_op,
    clippy::float_cmp,
    clippy::get_first,
    clippy::if_not_else,
    clippy::if_same_then_else,
    clippy::imprecise_flops,
    clippy::indexing_slicing,
    clippy::items_after_statements,
    clippy::manual_clamp,
    clippy::manual_midpoint,
    clippy::manual_range_patterns,
    clippy::missing_const_for_fn,
    clippy::needless_range_loop,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::same_functions_in_if_condition,
    clippy::struct_excessive_bools,
    clippy::suboptimal_flops,
    clippy::suspicious_arithmetic_impl,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::unnecessary_fallible_conversions,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq,
    clippy::while_float,
    clippy::wildcard_imports,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

pub mod band;
pub mod biquad;
pub mod calibration;
pub mod cascade;
pub mod chain;
pub mod constants;
pub mod delay;
pub mod design;
pub mod dynamics;
pub mod elliptic;
pub mod engine;
pub mod hardware_eq;
pub mod hardware_targets;
pub mod neve_1073;
pub mod parameters;
pub mod proq4_mzt;
pub mod proq4_peak;
pub mod proq4_per_section_helpers;
pub mod prototype;
pub mod response;
pub mod section;
pub mod shelf;
pub mod shelf_zpk;
pub mod slope;
pub mod spectral;
pub mod transform;
pub mod transient;
pub mod zpk;

pub use band::Band;
pub use chain::EqChain;
pub use design::FilterType;
