//! Faces: delay — six machines, one per family of the FTS delay's
//! profiles (`delay-ui/src/faces.rs`'s `EchoDesign`s), each drawn as the
//! hardware that family comes from rather than one panel re-coloured.
//!
//! - **Digital** — a 1U-style rack delay line: a VFD reading both times in
//!   ms *and* as note values, TAP, FREE/SYNC, LINK, an echo train drawn on
//!   the glass.
//! - **Tape** — a multi-head tape echo: the tape loop past a record head and
//!   three playback heads you switch in (each head lights), a small VU on the
//!   record level, chrome-skirted knobs on a black and green plate in tolex.
//! - **Analog** — a bucket-brigade stompbox: green enclosure, the BBD chip
//!   in a window, bypass and tap footswitches.
//! - **Pitch** — a pitch-shifting delay: repeats climbing a staircase on a
//!   graphic display, the interval in semitones.
//! - **Rhythmic** — a multi-tap delay laid out like a step sequencer: sixteen
//!   lit steps are the taps.
//! - **Special** — the black box for delays that don't give the signal
//!   back: reverse, spectral, diffuse, the smear drawn on a scope.
//!
//! Time, feedback and mix sit in the same place on every face; what is
//! between them is what the machine is about.

pub mod art;
pub mod parts;

mod digital;
mod tape;
mod analog;
mod pitch;
mod rhythmic;
mod special;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![digital::face(), tape::face(), analog::face(), pitch::face(), rhythmic::face(), special::face()]
}
