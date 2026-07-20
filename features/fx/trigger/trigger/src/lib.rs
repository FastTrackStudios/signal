//! `trigger` — the built-in drum-trigger FX for signal.
//!
//! Facade over the DSP core [`trigger_dsp`]; downstream crates depend on
//! `trigger` (never `trigger-dsp` directly). The plugin UI and preset
//! profiles are opt-in (`features = ["ui"]` / `["profiles"]`) and wire in
//! once the Dioxus editor lands — the DSP is always available.
//!
//! `trigger` turns a drum mic into MIDI: a fast envelope follower + onset
//! detector with a dynamics margin (ring-out never retriggers), a
//! retrigger-guard window, and peak-dB → velocity mapping. Hits come back as
//! `(sample_offset_in_block, velocity)` so the plugin shell can emit
//! sample-accurate `NoteOn`s.

pub use trigger_dsp as dsp;
pub use trigger_dsp::*;
