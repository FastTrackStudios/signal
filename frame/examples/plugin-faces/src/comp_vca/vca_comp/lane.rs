//! The desktop VCA compressor in a host's FX column (see [`crate::lane`]).
//!
//! - **Lane**: the gloss front in its chassis, slimmed to the lane —
//!   THRESHOLD with its BELOW / ABOVE lamps, COMPRESSION (the biggest
//!   knob), OUTPUT GAIN, the logotype, and the big decibel meter at the
//!   right. The meter shows what the METER keys last picked (the keys, and
//!   the mains key, stay on the rack and tall faces).
//! - **Tall**: the unit's own arrangement, which already has the column's
//!   proportions — the three knobs over the POWER and METER keys at the
//!   left, the meter over the logotype at the right.
//!
//! Both are the rack face's layout read at their own [`Spots`], binding
//! the rack face's addresses.

use super::{Spots, build, params, streams};
use crate::face::{Face, canvas};
use crate::lane::Shape;

const LANE: Spots = Spots {
    size: (760.0, 164.0),
    caps: 14.0,
    rails: (6.0, 6.0),
    screw: 7.0,
    knobs: [(82.0, 60.0), (210.0, 68.0), (350.0, 60.0)],
    row: 96.0,
    header: 19.0,
    head: 11.5,
    numerals: 9.5,
    stacked: false,
    lamps: (37.0, 44.0),
    keys: None,
    meter: (636.0, 82.0, 172.0),
    logo: (476.0, 74.0, 0.6),
};

const TALL: Spots = Spots {
    size: (760.0, 328.0),
    caps: 20.0,
    rails: (12.0, 14.0),
    screw: 9.0,
    knobs: [(98.0, 72.0), (244.0, 90.0), (416.0, 72.0)],
    row: 146.0,
    header: 42.0,
    head: 13.0,
    numerals: 11.0,
    stacked: false,
    lamps: (66.0, 56.0),
    keys: Some((266.0, 60.0)),
    meter: (616.0, 124.0, 224.0),
    logo: (616.0, 256.0, 0.82),
};

pub fn faces() -> Vec<Face> {
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let spots = if shape == Shape::Tall { &TALL } else { &LANE };
            let scene = canvas(
                &format!("VCA Compressor ({})", shape.suffix()),
                |b, root| build(b, root, spots),
            );
            Face {
                file: shape.file("09-vca-comp"),
                plugin: "comp",
                scene,
                params: params(shape == Shape::Tall),
                streams: streams(),
            }
        })
        .collect()
}
