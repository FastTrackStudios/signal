//! `reverb` — the built-in Reverb FX for signal.
//!
//! Facade over the DSP core [`reverb_dsp`]; downstream crates depend on `reverb`
//! (never `reverb-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.

pub use reverb_dsp as dsp;
pub use reverb_dsp::*;
