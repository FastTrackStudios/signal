//! `comp` — the built-in Comp FX for signal.
//!
//! Facade over the DSP core [`comp_dsp`]; downstream crates depend on `comp`
//! (never `comp-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.

pub use comp_dsp as dsp;
pub use comp_dsp::*;
