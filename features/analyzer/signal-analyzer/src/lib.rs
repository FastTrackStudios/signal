//! Measure FTS FX against reference plugins.
//!
//! The question this crate exists to answer: *when we translate a `FabFilter` or
//! Valhalla patch onto our own DSP, how close did we actually get?* It renders
//! the same stimulus through a reference plugin and through our processor, and
//! reports the difference under metrics chosen to suit the processor.
//!
//! # Architecture position
//!
//! ```text
//!   signal-plugin-host  (hosts the reference: CLAP + VST3, incl. yabridge)
//!   signal-fx           (the candidate: NativeEq / NativeReverb / ...)
//!   signal-import       (translates vendor state -> our parameters)
//!            |
//!            v
//!   signal-analyzer (this crate) -- renders both, measures, reports
//! ```
//!
//! This crate is deliberately **host-agnostic and allocation-tolerant**: it
//! runs offline over captured buffers, never on the audio thread, so it takes
//! no part in the `no_std` / no-allocation rules that govern `features/fx/*-dsp`.
//! It also takes no plugin dependency of its own — callers hand it rendered
//! buffers, which keeps it testable without a plugin installed and usable
//! against any source of audio.
//!
//! # The three metrics
//!
//! They are kept separate rather than blended into one score, because
//! "matching" means different things for different processors:
//!
//! | metric | question | right for |
//! |---|---|---|
//! | [`null`] | is it the *same processing*? | FTS-EQ vs Pro-Q 4 — they share a ZPK design pipeline and should null deeply |
//! | [`decay`] | does it ring like the *same space*? | reverb, where two algorithms never null |
//! | [`loudness`] | does it sit at the same *level and balance*? | any translated preset that has to drop into a mix |
//!
//! [`compare::Thresholds`] bundles them into a profile —
//! [`compare::Thresholds::exact_match`] for the first case,
//! [`compare::Thresholds::reverb_match`] for the second.
//!
//! # Example
//!
//! ```
//! use signal_analyzer::{compare, generators};
//!
//! let sr = 48_000.0;
//! let stimulus = generators::impulse(48_000);
//! // In a real run these come from a hosted plugin and from our own FX.
//! let reference = stimulus.clone();
//! let candidate = stimulus.clone();
//!
//! let result = compare::compare(
//!     &reference, &candidate,
//!     &reference, &candidate,   // impulse responses, for the decay metric
//!     sr,
//!     compare::Thresholds::exact_match(),
//! );
//! assert!(result.passed());
//! ```
//!
//! # A note on honest failure
//!
//! Every metric reports "not measurable" as `None` rather than inventing a
//! number, and every threshold treats `None` as a **failure**. A comparison
//! that measured nothing — silence, a tail too short to fit, no criteria
//! enabled — never reports success. This matters because the whole point is
//! to catch our DSP drifting from the reference, and a metric that passes
//! vacuously is worse than no metric.

pub mod compare;
pub mod decay;
pub mod elements;
pub mod eq_transfer;
pub mod filters;
pub mod generators;
pub mod loudness;
pub mod null;

pub use compare::{Comparison, Criterion, CriterionResult, Thresholds, compare};
pub use decay::{
    DecayComparison, DecayFit, compare_decay, compare_decay_against, reverb_time_best_effort,
};
pub use loudness::{LoudnessComparison, compare_loudness};
pub use null::{NullTest, align_by_latency, null_test};
