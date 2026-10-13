//! The hand-built pedals: five before the amp, two after it. Every one is
//! a navy enclosure under thick brushed paint, lettered by hand in paint
//! marker, named on a label-maker strip, with a chrome stomp and its LED
//! beside it in the bottom 30 %.
//!
//! Forms (docs/form-factors.md, 60 px/in, all 286 tall):
//! - `plain-boost` — Micro, 92 × 286
//! - `peak-filter`, `mild-drive`, `teal-drive`, `short-delay`,
//!   `plate-verb` — Standard, 156 × 286
//! - `haze-delay` — Large, 286 × 286 (a double-wide delay)

use frame::surface::proto::{Range, Unit};

use super::parts::{self, Cap, PEDAL_H, PLUGIN, knob, letter, pedal, stomp, tape, toggle};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const H: f64 = PEDAL_H;
/// Knob-legend capitals, px.
const LEG: f64 = 8.0;
/// Stomp row: actuator diameter and centre line.
const STOMP_D: f64 = 26.0;
const STOMP_Y: f64 = 0.79 * H;
/// Standard width.
const STD: f64 = 156.0;

fn level(ns: &str, id: &str, name: &str, default: f64) -> frame::surface::proto::ParamInfo {
    ui::percent(&format!("{ns}/{id}"), name, default)
}

fn on(ns: &str) -> frame::surface::proto::ParamInfo {
    ui::toggle(&format!("{ns}/on"), "Bypass", true).featured()
}

