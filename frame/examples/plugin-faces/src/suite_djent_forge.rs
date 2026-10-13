//! Suite: **Djent Forge** (`suite-djent-forge`) — a modern progressive
//! metal amp-sim suite recreated unit by unit as frame surfaces, drawn
//! after the real plugin's in-window screenshots.
//!
//! | file | unit | form |
//! |---|---|---|
//! | `01-pre-standard-compressor` | Skyline Compressor: silver powder coat, blue lettering, GR dot meter | Pedal Standard 156 × 286 |
//! | `02-pre-standard-tape-drift` | Tape Drift echo: sage green, LED time readout, MS/NOTE toggle | Pedal Standard |
//! | `03-pre-standard-twin-octave` | Twin Octave: champagne, OCT−1 / OCT+1 / DIRECT | Pedal Standard |
//! | `04-pre-standard-tight-drive` | Tight Drive: navy sparkle, lit GATE dial | Pedal Standard |
//! | `05-pre-micro-havoc` | Havoc: oxblood micro box, one MIX knob | Pedal Micro 92 × 286 |
//! | `06-amp-head-ridge-clean` | Ridge Clean: black head, blue-lit ridge | Head 1200 × 507 |
//! | `07-amp-head-mech-rhythm` | Mech Rhythm: white machined head, orange-lit core | Head 1200 × 551 |
//! | `08-amp-head-slat-lead` | Slat Lead: black slatted cage over lit valves | Head 1200 × 595 |
//! | `09-cab-2x12` | Stack cab (2×2 speakers) with two mics and the mic / room controls | Cab 900 × 900 |
//! | `10-eq-tabletop-nine-band` | Ivory Nine-Band graphic EQ | Tabletop 1000 × 290 |
//! | `11-post-wide-treadle-modulator` | Treadle Modulator: rose-gold rocker + range display | Pedal Wide 222 × 286 |
//! | `12-post-bigbox-stereo-echo` | Stereo Echo: twin 16-segment readouts, ENGAGE / TAP | Pedal BigBox 404 × 286 |
//! | `13-post-standard-shimmer-verb` | Shimmer Verb: midnight blue, SHIMMER / NORMAL | Pedal Standard |
//! | `14-fx-wide-treadle-beam` | Beam pitch treadle with a patch list | Pedal Wide 222 × 286 |
//! | `15-fx-tabletop-fracture` | Fracture grain glitch: waveform screen, modes, grain dials | Tabletop 800 × 479 |
//!
//! Real names are never printed: every unit carries an invented name set in
//! the original's lettering style, and the maker's mark is an invented
//! one ("AZIMUTH circuits").

pub mod amps;
pub mod post;
pub mod pre;
pub mod rack;

use crate::face::Face;

/// The suite's plugin id.
pub const PLUGIN: &str = "suite-djent-forge";

/// The blue the pedals' lettering is printed in.
pub const BLUE: &str = "#1f5fa3";
/// The lit blue of the pedals' status LEDs.
pub const LED_BLUE: &str = "#7cc4ff";

/// The component every knob of the suite is placed as: the GPU-rendered
/// `knob.3d` (its vector fallback is `knob.render`).
pub const KNOB_ID: &str = "knob.3d";

/// The pedals' knob: a tall glossy black cap of seven deep flutes, a white
/// index line from the middle of its top toward the edge, `d` across.
#[must_use]
pub const fn pedal_knob(d: f64) -> rk::KnobStyle<'static> {
    rk::KnobStyle::new(KNOB_ID, "fluted", d, "#ffffff").opts(PEDAL_KNOB)
}

/// [`pedal_knob`]'s `knob.3d` options.
pub const PEDAL_KNOB: &[&str] = &[
    "color=#0b0b0c", "pointer=#f4f4f2", "ticks=none", "ribs=7", "rib_depth=0.11", "cap_r=0.97", "height=0.6", "dome=0.1", "edge=0.16",
    "rough=0.3", "coat=0.45", "spec=1.0", "index=line", "index_w=0.09", "index_in=0.12", "index_out=0.72",
];

/// A knob of the suite in any `knob.3d` preset, colour and extra options
/// (`color=`, `profile=`, `skirt=`, `index=`, `ticks=`, …).
#[must_use]
pub const fn knob(cap: &'static str, d: f64, ink: &'static str, opts: &'static [&'static str]) -> rk::KnobStyle<'static> {
    rk::KnobStyle::new(KNOB_ID, cap, d, ink).opts(opts)
}

/// The suite's lettering.
pub mod type_ {
    use crate::rigkit::Letter;
    use surface_kit::draw::family;

    /// Model names across a pedal: wide engraved capitals, widely spaced.
    #[must_use]
    pub const fn name(size: f64, color: &str) -> Letter<'_> {
        Letter::new(size, 400.0, color).family(family::MICHROMA).track(0.14)
    }
    /// Control legends: a semi-condensed bold technical sans.
    #[must_use]
    pub const fn legend(size: f64, color: &str) -> Letter<'_> {
        Letter::new(size, 650.0, color).family(family::SAIRA).stretch(0.92).track(0.02)
    }
    /// The maker's mark: heavy geometric tech capitals.
    #[must_use]
    pub const fn mark(size: f64, color: &str) -> Letter<'_> {
        Letter::new(size, 900.0, color).family(family::ORBITRON)
    }
    /// Fine print ("designed in …"): the engraved face, small and spaced.
    #[must_use]
    pub const fn fine(size: f64, color: &str) -> Letter<'_> {
        Letter::new(size, 400.0, color).family(family::MICHROMA).track(0.2)
    }
}

use crate::rigkit as rk;

/// Every unit of the suite, in signal order.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = pre::faces();
    v.extend(amps::faces());
    v.extend(rack::faces());
    v.extend(post::faces());
    v
}
