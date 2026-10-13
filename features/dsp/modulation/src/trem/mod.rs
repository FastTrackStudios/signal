//! FTS Tremolo — amplitude modulation with MSEG pattern / LFO control.
//!
//! Signal flow: Input → Stereo Split → Amplitude Modulation → Mix → Output.
//!
//! Uses the fts-modulation engine for pattern-driven or tempo-synced
//! amplitude control with stereo phase offset.

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]
// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 11 findings in this crate, held under `expect` rather than fixed one by one.
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
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::items_after_statements,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

pub mod chain;
pub mod dynamics;
pub mod tremolo;

/// Re-export the modulator toolkit so wrappers can drive trigger modes.
pub use fts_modulation;
