//! Kit family additions for hand-built pedals and boutique amps: glossy and
//! printed pedal caps, the hand-painted enclosure, a stomp with its LED
//! beside it, a segment display, grille cloths and a tolex cabinet — plus
//! hand lettering drawn as strokes ([`letters`]) and the label-maker tape
//! strip ([`pedal::add_tape_label`]).

pub(crate) mod amp;
pub(crate) mod knobs;
pub mod letters;
pub mod pedal;

use crate::registry::ComponentDef;

/// This module's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut v = knobs::defs();
    v.extend(pedal::defs());
    v.extend(amp::defs());
    v
}
