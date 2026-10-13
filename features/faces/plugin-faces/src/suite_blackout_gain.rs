//! Suite: **Blackout Gain** (`suite-blackout-gain`) — a single-amp
//! high-gain suite in the plugin's black "blackout" skin, recreated unit by
//! unit as frame surfaces: three pedals, the head, a 4×12, a tabletop
//! graphic EQ, and two post-FX boxes.
//!
//! | file | unit | form |
//! |---|---|---|
//! | `01-pre-standard-ward-gate` | Ward: one-knob noise gate, petal rosette, twin skulls, hollow name | Pedal Standard 156 × 286 |
//! | `02-pre-standard-omen-drive` | Omen: three-knob drive, skulls face to face, hexagram round the red LED | Pedal Standard |
//! | `03-pre-standard-maul-boost` | Maul: one-knob boost, knot lattice, star medallion | Pedal Standard |
//! | `04-amp-head-blackout` | Blackout head: black leather, black panel, green jewel, seven knobs, I/II inputs | Head 1200 × 460 (the plugin's 2.9 : 1 faceplate plus its handle) |
//! | `05-cab-4x12` | 4×12 cabinet with two mics and room | Cab 900 × 900 |
//! | `06-eq-tabletop-nine-band` | Nine-band graphic EQ, blue-lit faders, HPF/LPF | Tabletop 1000 × 302 (3.3 : 1) |
//! | `07-post-large-crypt-delay` | Crypt Delay: LCD, SYNC/PING-PONG, ENGAGE + TAP | Pedal Large 286 × 286 (the original is near square) |
//! | `08-post-standard-tomb-verb` | Tomb Verb: MIX / DECAY / TONE | Pedal Standard |
//!
//! No real maker, artist or product name is printed: the maker's badge
//! reads an invented "VANTOM", and each unit has an invented name set in
//! the original's lettering style (tall condensed display capitals, the
//! gate's hollow block letters, the badge's squared extended caps).

pub mod amp;
pub mod art;
pub mod cab;
pub mod eq;
pub mod post;
pub mod pre;

use crate::face::Face;

/// The suite's plugin id.
pub const PLUGIN: &str = "suite-blackout-gain";

/// The component every knob of the suite is placed as: the GPU-rendered
/// `knob.3d` (its vector fallback is `knob.render`).
pub const KNOB_ID: &str = "knob.3d";

/// The pedals' knob: glossy black plastic, nine deep rounded flutes round
/// a flat top, a cream line from the shaft to the edge.
#[must_use]
pub const fn pedal_knob(d: f64) -> crate::rigkit::KnobStyle<'static> {
    crate::rigkit::KnobStyle::new(KNOB_ID, "fluted", d, art::INK).opts(PEDAL_KNOB)
}

/// [`pedal_knob`]'s `knob.3d` options.
pub const PEDAL_KNOB: &[&str] = &[
    "color=#040404",
    "pointer=#e6dcc0",
    "ribs=11",
    "rib_depth=0.08",
    "cap_r=0.98",
    "height=0.5",
    "dome=0.03",
    "edge=0.12",
    "rough=0.22",
    "coat=0.35",
    "spec=1.1",
    "tilt=20",
    "index=line",
    "index_w=0.08",
    "index_in=0.05",
    "index_out=0.7",
    "ticks=none",
];

/// The amp's knob: a black cap on a fine-knurled skirt, a white line index.
pub const AMP_KNOB: &[&str] = &[
    "color=#141414",
    "pointer=#f0eee8",
    "skirt=knurled",
    "skirt_color=#101010",
    "skirt_r=1.0",
    "skirt_h=0.2",
    "skirt_teeth=64",
    "cap_r=0.74",
    "height=0.56",
    "dome=0.06",
    "edge=0.1",
    "rough=0.3",
    "coat=0.5",
    "index=line",
    "index_w=0.08",
    "index_in=0.12",
    "index_out=0.9",
    "ticks=none",
];

/// The EQ's filter knob: a spun aluminium cap in a knurled chrome skirt, a
/// black index.
pub const EQ_KNOB: &[&str] = &[
    "aniso=0.45",
    "index_w=0.1",
    "index_in=0.15",
    "index_out=0.92",
    "skirt=knurled",
    "skirt_color=#c9ccd0",
    "skirt_metal=1",
    "skirt_rough=0.25",
    "skirt_teeth=60",
    "cap_r=0.82",
    "dome=0.08",
    "ticks=none",
];

/// The cab strip's knob: a dark grey knurled cap with a white dot index,
/// in its dotted ring.
pub const CAB_KNOB: &[&str] = &[
    "color=#2a2a2b",
    "pointer=#f2f2f0",
    "ribs=48",
    "rib_depth=0.025",
    "cap_r=0.84",
    "dome=0.08",
    "edge=0.12",
    "rough=0.35",
    "index=dot",
    "index_w=0.09",
    "index_in=0.62",
    "index_out=0.62",
];

/// The component for a knob of kind `id` (the amp's, the EQ's, the cab
/// strip's) — every other knob of the suite is placed through this: all
/// of them are [`KNOB_ID`] now, `id` naming the vector knob it replaced.
#[must_use]
pub const fn knob_id(_id: &str) -> &str {
    KNOB_ID
}

/// Every unit of the suite, in signal order.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = pre::faces();
    v.push(amp::face());
    v.push(cab::face());
    v.push(eq::face());
    v.extend(post::faces());
    v
}
