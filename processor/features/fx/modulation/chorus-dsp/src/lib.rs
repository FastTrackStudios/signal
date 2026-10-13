//! FTS Chorus — multi-engine chorus/flanger/vibrato.
//!
//! Eleven engines (the persisted `engine` index in brackets):
//! - **Cubic** [0]: clean Catmull-Rom interpolation (transparent)
//! - **BBD** [1]: bucket brigade whose bandwidth tracks its clock
//! - **Tape** [2]: wow/flutter/saturation
//! - **Orbit** [3]: dual-tap elliptical orbital modulation (spatial)
//! - **Juno** [4]: triangle LFO + allpass interpolation (Roland Juno-60)
//! - **CE-2** [5]: Boss CE-2 — MN3007, rounded triangle, the warm wet
//! - **Dimension** [6]: Roland SDD-320 — antiphase pair, inverted cross-feed
//! - **Clone** [7]: EHX Small Clone — deep, dark, the swirl
//! - **Tri-Chorus** [8]: three-phase rack chorus, left/centre/right
//! - **SCF** [9]: TC SCF-style pitch-mod chorus — depth is cents
//! - **Julia** [10]: Walrus Julia — Lag, and the dry/chorus/vibrato mix
//!
//! Each engine can operate in Chorus, Flanger, or Vibrato mode.
//!
//! Credits:
//! - Cubic interpolation: standard Catmull-Rom (fts-dsp)
//! - Tape modulation: `ChowDSP` `AnalogTapeModel` (wow/flutter), qdelay (tiagolr)
//! - Orbit modulation: Choroboros (`EsotericShadow`), elliptical 2D LFO
//! - Juno: TAL-NoiseMaker / `YKChorus` (`SpotlightKid`), allpass delay + DC block
//! - BBD decomposition (delay between clock-related filters, soft ceiling):
//!   Raffel & Smith, "Practical modeling of bucket-brigade device circuits",
//!   DAFx-10; Holters & Parker, "A combined model for a bucket brigade
//!   device and its input and output filters", DAFx-18
//! - Interpolation and chorus/flanger/vibrato delay ranges: Dattorro,
//!   "Effect Design Part 2: Delay-line modulation and chorus", JAES 1997
//! - Unit numbers: see the notes in [`classic`]

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]
// ── TEMPORARY: DSP rewrite pending ───────────────────────────────────────
// 62 findings in this crate, held under `expect` rather than fixed one by one.
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
    clippy::indexing_slicing,
    clippy::similar_names,
    reason = "pending the DSP algorithm rewrite; see the note above"
)]

pub mod chain;
pub mod classic;
pub mod dsp;
pub mod engine;

/// Display helpers — what the modulation actually does, sampled from the
/// engines themselves.
///
/// A modulator's picture is its movement over time, and the temptation is to
/// draw a sine and be done. That is a picture of a chorus rather than of
/// *this* chorus: the tape engine's line wanders on wow and flutter that are
/// not locked to the rate at all, the orbit engine traces an ellipse whose
/// projection depends on a second slow rotation, and the Juno's is a triangle
/// through an allpass. None of those are sine waves and all of them are what
/// the listener hears.
///
/// So the shape is taken from a real voice: build one, run it, and read the
/// delay it actually chose each tick.
pub mod analysis {
    use crate::engine::{EffectType, EngineType, Frame};

    /// The sample rate the shape is sampled at.
    ///
    /// Not the host's: a display only needs a couple of hundred points and
    /// running one cycle of a 0.05 Hz LFO at 48 kHz is a million ticks for a
    /// picture. It cannot be arbitrarily low either — the engines size their
    /// delay lines from it, and the Juno reallocates to `buf_len + 4`, so a
    /// pretend rate of a few hundred Hz leaves it with a four-sample buffer
    /// and a delay it cannot read. 4 kHz is comfortably above every engine's
    /// longest delay and cheap enough to run per frame.
    ///
    /// The engines control-rate some targets every 16 samples (4 ms here);
    /// they ramp in between, so the picture is the same line.
    pub const SHAPE_RATE: f64 = 4_000.0;

    /// Longest run the shape will do, whatever the LFO rate. At the slowest
    /// rate on the dial this caps the picture at slightly less than one full
    /// cycle rather than costing a frame.
    const MAX_TICKS: usize = 300_000;

    /// Sample one LFO cycle of a voice's delay time into `out`, in ms.
    ///
    /// The voice is run for real, at [`SHAPE_RATE`], for as many ticks as one
    /// cycle of `rate_hz` takes — then subsampled into `out`. The input is
    /// silence, because the delay a voice reads at does not depend on what is
    /// going through it.
    ///
    /// Engines with their own free-running motion (tape's wow and flutter,
    /// orbit's second rotation) are *not* locked to that cycle and will not
    /// close the loop exactly. That is not an artefact of the sampling — it is
    /// the thing those engines are for, and the panel should show it.
    // One knob per argument; a struct here would just move the list.
    #[expect(clippy::too_many_arguments)]
    pub fn delay_cycle(
        engine: EngineType,
        effect: EffectType,
        rate_hz: f64,
        depth: f64,
        color: f64,
        feedback: f64,
        _phase_offset: f64,
        out: &mut [f64],
    ) {
        let n = out.len().max(2);
        let rate = rate_hz.clamp(1.0e-3, SHAPE_RATE * 0.25);
        let ticks = ((SHAPE_RATE / rate) as usize).clamp(n, MAX_TICKS);
        let mut v = crate::chain::make_engine(engine);
        v.update(SHAPE_RATE);
        v.reset();
        let f = Frame {
            rate_hz: rate,
            depth,
            feedback,
            color,
            width: 1.0,
            effect,
            voices: 1,
        };
        let mut cursor = 0usize;
        for i in 0..ticks {
            v.tick(0.0, 0.0, &f);
            // Subsample: take the point whenever the output index advances.
            let want = i * n / ticks;
            if want == cursor && cursor < n {
                out[cursor] = v.delay_ms();
                cursor += 1;
            }
        }
        // A short run can leave the tail unfilled; hold the last value.
        let last = out.get(cursor.saturating_sub(1)).copied().unwrap_or(0.0);
        for slot in out.iter_mut().skip(cursor) {
            *slot = last;
        }
    }
}
