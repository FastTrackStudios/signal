//! `eq` — the built-in Eq FX for signal.
//!
//! Facade over the DSP core [`eq_dsp`]; downstream crates depend on `eq`
//! (never `eq-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.

pub use eq_dsp as dsp;
pub use eq_dsp::*;