/// The clean boost: one big white knob, a BOOST toggle, the name on two
/// strips. Micro, 92 × 286.
pub fn plain_boost() -> Face {
    const W: f64 = 92.0;
    let scene = canvas("Plain Boost", |b, root| {
        let lid = pedal(b, root, W, Some("#e8e8f0"));
        knob(
            b,
            lid,
            Cap::WhiteFluted,
            "boost/volume",
            "Volume",
            50.0,
            W * 0.5,
            H * 0.135,
        );
        letter(b, lid, "VOLUME", W * 0.5, H * 0.265, LEG);
        letter(b, lid, "BOOST", W * 0.5, H * 0.39, LEG);
        toggle(b, lid, "boost/boost", "Boost", 13.0, W * 0.5, H * 0.47);
        tape(
            b,
            lid,
            "PLAIN",
            W * 0.52,
            H * 0.6,
            W * 0.72,
            14.0,
            "#c9c9d6",
            -0.6,
        );
        tape(
            b,
            lid,
            "BOOST",
            W * 0.46,
            H * 0.665,
            W * 0.68,
            14.0,
            "#c9c9d6",
            0.5,
        );
        stomp(
            b,
            lid,
            "boost/on",
            "Bypass",
            STOMP_D,
            W * 0.56,
            STOMP_Y,
            "#3b8cff",
            true,
        );
    });
    let params = vec![
        ui::param(
            "boost/volume",
            "Volume",
            Range::linear(0.0, 20.0, Unit::Db),
            10.0,
        )
        .featured(),
        ui::toggle("boost/boost", "Boost", false),
        on("boost"),
    ];
    Face {
        file: "01-pedal-micro-plain-boost",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The envelope filter: two pink caps (GAIN, PEAK), three mini toggles
/// (UP/DOWN, FAST/SLOW, HI/LO) lettered above and below.
pub fn peak_filter() -> Face {
    const W: f64 = STD;
    let scene = canvas("Peak Filter", |b, root| {
        let lid = pedal(b, root, W, Some("#ff4fa3"));
        for (addr, name, x) in [("filter/gain", "GAIN", 0.19), ("filter/peak", "PEAK", 0.81)] {
            knob(b, lid, Cap::PlumDome, addr, name, 40.0, W * x, H * 0.115);
            letter(
                b,
                lid,
                if name == "GAIN" { "GAiN" } else { name },
                W * x,
                H * 0.255,
                LEG,
            );
        }
        for (addr, name, up, down, x) in [
            ("filter/direction", "Direction", "UP", "DOWN", 0.19),
            ("filter/speed", "Speed", "FAST", "SLOW", 0.5),
            ("filter/range", "Range", "Hi", "LO", 0.81),
        ] {
            letter(b, lid, up, W * x, H * 0.32, LEG * 0.95);
            toggle(b, lid, addr, name, 13.0, W * x, H * 0.385);
            letter(b, lid, down, W * x, H * 0.452, LEG * 0.95);
        }
        tape(
            b,
            lid,
            "PEAK FILTER",
            W * 0.5,
            H * 0.62,
            W * 0.86,
            16.0,
            "#f2a9bd",
            -0.4,
        );
        stomp(
            b,
            lid,
            "filter/on",
            "Bypass",
            STOMP_D,
            W * 0.5,
            STOMP_Y,
            "#ff3b2f",
            true,
        );
    });
    let params = vec![
        level("filter", "gain", "Gain", 50.0).featured(),
        level("filter", "peak", "Peak", 50.0).featured(),
        ui::choice("filter/direction", "Direction", &["Down", "Up"], 1),
        ui::choice("filter/speed", "Speed", &["Slow", "Fast"], 1),
        ui::choice("filter/range", "Range", &["Lo", "Hi"], 1),
        on("filter"),
    ];
    Face {
        file: "02-pedal-standard-peak-filter",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The vintage overdrive: GAIN and LEVEL up top, TREBLE between them
/// below, all dark knurled caps.
pub fn mild_drive() -> Face {
    const W: f64 = STD;
    let scene = canvas("Mild Drive", |b, root| {
        let lid = pedal(b, root, W, None);
        for (addr, name, x, y, ly) in [
            ("mild/gain", "GAiN", 0.23, 0.13, 0.265),
            ("mild/level", "LEVEL", 0.77, 0.13, 0.262),
            ("mild/treble", "TREBLE", 0.5, 0.37, 0.51),
        ] {
            knob(b, lid, Cap::BrownDome, addr, name, 48.0, W * x, H * y);
            letter(b, lid, name, W * x, H * ly, LEG);
        }
        tape(
            b,
            lid,
            "MILD DRIVE",
            W * 0.5,
            H * 0.64,
            W * 0.84,
            16.0,
            "#dcdce6",
            0.3,
        );
        stomp(
            b,
            lid,
            "mild/on",
            "Bypass",
            STOMP_D,
            W * 0.5,
            STOMP_Y,
            "#ff3b2f",
            true,
        );
    });
    let params = vec![
        level("mild", "gain", "Gain", 50.0).featured(),
        level("mild", "level", "Level", 50.0).featured(),
        level("mild", "treble", "Treble", 50.0),
        on("mild"),
    ];
    Face {
        file: "03-pedal-standard-mild-drive",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The transparent overdrive: chevron caps (GAIN, LEVEL, TONE) and a
/// two-voicing toggle lettered BB over TS.
pub fn teal_drive() -> Face {
    const W: f64 = STD;
    let scene = canvas("Teal Drive", |b, root| {
        let lid = pedal(b, root, W, Some("#3fe08a"));
        // Two colourways: mint chevron caps on a mint strip, or black caps
        // with blue pointers on a blue strip.
        let mut art = Vec::new();
        for (addr, name, x, y, ly) in [
            ("teal/gain", "GAiN", 0.17, 0.115, 0.245),
            ("teal/level", "LEVEL", 0.83, 0.115, 0.245),
            ("teal/tone", "TONE", 0.5, 0.36, 0.49),
        ] {
            art.push(
                parts::knob_pair(
                    b,
                    lid,
                    [Cap::MintChevron, Cap::BluePointer],
                    addr,
                    name,
                    38.0,
                    W * x,
                    H * y,
                )
                .1,
            );
            letter(b, lid, name, W * x, H * ly, LEG);
        }
        letter(b, lid, "BB", W * 0.5, H * 0.065, LEG);
        toggle(b, lid, "teal/voice", "Voice", 13.0, W * 0.5, H * 0.14);
        letter(b, lid, "TS", W * 0.5, H * 0.215, LEG);
        let mint = tape(
            b,
            lid,
            "TEAL DRIVE",
            W * 0.5,
            H * 0.63,
            W * 0.88,
            16.0,
            "#8fd9a4",
            -0.3,
        );
        let blue = tape(
            b,
            lid,
            "TEAL DRIVE",
            W * 0.5,
            H * 0.63,
            W * 0.88,
            16.0,
            "#6f98e6",
            -0.3,
        );
        art.push([mint, blue]);
        parts::colourway(b, lid, "teal/colour", &art);
        stomp(
            b,
            lid,
            "teal/on",
            "Bypass",
            STOMP_D,
            W * 0.5,
            STOMP_Y,
            "#ff3b2f",
            true,
        );
    });
    let params = vec![
        level("teal", "gain", "Gain", 50.0).featured(),
        level("teal", "level", "Level", 50.0).featured(),
        level("teal", "tone", "Tone", 50.0),
        ui::choice("teal/voice", "Voicing", &["TS", "BB"], 1),
        ui::choice("teal/colour", "Colourway", &["Mint", "Midnight"], 0),
        on("teal"),
    ];
    Face {
        file: "04-pedal-standard-teal-drive",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The short delay: its time on a small segment display, teal caps for
/// TIME, FEEDBACK and BLEND.
pub fn short_delay() -> Face {
    const W: f64 = STD;
    let scene = canvas("Short Delay", |b, root| {
        let lid = pedal(b, root, W, Some("#4fd0e0"));
        parts::segments(
            b,
            lid,
            "short/time",
            (W * 0.26, H * 0.058, W * 0.48, 21.0),
            7,
            11.0,
            "#dfe9f6",
            frame::surface::proto::ValueFormat::decimals(1).unit("MS"),
            true,
        );
        for (addr, name, x, y, ly) in [
            ("short/time", "TiME", 0.19, 0.2, 0.32),
            ("short/feedback", "FEEDBACK", 0.8, 0.2, 0.32),
            ("short/blend", "BLEND", 0.5, 0.39, 0.505),
        ] {
            knob(b, lid, Cap::TealDome, addr, name, 40.0, W * x, H * y);
            letter(
                b,
                lid,
                name,
                W * x,
                H * ly,
                if name == "FEEDBACK" { LEG * 0.8 } else { LEG },
            );
        }
        tape(
            b,
            lid,
            "SHORT DELAY",
            W * 0.5,
            H * 0.63,
            W * 0.86,
            16.0,
            "#86b8e0",
            0.3,
        );
        stomp(
            b,
            lid,
            "short/on",
            "Bypass",
            STOMP_D,
            W * 0.5,
            STOMP_Y,
            "#3b8cff",
            true,
        );
    });
    let params = vec![
        ui::param(
            "short/time",
            "Time",
            Range::log(20.0, 600.0, Unit::Milliseconds),
            212.0,
        )
        .featured(),
        level("short", "feedback", "Feedback", 50.0).featured(),
        level("short", "blend", "Blend", 50.0),
        on("short"),
    ];
    Face {
        file: "05-pedal-standard-short-delay",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The double-wide delay after the amp: an alphanumeric display with its
/// tempo mode, SYNC and PING PONG toggles, four black bar caps, and two
/// stomps — ACTIVE and TAP TEMPO. Large, 286 × 286.
pub fn haze_delay() -> Face {
    const W: f64 = 286.0;
    let scene = canvas("Haze Delay", |b, root| {
        let lid = pedal(b, root, W, Some("#ff5ad0"));
        // The display: time, and the tempo mode lamps beside it.
        let (dx, dy, dw, dh) = (W * 0.07, H * 0.07, W * 0.62, H * 0.15);
        b.add(
            lid,
            frame::surface::kit::draw::rounded(
                frame::surface::kit::draw::rect(
                    "display bezel",
                    dx,
                    dy,
                    dw,
                    dh,
                    frame::surface::kit::draw::solid("#0b0b0d"),
                ),
                5.0,
            ),
        );
        parts::segments(
            b,
            lid,
            "haze/time",
            (dx + 5.0, dy + 5.0, dw * 0.66 - 5.0, dh - 10.0),
            5,
            dh * 0.5,
            "#ff4636",
            frame::surface::proto::ValueFormat::decimals(0).unit(""),
            true,
        );
        for (i, (a, u)) in [("FREE", "MS"), ("APP", "BPM"), ("TAP", "BPM")]
            .iter()
            .enumerate()
        {
            let y = dy + dh * (0.25 + 0.25 * i as f64);
            parts::print_right(b, lid, a, dx + dw * 0.82, y, 6.5, "#e6e6ea", 600.0, 0.02);
            parts::print_left(b, lid, u, dx + dw * 0.83, y, 6.5, "#8e8e96", 600.0, 0.02);
        }
        parts::radio(
            b,
            lid,
            "haze/mode",
            3,
            dx + dw * 0.95,
            dy + dh * 0.25,
            dh * 0.25,
            5.0,
            "#ffffff",
        );
        // SYNC and PING PONG.
        letter(b, lid, "SYNC", W * 0.765, H * 0.065, LEG * 0.95);
        toggle(b, lid, "haze/sync", "Sync", 12.0, W * 0.765, H * 0.155);
        letter(b, lid, "PiNG", W * 0.921, H * 0.05, LEG * 0.85);
        letter(b, lid, "PONG", W * 0.921, H * 0.09, LEG * 0.85);
        toggle(
            b,
            lid,
            "haze/pingpong",
            "Ping pong",
            12.0,
            W * 0.921,
            H * 0.165,
        );
        for (addr, name, x) in [
            ("haze/mix", "MiX", 0.13),
            ("haze/hardness", "HARDNESS", 0.385),
            ("haze/feedback", "FEEDBACK", 0.64),
            ("haze/time", "TiME", 0.89),
        ] {
            knob(b, lid, Cap::BlackBar, addr, name, 44.0, W * x, H * 0.35);
            letter(
                b,
                lid,
                name,
                W * x,
                H * 0.5,
                if name.len() > 5 { LEG * 0.85 } else { LEG },
            );
        }
        letter(b, lid, "ACTiVE", W * 0.13, H * 0.672, LEG);
        letter(b, lid, "TAP", W * 0.89, H * 0.64, LEG);
        letter(b, lid, "TEMPO", W * 0.89, H * 0.685, LEG);
        tape(
            b,
            lid,
            "HAZE DELAY",
            W * 0.51,
            H * 0.665,
            W * 0.46,
            16.0,
            "#c98fdc",
            0.3,
        );
        stomp(
            b,
            lid,
            "haze/on",
            "Active",
            STOMP_D,
            W * 0.13,
            STOMP_Y,
            "#ff6a5a",
            false,
        );
        let tap = frame::surface::kit::Place::new("Tap tempo")
            .size(STOMP_D)
            .bind(ui::bind("haze/tap"))
            .legend("")
            .options(&["side=left", "color=#ff6a5a", "momentary"]);
        let t = frame::surface::kit::place(b, lid, "switch.stomp_side_led", &tap)
            .unwrap_or_else(|e| panic!("{e}"));
        let (tw, th) = b
            .scene
            .node(t)
            .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
        ui::move_to(b, t, W * 0.89 - (tw - STOMP_D * 0.85), STOMP_Y - th / 2.0);
    });
    let params = vec![
        level("haze", "mix", "Mix", 50.0).featured(),
        level("haze", "hardness", "Hardness", 50.0),
        level("haze", "feedback", "Feedback", 50.0).featured(),
        ui::param(
            "haze/time",
            "Time",
            Range::log(20.0, 2000.0, Unit::Milliseconds),
            120.0,
        )
        .featured(),
        ui::choice(
            "haze/mode",
            "Tempo mode",
            &["Free", "App BPM", "Tap BPM"],
            2,
        ),
        ui::toggle("haze/sync", "Sync", false),
        ui::toggle("haze/pingpong", "Ping pong", false),
        ui::toggle("haze/tap", "Tap tempo", false),
        on("haze"),
    ];
    Face {
        file: "06-pedal-large-haze-delay",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

/// The studio reverb: coral-ringed caps (MIX, PRE-DELAY, DECAY, LOW CUT,
/// HIGH CUT) and a HALL/PLATE toggle; the LED to the stomp's right.
pub fn plate_verb() -> Face {
    const W: f64 = STD;
    let scene = canvas("Plate Verb", |b, root| {
        let lid = pedal(b, root, W, Some("#ff8a4a"));
        for (addr, name, x) in [
            ("plate/mix", "MiX", 0.17),
            ("plate/predelay", "PRE-DELAY", 0.5),
            ("plate/decay", "DECAY", 0.83),
        ] {
            knob(b, lid, Cap::CoralRing, addr, name, 40.0, W * x, H * 0.135);
            letter(
                b,
                lid,
                name,
                W * x,
                H * 0.262,
                if name.len() > 5 { LEG * 0.8 } else { LEG },
            );
        }
        for (addr, name, x) in [
            ("plate/lowcut", "LOW", 0.17),
            ("plate/highcut", "HiGH", 0.83),
        ] {
            knob(b, lid, Cap::CoralRing, addr, name, 40.0, W * x, H * 0.37);
            letter(b, lid, name, W * x, H * 0.49, LEG);
            letter(b, lid, "CUT", W * x, H * 0.53, LEG);
        }
        letter(b, lid, "HALL", W * 0.5, H * 0.315, LEG);
        toggle(b, lid, "plate/type", "Type", 13.0, W * 0.5, H * 0.375);
        letter(b, lid, "PLATE", W * 0.5, H * 0.435, LEG);
        tape(
            b,
            lid,
            "PLATE VERB",
            W * 0.5,
            H * 0.665,
            W * 0.82,
            16.0,
            "#f7a58a",
            -0.3,
        );
        stomp(
            b,
            lid,
            "plate/on",
            "Bypass",
            STOMP_D,
            W * 0.5,
            STOMP_Y,
            "#ff3b2f",
            false,
        );
    });
    let params = vec![
        level("plate", "mix", "Mix", 50.0).featured(),
        ui::param(
            "plate/predelay",
            "Pre-delay",
            Range::linear(0.0, 250.0, Unit::Milliseconds),
            20.0,
        ),
        ui::param(
            "plate/decay",
            "Decay",
            Range::log(0.2, 12.0, Unit::Seconds),
            2.0,
        )
        .featured(),
        ui::param(
            "plate/lowcut",
            "Low cut",
            Range::log(20.0, 1000.0, Unit::Hz),
            100.0,
        ),
        ui::param(
            "plate/highcut",
            "High cut",
            Range::log(1000.0, 20000.0, Unit::Hz),
            8000.0,
        ),
        ui::choice("plate/type", "Type", &["Plate", "Hall"], 1),
        on("plate"),
    ];
    Face {
        file: "11-pedal-standard-plate-verb",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
