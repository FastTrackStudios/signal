//! `gate` — the drum-gate FX.
//!
//! Facade over the DSP core [`gate_dsp`]; downstream crates depend on `gate`
//! (never `gate-dsp` directly). The engine is [`DrumGate`]: four source
//! modes (kick, snare top/bottom, toms) that fix the look-ahead, detector
//! key filter and de-bleed crossover; threshold, reduction, a shaped
//! release of a given length, a Ghost switch for quieter hits, and the HF
//! de-bleed expander. See `gate_dsp`'s crate docs for the measured model.

#![no_std]

pub use gate_dsp as dsp;
pub use gate_dsp::*;
