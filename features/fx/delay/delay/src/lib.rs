//! `delay` — the built-in Delay FX for signal.
//!
//! Facade over the DSP core [`delay_dsp`]; downstream crates depend on `delay`
//! (never `delay-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.

pub use delay_dsp as dsp;
pub use delay_dsp::*;
