//! Golden-master and realtime-safety harness for the fx DSP cores.
//!
//! The DSP crates are being restructured into idiomatic Rust — typed sample
//! indices, iterators instead of raw indexing, checked conversions at the
//! boundary — and the whole point of that work is that **the audio does not
//! change**. A rewrite of a reverb tank that sounds different is not a
//! refactor, it is a new reverb, and no amount of reading the diff will tell
//! you which one you produced.
//!
//! So the rewrite is done against evidence, and this crate is the evidence:
//!
//! - [`signal`] generates deterministic excitation — impulse, sweep, fixed-seed
//!   noise, transient bursts, DC, silence.
//! - [`golden`] pins the resulting output as a bit-exact reference vector and
//!   tells you *where* a later run diverged.
//! - [`alloc`] arms a counting allocator around `process()` so "allocation-free
//!   on the hot path" is a test result rather than a comment.
//! - [`num`] is re-exported from `dsp-core`: the audited conversion boundary the
//!   DSP crates themselves are built on, the one place an `as` may happen.
//!
//! Dev-dependency only. Nothing here ships in a plugin.
//!
//! # The workflow
//!
//! 1. Before touching an algorithm, add its fixtures and record them on the
//!    unmodified code (`UPDATE_GOLDEN=1`). Commit the reference files on their
//!    own, so the baseline is a reviewable artifact.
//! 2. Refactor.
//! 3. Run the tests. A reference file that changes is the refactor telling you
//!    it was not one.

pub mod alloc;
pub mod golden;
pub mod signal;

/// The audited conversion boundary, re-exported so a test crate needs one
/// dev-dependency rather than two.
pub use dsp_core::num;

pub use alloc::{AllocReport, CountingAlloc, assert_no_alloc, measure_alloc};
pub use golden::{Golden, Mismatch, Sample};
