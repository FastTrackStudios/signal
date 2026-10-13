//! `level` — the built-in vocal-leveling FX for signal.
//!
//! Facade over the DSP core [`level_dsp`]; downstream crates depend on `level`
//! (never `level-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.
//!
//! `level` rides, gates, de-esses, and de-breathes a vocal; its pitch-focused
//! sibling is the `tune` FX. Both share the ZCR + spectral-centroid + flux block
//! classifier, exposed here as [`level_dsp::Classifier`].

pub use level_dsp as dsp;
pub use level_dsp::*;
