//! `modulation` — built-in modulation FX for signal (chorus / flanger / vibrato,
//! tremolo, wah).
//!
//! Facade over the DSP cores; downstream depends on `modulation`, never the
//! `*-dsp` crates. UI/profiles are opt-in features, wired when the plugin shell
//! lands.


pub mod chorus;
pub mod trem;
pub mod wah;
#[cfg(feature = "profiles")]
pub mod profiles;
#[cfg(feature = "ui")]
pub mod ui;
