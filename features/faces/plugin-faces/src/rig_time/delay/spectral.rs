//! Spectral: the echoes band by band. The signal is split into eight
//! bands, low at the bottom, high at the top, and each band is a delay of
//! its own: a row of echoes, each a delay time along and dimmer by the
//! feedback — the rows tilted by the tilt (`interval`), so the lows run
//! long and the highs short or the other way round. What the cuts take
//! out goes dark: the bands below the low cut and above the high cut fade.
//! Drawn as an analyser draws it, a waterfall of lit cells on a black
//! screen; a column at the left is the note itself, every band lit.
//!
//! A lab instrument: white enamel, or black.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use super::machine::{self, Look, Machine, ON, Room};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const DIM: &str = "#7f8ea6";
const BANDS: usize = 8;
const ECHOES: u32 = 7;
/// The bands' centres, Hz, low to high.
fn centre(band: usize) -> f64 {
    60.0 * 200f64.powf(band as f64 / (BANDS - 1) as f64)
}

const MACHINE: Machine = Machine {
    name: "Spectral",
    file: "71-dly-spectral",
    light: Look {
        ink: "#1a1d22",
        dim: "#646b76",
        panel: [(0.0, "#f7f8f9"), (0.6, "#eceef1"), (1.0, "#dde0e5")],
        edge: "#c3c8cf",
        texture: "#00000008",
        well: [(0.0, "#07090d"), (1.0, "#0c1018")],
        knob: [
            "color=#f6f7f9",
            "pointer=#1e3a8a",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#b7bdc6",
        ],
        wheel: [
            "color=#20242b",
            "pointer=#bcd6ff",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#c9ced5",
        ],
        accent: kit::BLUE_DEEP,
        scrim: "#eceef1e6",
    },
    dark: Look {
        ink: "#e6e9ee",
        dim: "#8a929e",
        panel: [(0.0, "#1c1e22"), (0.6, "#141619"), (1.0, "#0d0e10")],
        edge: "#2c2f35",
        texture: "#ffffff07",
        well: [(0.0, "#05070a"), (1.0, "#0a0d13")],
        knob: [
            "color=#26292e",
            "pointer=#bcd6ff",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#0a0b0d",
        ],
        wheel: [
            "color=#1b1d21",
            "pointer=#bcd6ff",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#3d424a",
        ],
        accent: kit::BLUE_LIGHT,
        scrim: "#141619e6",
    },
    title: (draw::family::SAIRA, 500.0, 17.0, 0.04, 0.58),
    front: [
        ("feedback", "Repeats"),
        ("interval", "Tilt"),
        ("high_cut", "High Cut"),
        ("level", "Level"),
    ],
    phone: [
        ("feedback", "Repeats"),
        ("interval", "Tilt"),
        ("level", "Level"),
    ],
    main: [
        ("feedback", "Repeats"),
        ("interval", "Tilt"),
        ("high_cut", "High Cut"),
        ("level", "Level"),
    ],
    picture,
    glass: true,
};

/// How much of `band` the cuts let through, 0…1.
fn pass(band: usize) -> String {
    let f = centre(band);
    format!(
        "smoothstep({:.1}, {:.1}, param({DLY}/high_cut)) * (1 - smoothstep({:.1}, {:.1}, param({DLY}/high_pass)))",
        f * 0.7,
        f * 1.5,
        f * 0.6,
        f * 1.3
    )
}

/// A band's delay time against the time: 1 at the middle band, tilted
/// across the bands by the tilt (lows long at 0, highs long at 30).
fn stretch(band: usize) -> String {
    let u = band as f64 / (BANDS - 1) as f64 - 0.5;
    format!("(1 + {u:.4} * 1.1 * (param({DLY}/interval) / 15 - 1))")
}

