//! Finding drum hits — where each stroke on a kit starts, how hard it
//! was, and how sure we are — from a piece's close mic and its trigger.
//!
//! Spec: `docs/spec/session/drum-editing.md` (`r[hits.detect.*]`).
//!
//! The pieces, in the order a detection runs them:
//!
//! - [`gate`]: the envelope gate (a fast envelope outrunning a slow one,
//!   over a threshold) — Perfect Timing!'s detector, the same settings
//!   meaning the same things. A [`Scan`] walks a signal once and then
//!   answers the gate at any setting on its grid for the price of a walk
//!   over its stretches.
//! - [`trigger`]: a trigger's spikes, and whether the trigger is alive.
//! - [`calibrate`]: the gate's threshold and crest fitted so a mic's
//!   hits agree with its trigger's. Measured on Unbreakable, this is the
//!   difference between F 0.04 and F 0.95 on a tom mic.
//! - [`fuse`]: trigger and mic made one list — the trigger says *that*
//!   a hit happened, the mic says *when* (the attack start, to the
//!   sample) and how hard.
//! - [`bleed`]: a piece with no working trigger, from its mic alone,
//!   with the other close mics' bleed predicted and refused.
//! - [`attack`]: where an attack starts and how loud it gets.
//! - [`kit`]: all of it for a whole kit, in parallel ([`detect_kit`]) —
//!   what a caller runs, on a worker.
//!
//! Everything works in **samples of one contiguous signal** at its own
//! rate. The caller maps a sample to source or project time; nothing
//! here knows what an item is.

pub mod attack;
pub mod bleed;
pub mod calibrate;
pub mod fuse;
pub mod gate;
pub mod kit;
pub mod trigger;

pub use attack::{attack_peak, attack_start};
pub use calibrate::{Agreement, Calibration, agree, calibrate};
pub use fuse::{Hit, fuse, pair_flams};
pub use gate::{Gate, Scan, gate};
pub use kit::{Found, Method, Piece, Settings, Trust, detect_kit};
pub use trigger::{Health, health, spikes};

/// Seconds as a whole number of samples at `rate` (0 for anything
/// negative or not finite).
#[must_use]
pub fn samples(seconds: f64, rate: f64) -> usize {
    num_traits::ToPrimitive::to_usize(&(seconds * rate).round()).unwrap_or(0)
}

/// A sample index as seconds at `rate`.
#[must_use]
pub fn seconds(sample: usize, rate: f64) -> f64 {
    num_traits::ToPrimitive::to_f64(&sample).unwrap_or(0.0) / rate.max(1.0)
}

/// Decibels as a linear gain.
#[must_use]
pub fn gain(db: f64) -> f64 {
    10_f64.powf(db / 20.0)
}

/// A linear level in decibels, floored well under anything audible.
#[must_use]
pub fn db(level: f64) -> f64 {
    20.0 * level.max(1e-9).log10()
}
