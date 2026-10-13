//! Faces: the VCA and hybrid compressors — the console bus compressor in
//! its rack form, the desktop VCA with three knobs, and the digitally
//! controlled character compressor.
//!
//! Each is laid out from photographs of the real unit: its proportions, the
//! controls it really has, their stepped positions and printed scales, and
//! the meter it reads — an amber compression movement for the bus
//! compressor, a blue-printed decibel card for the desktop unit, an LED
//! ladder for the character compressor.

pub mod bus_comp;
pub(crate) mod character_comp;
mod logo;
pub(crate) mod ui;
pub mod vca_comp;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![bus_comp::face(), vca_comp::face(), character_comp::face()]
}

/// The VCA compressors redrawn for a host's FX lane (see [`crate::lane`]).
pub fn lane_faces() -> Vec<Face> {
    let mut v = bus_comp::lane::faces();
    v.extend(vca_comp::lane::faces());
    v.extend(character_comp::lane::faces());
    v
}
