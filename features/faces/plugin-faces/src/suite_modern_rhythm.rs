//! Suite: **Modern Rhythm** (`suite-modern-rhythm`) — a modern metal
//! producer's amp-sim suite recreated unit by unit as frame surfaces, drawn
//! after the real plugin's in-window screenshot and its manual's
//! straight-on renders.
//!
//! | file | unit | form |
//! |---|---|---|
//! | `01-pre-standard-compressor` | Compressor: brushed silver, ATTACK toggle | Pedal Standard 156 × 286 |
//! | `02-pre-standard-overdrive-one` | Overdrive 1: brushed teal, clip toggle | Pedal Standard |
//! | `03-pre-standard-delay-one` | Delay 1: brushed purple, LCD time readout | Pedal Standard |
//! | `04-pre-standard-overdrive-two` | Overdrive 2: brushed orange, 4 knobs | Pedal Standard |
//! | `05-amp-head-hex-rhythm` | Hex Rhythm: black tolex, hex lattice over lit valves, cream strip | Head 1200 × 545 |
//! | `06-amp-head-croc-clean` | Croc Clean: grey croc skin, steel strip, cream pointer knobs | Head 1200 × 545 |
//! | `07-amp-head-cube-crunch` | Cube Crunch: cream tolex, cube lattice, slate strip | Head 1200 × 545 |
//! | `08-amp-head-cage-lead` | Cage Lead: perforated black cage over valves | Head 1200 × 545 |
//! | `09-cab-4x12` | 4×12 cabinet behind hex mesh, with the mic strip | Cab 900 × 900 |
//! | `10-eq-tabletop-nine-band` | Nine-band graphic with violet-lit caps | Tabletop 1000 × 254 |
//! | `11-post-bigbox-delay-two` | Delay 2: slate blue, LCD, ENGAGE / TAP TEMPO | Pedal BigBox 404 × 286 |
//! | `12-post-standard-reverb` | Reverb: dusty rose, 4 knobs | Pedal Standard |
//!
//! No real names are printed: units carry generic names in the original's
//! lettering style, and the player's signature on the amps is an invented
//! scribble.

pub mod amps;
pub mod cab;
pub mod common;
pub mod eq;
pub mod post;
pub mod pre;

use crate::face::Face;

/// The suite's plugin id.
pub const PLUGIN: &str = "suite-modern-rhythm";

/// Every knob of the suite is placed through this: the component for a
/// knob of kind `id`. The hardware knobs are all `knob.3d` (the GPU knob,
/// `knob.render` without the GPU stage); the cab's flat dials are the
/// plugin's own UI and draw themselves.
#[must_use]
pub const fn knob_id(id: &str) -> &str {
    id
}

/// Every unit of the suite, in signal order.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = pre::faces();
    v.extend(amps::faces());
    v.extend(cab::faces());
    v.extend(eq::faces());
    v.extend(post::faces());
    v
}
