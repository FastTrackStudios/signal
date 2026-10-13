//! Kit family: the "deluxe" boutique hardware — satin-anodised pedals with
//! metal knurled knobs, hex-nut stomps, mini levers, lit fader caps and the
//! capped amp knobs of a boutique head. Drawn from straight-on product
//! shots of modern boutique amp-sim suites (see `KIT.md`).
//!
//! - `knob.gold_knurl` — a machined knob seen a little from the front: a
//!   smooth domed cap over a knurled skirt, an index dot on the cap.
//! - `knob.amp_cap` — a head's knob seen straight on: a dark grooved rim, a
//!   metallic cap with an index dot, a printed tick ring.
//! - `switch.hex_stomp` — a stomp actuator on a hex nut, its status LED
//!   beside it.
//! - `switch.mini_lever` — a two-position mini toggle whose bat leans left
//!   or right.
//! - `switch.slot_lever` — a three-position lever in a horizontal slot.
//! - `lever.glow_cap` — a graphic-EQ fader: a deep slot, a lit cap.

mod knobs;
mod switches;

use crate::registry::ComponentDef;

/// A variant's finish: highlight, body, shadow, and the index/ink colour.
#[derive(Clone, Copy)]
pub(crate) struct Finish {
    pub hi: &'static str,
    pub mid: &'static str,
    pub lo: &'static str,
    pub mark: &'static str,
}

/// The metal finishes the family shares.
pub(crate) fn finish(variant: Option<&str>) -> Finish {
    match variant {
        Some("rose") => Finish {
            hi: "#ffeccd",
            mid: "#d6a276",
            lo: "#7a4234",
            mark: "#2a1712",
        },
        Some("black") => Finish {
            hi: "#9a9aa0",
            mid: "#2c2c30",
            lo: "#060607",
            mark: "#f4f4f2",
        },
        Some("pearl") => Finish {
            hi: "#ffffff",
            mid: "#e2e2de",
            lo: "#8e8f8b",
            mark: "#2b2b2b",
        },
        Some("magenta") => Finish {
            hi: "#f7b3cf",
            mid: "#b33c6c",
            lo: "#420c24",
            mark: "#1a050e",
        },
        Some("chrome") => Finish {
            hi: "#ffffff",
            mid: "#b9babc",
            lo: "#4b4d50",
            mark: "#1d1d1f",
        },
        Some("pink") => Finish {
            hi: "#f7c9d7",
            mid: "#d98fa8",
            lo: "#8d4a60",
            mark: "#2a0f19",
        },
        Some("white") => Finish {
            hi: "#ffffff",
            mid: "#e6e6e4",
            lo: "#a2a2a0",
            mark: "#3a3a3a",
        },
        _ => Finish {
            hi: "#fff2cc",
            mid: "#d6a95c",
            lo: "#6d4617",
            mark: "#2a2118",
        },
    }
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut v = knobs::defs();
    v.extend(switches::defs());
    v
}
