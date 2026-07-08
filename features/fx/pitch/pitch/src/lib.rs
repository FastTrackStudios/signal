//! `pitch` — the built-in Pitch FX for signal.
//!
//! Facade over the DSP core [`pitch_dsp`]; downstream crates depend on `pitch`
//! (never `pitch-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.

pub use pitch_dsp as dsp;
pub use pitch_dsp::*;
