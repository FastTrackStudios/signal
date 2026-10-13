//! MultiTap: the tap ladder. The delay line is the top rail, read by six
//! taps hanging off it as the rungs of a ladder down to the output rail,
//! each with its own level. The taps spread along the line with the time,
//! bunched early or late by the spread (`interval`); a read pulse runs
//! down the rail once a delay time and each tap lights as it passes. Past
//! the last tap, the feedback sends the whole pattern round again, lower.
//!
//! Anodised: pale aluminium, or slate.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::machine::{self, Look, Machine, Room, ON, PERIOD};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const DIM: &str = "#7f8ea6";
const RAIL: &str = "#9fb0c8";
/// The taps, and each one's level: the pattern.
const TAPS: usize = 6;
const LEVELS: [f64; TAPS] = [1.0, 0.55, 0.82, 0.42, 0.68, 0.34];

const MACHINE: Machine = Machine {
    name: "MultiTap",
    file: "70-dly-multitap",
    light: Look {
        ink: "#18202c",
        dim: "#5c6878",
        panel: [(0.0, "#eef1f5"), (0.6, "#dde2e9"), (1.0, "#c9d0d9")],
        edge: "#aeb7c3",
        texture: "#ffffff50",
        well: [(0.0, "#141a24"), (1.0, "#0b0f16")],
        knob: ["color=#f4f6f9", "pointer=#1e3a8a", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#9aa5b4"],
        wheel: ["color=#1f2733", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#aab4c2"],
        accent: kit::BLUE_DEEP,
        scrim: "#dde2e9e6",
    },
    dark: Look {
        ink: "#e3e9f2",
        dim: "#8593a8",
        panel: [(0.0, "#2a313c"), (0.6, "#20262f"), (1.0, "#171b22")],
        edge: "#3a4452",
        texture: "#ffffff0a",
        well: [(0.0, "#0e131b"), (1.0, "#080b10")],
        knob: ["color=#2b3340", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0f1318"],
        wheel: ["color=#1a2029", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#4a5566"],
        accent: kit::BLUE_LIGHT,
        scrim: "#20262fe6",
    },
    title: (draw::family::ORBITRON, 600.0, 13.0, 0.06, 0.82),
    front: [("feedback", "Repeats"), ("interval", "Spread"), ("high_cut", "Tone"), ("level", "Level")],
    phone: [("feedback", "Repeats"), ("interval", "Spread"), ("level", "Level")],
    main: [("feedback", "Repeats"), ("interval", "Spread"), ("high_cut", "Tone"), ("level", "Level")],
    picture,
    glass: true,
};

/// The pattern's length across the ladder (a fraction of its span): longer
/// the longer the time.
fn length(ghosts: bool) -> String {
    // With the pattern drawn twice it takes half the ladder at most.
    let (lo, hi) = if ghosts { (0.32, 0.54) } else { (0.55, 0.97) };
    format!("lerp({lo}, {hi}, norm({DLY}/time))")
}

/// Where tap `i` (1…) sits along the pattern, 0…1: bunched early or late
/// by the spread.
fn place(i: usize) -> String {
    format!("pow({:.4}, lerp(1.8, 0.55, param({DLY}/interval) / 30))", i as f64 / TAPS as f64)
}

/// The ladder in `well` (`w × h`), its top rail at `top`.
fn ladder(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), top: f64, ghosts: bool) {
    let (x0, x1) = (24.0, w - 14.0);
    let span = x1 - x0;
    let bottom = h - 16.0;
    // The rails: the line in, the sum out; a ruler on the line.
    b.add(well, ui::strokes("rails", &[vec![(x0 - 10.0, top), (x1, top)], vec![(x0 - 10.0, bottom), (x1, bottom)]], 1.4, draw::solid(&ui::alpha(RAIL, 0.6))));
    let ticks: Vec<Vec<(f64, f64)>> = (0..=40).map(|i| {
        let x = x0 + span * f64::from(i) / 40.0;
        let l = if i % 5 == 0 { 4.0 } else { 2.0 };
        vec![(x, top - l), (x, top)]
    }).collect();
    b.add(well, ui::strokes("ruler", &ticks, 0.8, draw::solid(&ui::alpha(RAIL, 0.35))));
    ui::silk_right(b, well, "In", x0 - 13.0, top, 5.0, DIM, 800.0);
    ui::silk_right(b, well, "Out", x0 - 13.0, bottom, 5.0, DIM, 800.0);
    // The note, at the head of the line.
    b.add(well, ui::glowing(draw::rect("note", x0 - 1.0, top, 2.0, bottom - top, draw::solid(kit::BLUE_LIGHT)), kit::BLUE_LIGHT, 1.1, kit::GLOW));
    // The read pulse, running down the line once a delay time.
    let pulse = b.add(well, draw::emissive(draw::circle("read", x0, top, 3.2, draw::solid(kit::BLUE_LIGHT)), draw::hex(kit::BLUE_LIGHT), 1.3, kit::BLOOM));
    let run = format!("fract(time * 1000 / {PERIOD}) * {}", length(ghosts));
    motion::add(b, pulse, Motion::new(Target::X).input(run).range(0.0, span));
    let rung_h = bottom - top;
    let rung = |b: &mut Builder<'_>, name: &str, at: String, level: String, light: Option<String>, color: &str| {
        let lit = light.is_some();
        let col = b.add(well, draw::frame(name, x0 - 6.0, top - 6.0, 12.0, rung_h + 6.0));
        motion::add(b, col, Motion::new(Target::X).input(at).range(0.0, span).dynamics(machine::spring()));
        b.add(col, ui::strokes("rung", &[vec![(6.0, 6.0), (6.0, rung_h + 6.0)]], 1.0, draw::solid(&ui::alpha(RAIL, 0.4))));
        // Its level: a bar up from the out rail.
        // (Stretched as a group: a light stretches with its group, not on
        // its own node.)
        let bar = b.add(col, draw::frame("level", 3.5, 14.0, 5.0, rung_h - 8.0));
        let fill = draw::rect("bar", 0.0, 0.0, 5.0, rung_h - 8.0, draw::solid(color));
        b.add(bar, if lit { ui::glowing(fill, color, 0.9, kit::GLOW) } else { fill });
        motion::add(b, bar, Motion::new(Target::StretchY).input(level).range(0.0, 1.0).pivot(0.5, 1.0).dynamics(machine::spring()));
        // Its head on the line: a diamond, lit as the pulse passes.
        let head = draw::path("tap", 0.0, 0.0, 12.0, 12.0, draw::polygon(&[(6.0, 1.5), (10.5, 6.0), (6.0, 10.5), (1.5, 6.0)]), draw::solid(color));
        let head = b.add(col, draw::emissive(head, draw::hex(color), 1.2, kit::GLOW));
        if let Some(light) = light {
            motion::add(b, head, Motion::new(Target::Opacity).input(light).range(0.35, 1.0));
        }
    };
    for (k, lv) in LEVELS.iter().enumerate() {
        let i = k + 1;
        let at = format!("{} * {}", place(i), length(ghosts));
        let light = format!("pow(1 - fract(time * 1000 / {PERIOD} - {}), 6) * {ON}", place(i));
        rung(b, &format!("tap {i}"), at, format!("{lv} * (0.25 + 0.75 * {ON})"), Some(light), kit::BLUE_LIGHT);
    }
    if ghosts {
        // The pattern again, a pattern's length on, lower by the feedback.
        for (k, lv) in LEVELS.iter().enumerate() {
            let i = k + 1;
            let at = format!("(1 + {}) * {}", place(i), length(ghosts));
            rung(b, &format!("repeat {i}"), at, format!("{lv} * param({DLY}/feedback) * (0.25 + 0.75 * {ON})"), None, kit::BLUE);
        }
    }
}

fn picture(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), _l: &Look, room: Room, _dark: bool) {
    // A faint grid behind the ladder.
    let grid: Vec<Vec<(f64, f64)>> = (1..(w / 18.0) as i32).map(|i| vec![(f64::from(i) * 18.0, 0.0), (f64::from(i) * 18.0, h)]).collect();
    b.add(well, ui::strokes("grid", &grid, 0.5, draw::solid(&ui::alpha(RAIL, 0.06))));
    match room {
        Room::Phone => ladder(b, well, (w, h), 30.0, false),
        Room::Lane => ladder(b, well, (w, h), 30.0, true),
        Room::Tall => ladder(b, well, (w, h), 54.0, true),
    }
    ui::silk_left(b, well, if room == Room::Phone { "Tap ladder" } else { "Tap ladder · 6 taps" }, 8.0, 9.0, 5.5, DIM, 800.0);
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
