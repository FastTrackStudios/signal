//! `tune` — the built-in pitch-correction FX for signal.
//!
//! Facade over the DSP core [`tune_dsp`]; downstream crates depend on `tune`
//! (never `tune-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.
//!
//! `tune` retunes a vocal; its level-focused sibling is the `level` FX. The
//! actual pitch shifting is delegated to the shared `pitch` engine, re-exported
//! as [`tune_dsp::shifter`].

pub use tune_dsp as dsp;
pub use tune_dsp::*;