/// The waterfall in (`x, y, w, h`).
fn waterfall(
    b: &mut Builder<'_>,
    well: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    labels: bool,
    echoes: u32,
) {
    let mut f = draw::frame("Bands", x, y, w, h);
    f.clips_content = true;
    let f = b.add(well, f);
    let lx = if labels { 22.0 } else { 4.0 };
    let span = w - lx - 8.0;
    let row = h / BANDS as f64;
    // The band lines and their frequencies.
    let lines: Vec<Vec<(f64, f64)>> = (0..=BANDS)
        .map(|i| vec![(lx, i as f64 * row), (w, i as f64 * row)])
        .collect();
    b.add(
        f,
        ui::strokes(
            "band lines",
            &lines,
            0.5,
            draw::solid(&ui::alpha(DIM, 0.18)),
        ),
    );
    if labels {
        for (band, s) in [(0, "60"), (2, "270"), (4, "1k"), (6, "5k"), (7, "12k")] {
            let cy = h - (band as f64 + 0.5) * row;
            ui::silk_right(b, f, s, lx - 4.0, cy, 5.0, DIM, 700.0);
        }
    }
    let window = 2500.0;
    let cw = 8.0;
    let ch = (row - 3.0).max(3.0);
    for band in 0..BANDS {
        let cy = h - (band as f64 + 0.5) * row;
        // Low bands deep blue, high bands pale.
        let t = band as f64 / (BANDS - 1) as f64;
        let color = ui::blend(kit::BLUE_DEEP, kit::BLUE_LIGHT, 0.25 + 0.75 * t);
        let lane = b.add(
            f,
            draw::frame(&format!("band {band}"), lx, cy - ch / 2.0, span, ch),
        );
        motion::add(
            b,
            lane,
            Motion::new(Target::Opacity)
                .input(format!("0.12 + 0.88 * {}", pass(band)))
                .range(0.0, 1.0)
                .dynamics(machine::spring()),
        );
        // The note: every band lit at zero.
        b.add(
            lane,
            ui::glowing(
                draw::rounded(
                    draw::rect("note", 0.0, 0.0, 3.0, ch, draw::solid(kit::BLUE_LIGHT)),
                    1.0,
                ),
                kit::BLUE_LIGHT,
                1.0,
                kit::GLOW,
            ),
        );
        for k in 1..=echoes {
            let cell = draw::rounded(
                draw::rect(&format!("echo {k}"), 0.0, 0.0, cw, ch, draw::solid(&color)),
                1.5,
            );
            // The first echoes give off light; the rest are drawn.
            let cell = b.add(
                lane,
                if k <= 2 {
                    ui::glowing(cell, &color, 0.9, kit::GLOW)
                } else {
                    cell
                },
            );
            let at = format!(
                "clamp({k} * param({DLY}/time) * {} / {window}, 0, 1.1)",
                stretch(band)
            );
            motion::add(
                b,
                cell,
                Motion::new(Target::X)
                    .input(at)
                    .range(0.0, span - cw)
                    .dynamics(machine::spring()),
            );
            let fall = format!(
                "(0.06 + 0.94 * pow(param({DLY}/feedback), {})) * (0.3 + 0.7 * {ON})",
                k - 1
            );
            motion::add(
                b,
                cell,
                Motion::new(Target::Opacity)
                    .input(fall)
                    .range(0.0, 1.0)
                    .dynamics(machine::spring()),
            );
        }
    }
}

/// What the cuts let through, in (`x, y, w, h`): a bar a band, low to
/// high, as tall as the band passes.
fn cuts(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let f = b.add(well, draw::frame("Cuts", x, y, w, h));
    let base = h - 10.0;
    b.add(
        f,
        ui::strokes(
            "floor",
            &[vec![(0.0, base), (w, base)]],
            0.8,
            draw::solid(&ui::alpha(DIM, 0.4)),
        ),
    );
    let step = w / BANDS as f64;
    for band in 0..BANDS {
        let t = band as f64 / (BANDS - 1) as f64;
        let color = ui::blend(kit::BLUE_DEEP, kit::BLUE_LIGHT, 0.25 + 0.75 * t);
        let bar = b.add(
            f,
            draw::frame(
                &format!("band {band}"),
                step * band as f64 + step * 0.2,
                4.0,
                step * 0.6,
                base - 4.0,
            ),
        );
        b.add(
            bar,
            ui::glowing(
                draw::rounded(
                    draw::rect("bar", 0.0, 0.0, step * 0.6, base - 4.0, draw::solid(&color)),
                    1.5,
                ),
                &color,
                0.8,
                kit::GLOW,
            ),
        );
        motion::add(
            b,
            bar,
            Motion::new(Target::StretchY)
                .input(format!("0.04 + 0.96 * {}", pass(band)))
                .range(0.0, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(machine::spring()),
        );
    }
    ui::silk_left(b, f, "Low cut", 0.0, h - 3.0, 5.0, DIM, 800.0);
    ui::silk_right(b, f, "High cut", w, h - 3.0, 5.0, DIM, 800.0);
}

fn picture(
    b: &mut Builder<'_>,
    well: NodeId,
    (w, h): (f64, f64),
    _l: &Look,
    room: Room,
    _dark: bool,
) {
    match room {
        Room::Phone => waterfall(b, well, (6.0, 16.0, w - 10.0, h - 22.0), false, 5),
        Room::Lane => waterfall(b, well, (6.0, 16.0, w - 12.0, h - 24.0), true, ECHOES),
        Room::Tall => {
            waterfall(b, well, (6.0, 14.0, 410.0, h - 20.0), true, ECHOES);
            cuts(b, well, (440.0, 50.0, w - 456.0, h - 58.0));
        }
    }
    if room == Room::Lane {
        ui::silk_right(b, well, "0 → 2.5 s", w - 8.0, h - 5.0, 5.0, DIM, 700.0);
    }
    ui::silk_left(b, well, "8 bands · an echo each", 8.0, 9.0, 5.5, DIM, 800.0);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    MACHINE.face_at(dark, tier)
}

#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    MACHINE.back_at(dark, tier)
}

#[must_use]
pub fn tall(dark: bool) -> Face {
    MACHINE.tall(dark)
}
