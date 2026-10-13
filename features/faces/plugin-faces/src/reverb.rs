//! Faces: the reverbs — one per family of the FTS reverb (`reverb-ui`'s
//! `SpaceDesign`s), each drawn as the machine that family is.
//!
//! - `01-ir` — a convolution processor: the loaded impulse drawn on a glass
//!   display over a time axis, its file bank as a row of metal buttons.
//! - `02-hall` — a digital hall rack in the late-'70s manner: a phosphor
//!   display with the program, its decay drawn as reflections and a tail,
//!   and the RT60 / pre-delay / size read out; the parameters on a bank of
//!   long-throw faders, as the remote of such a unit had.
//! - `03-plate` — a steel plate: the sheet hung in its frame by its springs,
//!   the driver and pickups on it and the damper beside it; the remote's big
//!   DECAY dial printed in seconds, the output on a VU.
//! - `04-room` — a room box: the room in perspective with its first
//!   reflections, the early-reflection pattern as a tap diagram.
//! - `05-spring` — an amp's spring tank, open on the chassis: two coils
//!   between their transducers; chicken-heads for dwell, tone and mix.
//! - `06-ambient` — no walls: a lit halo that spreads with the wash.
//! - `07-random` — lines that wander: the delay taps drawn drifting.
//! - `08-special` — shimmer / chorale / magneto / non-linear: a black box
//!   with a lamp ladder of the tail's level and the engine on a selector.
//!
//! Params are namespaced per face (`ir/…`, `hall/…`, …) with real units:
//! decay in seconds, pre-delay in ms (printed 0…250 unevenly, as a dial is),
//! width to 200 %, the bass multiplier as ×. Every face declares `<face>/in`
//! and `<face>/out` stereo level streams; the ones with meters bind them.

mod ambient;
mod hall;
mod ir;
mod plate;
mod random;
mod room;
mod special;
pub mod spring;
pub mod ui;

use frame::surface::proto::{ParamInfo, Range, StreamInfo, Unit};

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    [
        ir::face(),
        hall::face(),
        plate::face(),
        room::face(),
        spring::face(),
        ambient::face(),
        random::face(),
        special::face(),
    ]
    .into_iter()
    .map(ui::formatted)
    .collect()
}

/// Pre-delay as a dial prints it: evenly spaced marks, uneven ms.
pub(crate) const PREDELAY_MS: &[f64] = &[0.0, 10.0, 25.0, 50.0, 100.0, 250.0];

/// Decay (RT60) in seconds, swept logarithmically.
pub(crate) fn decay(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    ui::param(addr, name, Range::log(min, max, Unit::Seconds), default).featured()
}

/// Pre-delay 0…250 ms on the printed scale.
pub(crate) fn predelay(addr: &str, name: &str, default: f64) -> ParamInfo {
    ui::param(
        addr,
        name,
        ui::printed(PREDELAY_MS, Unit::Milliseconds),
        default,
    )
    .featured()
}

/// A 0–100 % amount.
pub(crate) fn pct(addr: &str, name: &str, default: f64) -> ParamInfo {
    ui::percent(addr, name, default)
}

/// Stereo width, 0 (mono) to 200 %.
pub(crate) fn width(addr: &str) -> ParamInfo {
    ui::param(
        addr,
        "Width",
        Range::linear(0.0, 200.0, Unit::Percent),
        100.0,
    )
}

/// The low band's decay as a multiple of the mid's.
pub(crate) fn bass(addr: &str, name: &str) -> ParamInfo {
    ui::param(
        addr,
        name,
        Range::linear(
            0.25,
            2.0,
            Unit::Custom {
                suffix: "×".into()
            },
        ),
        1.0,
    )
}

/// Wet/dry.
pub(crate) fn mix(addr: &str, default: f64) -> ParamInfo {
    ui::percent(addr, "Mix", default).featured()
}

/// `<face>/in` and `<face>/out`, stereo.
pub(crate) fn levels(face: &str) -> Vec<StreamInfo> {
    vec![
        ui::level_stream(&format!("{face}/in"), "Input Level", 2),
        ui::level_stream(&format!("{face}/out"), "Output Level", 2),
    ]
}

/// A fixed pseudo-random value in 0..1 for `i` (the same every build, so a
/// drawn texture doesn't crawl).
pub(crate) fn jitter(i: f64) -> f64 {
    ((i * 12.9898).sin() * 43_758.545_3).fract().abs()
}
