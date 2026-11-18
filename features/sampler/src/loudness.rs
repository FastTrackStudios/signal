//! Integrated loudness metering — ITU-R BS.1770-4 / EBU R128.
//!
//! This is the perceptual "how loud does it *sound*" measure (LUFS), the same
//! standard NAM's trainer uses to compute a model's `loudness` metadata. We use
//! it to guarantee every amp model in a library lands on the same average
//! output loudness: run a fixed DI clip through each model (see
//! [`crate::nam_calibrate`]), measure the integrated loudness here, and derive a
//! makeup gain toward a common target. Because the measure is K-weighted (not
//! raw RMS), a scooped clean and a saturated high-gain model that "read" the
//! same here actually *sound* the same — so switching amps never jumps the
//! level.
//!
//! The pipeline is the textbook BS.1770 one:
//!   1. K-weighting: a high-shelf "head" filter + a ~38 Hz high-pass (RLB).
//!   2. 400 ms mean-square blocks with 75 % overlap (100 ms hop).
//!   3. Two-stage gating: absolute −70 LUFS, then a relative −10 LU gate.
//!   4. Integrated loudness = −0.691 + 10·log10(mean gated block power).
//!
//! Mono only — guitar amp models are mono in / mono out, so the channel weight
//! is 1.0 and there is no L/R summation to worry about here.
//!
//! The measurement itself is [`dsp_core::audio::loudness::integrated_lufs`]
//! — one BS.1770 implementation for the whole stack.

pub use dsp_core::audio::loudness::{SILENCE_LUFS, integrated_lufs};
