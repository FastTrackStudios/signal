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
