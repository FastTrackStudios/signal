//! Faces: the modulation effects — one stompbox per family of the FTS
//! modulation plugin (`modulation-ui`'s `ModDesign`s).
//!
//! - `01-chorus` — a sky-blue stereo chorus: RATE, DEPTH, MIX up top, the
//!   voicing trims under them, the circuit (poly / BBD / tape / orbit /
//!   clean) on a selector.
//! - `02-flanger` — a purple flanger with its comb printed on the lid and a
//!   big REGEN, the jet control.
//! - `03-vibrato` — a cream vibrato in the amp manner, chicken-heads, no
//!   MIX (it is wet only: pitch modulation with a dry path is a chorus).
//! - `04-tremolo` — a brownface tremolo: the lamp-and-photocell drawn on
//!   the lid, and a shape selector.
//! - `05-wah` — the wah pedal, side on: the treadle rocks on its axle (drag
//!   it), with the voicing controls on the side plate and the engage stomp.
//!
//! Every stompbox has the LFO rate lamp — it pulses with the modulator, fed
//! by a `<face>/lfo` scalar stream — and a bypass footswitch whose LED is
//! the `<face>/on` param. Params are namespaced per face with real units:
//! rate in Hz (0.05–20, logarithmic), output in dB, the rest in %.

mod chorus;
mod flanger;
pub mod pedal;
mod tremolo;
mod vibrato;
mod wah;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    [
        chorus::face(),
        flanger::face(),
        vibrato::face(),
        tremolo::face(),
        wah::face(),
    ]
    .into_iter()
    .map(crate::reverb::ui::formatted)
    .collect()
}
