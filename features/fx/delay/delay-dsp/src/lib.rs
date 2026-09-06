//! Delay DSP engine — time-based effects for the plugin suite.
//!
//! Based on techniques from qdelay (tiagolr) and `ChowDSP`'s `AnalogTapeModel`.
//!
//! - [`tape_delay::TapeDelay`] — Tape echo with wow/flutter, feedback filtering,
//!   saturation, ducking, and diffusion
//! - [`pitch_delay::PitchDelay`] — Per-repeat pitch shifting with granular crossfade
//! - [`engine::DelayEngine`] — Unified wrapper over all delay styles
//! - [`chain::DelayChain`] — Full stereo delay with ping-pong, swing, and mix

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]
// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 991 findings in this crate, held under `expect` rather than fixed one by one.
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
    clippy::float_cmp,
    clippy::imprecise_flops,
    clippy::indexing_slicing,
    clippy::items_after_statements,
    clippy::many_single_char_names,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_ref_mut,
    clippy::option_if_let_else,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::suboptimal_flops,
    clippy::suspicious_operation_groupings,
    clippy::too_many_lines,
    clippy::while_float,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

pub mod bbd_core;
pub mod bbd_delay;
pub mod chain;
pub mod clean_delay;
pub mod drum_delay;
pub mod dual;
pub mod engine;
pub mod filter_delay;
pub mod lofi_delay;
pub mod modulation;
pub mod multitap_delay;
pub mod oilcan_delay;
pub mod pitch_delay;
pub mod reverb_delay;
pub mod reverse_delay;
pub mod rhythm_delay;
pub mod shimmer_delay;
pub mod spectral_delay;
pub mod tape_delay;
pub mod tilt;

pub use bbd_delay::BbdVoice;
pub use chain::{DelayChain, HeadMode, TapDivision};
pub use drum_delay::{DrumHead, DrumSpacing, GOLDEN_HEADS, HeadPlayback, SILVER_HEADS};
pub use dual::{DualDelay, DualRouting};
pub use engine::{DelayEngine, DelayStyle};
pub use filter_delay::{FilterLfoShape, FilterLocation};
pub use lofi_delay::LoFiFilterShape;
pub use modulation::WobbleShape;
pub use multitap_delay::{FeedbackMode, MAX_TAPS, Tap, TapFilter, TapGrid, TapPreset};
pub use oilcan_delay::OilCanHeads;
pub use pitch_delay::{IceInterval, IceSlice};
pub use spectral_delay::{DensityMode, GrainDirection, GrainShape};
pub use tape_delay::{SaturationType, TapeSpeed, TapeVoice};

/// Equal-power pan gains — see [`audiocore_dsp::stereo::pan_equal_power`]
/// (centered = unity both sides; hard pans full-power one side).
#[inline]
pub(crate) fn pan_gains(pan: f64) -> (f64, f64) {
    audiocore_dsp::stereo::pan_equal_power(pan)
}
