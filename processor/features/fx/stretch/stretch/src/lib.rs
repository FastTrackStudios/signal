//! Tempo and pitch, independently: what a host runs per playing voice.
//!
//! The facade over [`stretch_dsp`] — hosts depend on this, never on the
//! engine crate. A voice reads its [`Source`] wherever the time ratio puts
//! it, so a DAW item stretches (tempo without pitch), transposes (pitch
//! without tempo) or both, and lands sample-exact after a
//! [`Stretcher::seek`].

pub use stretch_dsp::{Config, Source, Stretcher};

/// The frequency factor for a transposition in semitones (+12 = ×2).
#[must_use]
pub fn semitones(semitones: f32) -> f32 {
    (semitones / 12.0).exp2()
}
