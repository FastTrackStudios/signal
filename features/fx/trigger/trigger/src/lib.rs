//! `trigger` — the built-in drum-trigger FX for signal.
//!
//! Facade over the DSP core [`trigger_dsp`]; downstream crates depend on
//! `trigger` (never `trigger-dsp` directly). Preset profiles are opt-in
//! (`features = ["profiles"]`); the plugin UI feature is declared and wires
//! in once the Dioxus editor lands — the DSP is always available.
//!
//! The engine is the legacy FTS-Trigger chain: sidechain HPF/LPF (`eq-dsp`)
//! → onset detector (time-domain peak/RMS state machine or one of six FFT
//! onset detection functions — spectral flux, SuperFlux, HFC, complex
//! domain, rectified complex domain, modified KL) → velocity curve
//! (linear/log/exp/fixed) → sample playback (velocity layers + round-robin)
//! → mix (replace/layer/blend). MIDI-out shells drive
//! [`TriggerChain::detect_tick`](chain::TriggerChain::detect_tick) per
//! sample for sample-accurate `NoteOn`s and leave the sampler idle.

pub use trigger_dsp as dsp;
pub use trigger_dsp::*;
