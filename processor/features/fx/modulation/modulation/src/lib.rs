//! `modulation` — built-in modulation FX for signal (chorus / flanger / vibrato,
//! tremolo, wah).
//!
//! Facade over the DSP cores; downstream depends on `modulation`, never the
//! `*-dsp` crates. UI/profiles are opt-in features, wired when the plugin shell
//! lands.

pub use chorus_dsp as chorus;
pub use trem_dsp as trem;
pub use wah_dsp as wah;
