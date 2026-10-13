//! Lo-Fi: a cheap sampler's echo — a toy keyboard's plastic case round a
//! blue-backlit pixel LCD. The note is a clean burst at the left; each
//! repeat is the same burst sampled again, coarser every time round:
//! its steps fewer, its levels fewer, the way an 8-bit echo crumbles. The
//! Tone knob crushes harder the darker it is set; the warble wobbles the
//! repeats. The LCD's pixel grid lies over all of it.
//!
//! Two finishes: light, putty plastic; dark, black plastic.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Device, Look};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

/// The lit pixels.
const PIXEL: &str = "#9cc4ff";

fn look(dark: bool) -> Look {
    if dark {
        Look {
            ink: "#ebe6db",
            dim: "#958d80",
            panel: [(0.0, "#2c2b28"), (0.6, "#232220"), (1.0, "#1a1918")],
            edge: "#3d3b36",
            well: [(0.0, "#0c1a36"), (1.0, "#071127")],
            knob: [
                "color=#33312d",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#111110",
            ],
            wheel: [
                "color=#1c1b19",
                "pointer=#bcd6ff",
                "index=dot",
                "skirt=plain",
                "skirt_color=#0b0b0a",
            ],
            accent: kit::BLUE_LIGHT,
            scrim: "#232220e6",
            light: PIXEL,
            faint: "#9cc4ff",
            metal: false,
        }
    } else {
        Look {
            ink: "#2a2620",
            dim: "#7a705f",
            panel: [(0.0, "#efe9dc"), (0.6, "#e2dac8"), (1.0, "#d3c9b3")],
            edge: "#b9ae96",
            well: [(0.0, "#0c1a36"), (1.0, "#071127")],
            knob: [
                "color=#f6f2e8",
                "pointer=#1e3a8a",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#a89c82",
            ],
            wheel: [
                "color=#3a362f",
                "pointer=#e8eeff",
                "index=dot",
                "skirt=plain",
                "skirt_color=#5a5246",
            ],
            accent: kit::BLUE_DEEP,
            scrim: "#e6dfcfe6",
            light: PIXEL,
            faint: "#9cc4ff",
            metal: false,
        }
    }
}

/// The case's moulding: a speaker grille's holes along the bottom edge.
fn dress(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), l: &Look) {
    // Only where it clears the knobs: under the bypass on a full lane.
    if (w - kit::W).abs() > 1.0 || h > kit::H {
        return;
    }
    let hole = ui::alpha(l.ink, 0.16);
    let mut dots = Vec::new();
    let mut x = w - 150.0;
    while x < w - 16.0 {
        dots.push(vec![(x, h - 9.0), (x + 0.01, h - 9.0)]);
        x += 6.0;
    }
    b.add(panel, ui::strokes("grille", &dots, 2.6, draw::solid(&hole)));
}

/// One burst, `bw` wide and ±`amp` about 0: the signal at `x`.
fn signal(x: f64, bw: f64, amp: f64) -> f64 {
    let u = x / bw;
    let env = (1.0 - u).powf(1.6) * (u * 12.0).min(1.0);
    -amp * env * (u * 3.2 * std::f64::consts::TAU).sin()
}

/// A burst's outline at `steps` samples: quantised to `levels` and held
/// flat between samples, else the clean curve through the same points —
/// the same commands either way, so one morphs into the other.
fn burst(bw: f64, amp: f64, steps: u32, levels: Option<f64>) -> String {
    let dx = bw / f64::from(steps);
    let mut pts = Vec::new();
    for s in 0..steps {
        let (x0, x1) = (dx * f64::from(s), dx * f64::from(s + 1));
        if let Some(q) = levels {
            let step = amp / q;
            let y = (signal(x0, bw, amp) / step).round() * step;
            pts.push((x0, amp + y));
            pts.push((x1, amp + y));
        } else {
            pts.push((x0, amp + signal(x0, bw, amp)));
            pts.push((x1, amp + signal(x1, bw, amp)));
        }
    }
    shell::poly(&pts)
}

fn picture(b: &mut Builder<'_>, v: NodeId, (w, h): (f64, f64), l: &Look) {
    let top = 22.0;
    let mid = (top + h - 10.0) / 2.0;
    let amp = (h - 10.0 - top) / 2.0;
    let lead = 16.0;
    let span = w - lead - 30.0;
    let window = shell::window(span);
    // The LCD's unlit segments: the zero line and a scale.
    b.add(
        v,
        ui::strokes(
            "zero",
            &[vec![(0.0, mid), (w, mid)]],
            1.0,
            draw::solid(&ui::alpha(l.faint, 0.18)),
        ),
    );
    let ticks: Vec<Vec<(f64, f64)>> = (0..=10)
        .map(|i| {
            let x = lead + span * f64::from(i) / 10.0;
            vec![(x, h - 6.0), (x, h - 3.0)]
        })
        .collect();
    b.add(
        v,
        ui::strokes("scale", &ticks, 1.0, draw::solid(&ui::alpha(l.faint, 0.3))),
    );
    let bw = if w < 300.0 { 30.0 } else { 44.0 };
    // The note: the clean burst.
    let note = b.add(v, draw::frame("note", lead, mid - amp, bw, 2.0 * amp));
    b.add(
        note,
        ui::glowing(
            shell::trace(
                "burst",
                (0.0, 0.0, bw, 2.0 * amp),
                burst(bw, amp, 32, None),
                "#e4eeff",
                1.6,
            ),
            kit::BLUE_LIGHT,
            1.2,
            kit::GLOW,
        ),
    );
    // The repeats: each sampled again, coarser — fewer levels each time
    // round, and fewer still the darker the Tone.
    for i in 1..=8_u32 {
        let fi = f64::from(i);
        let g = b.add(
            v,
            draw::frame(&format!("repeat {i}"), lead, mid - amp, bw, 2.0 * amp),
        );
        let steps = (16 - 2 * i).max(6);
        let clean = burst(bw, amp, steps, None);
        let crushed = burst(bw, amp, steps, Some(0.6f64.mul_add(-fi, 4.5).max(1.5)));
        let t = b.add(
            g,
            ui::glowing(
                shell::trace(
                    "burst",
                    (0.0, 0.0, bw, 2.0 * amp),
                    clean.clone(),
                    PIXEL,
                    1.6,
                ),
                kit::BLUE,
                1.2,
                kit::GLOW,
            ),
        );
        let crush = format!(
            "clamp((1 - norm({DLY}/high_cut)) * 1.4 + {:.2}, 0, 1)",
            0.1 * fi
        );
        motion::add(
            b,
            t,
            Motion::new(Target::Morph {
                shapes: vec![clean, crushed],
            })
            .input(crush)
            .range(0.0, 1.0)
            .dynamics(shell::spring()),
        );
        shell::slide(b, g, i, window, span);
        shell::fall(b, g, i, 0.5);
        // The warble: a cheap clock's wow.
        let wob = format!(
            "0.5 + 0.5 * sin(time * 6.2832 * param({DLY}/mod_rate) + {fi}) * norm({DLY}/mod_depth)"
        );
        motion::add(b, g, Motion::new(Target::Y).input(wob).range(-3.0, 3.0));
    }
    // The pixel grid over it all: the LCD's dots.
    let mut grid = Vec::new();
    let mut x = 1.5;
    while x < w {
        grid.push(vec![(x, 0.0), (x, h)]);
        x += 3.0;
    }
    let mut y = 1.5;
    while y < h {
        grid.push(vec![(0.0, y), (w, y)]);
        y += 3.0;
    }
    b.add(v, ui::bars("pixels", &grid, 0.8, "#05091466"));
    ui::silk_right(
        b,
        v,
        &shell::ms_label(window),
        w - 8.0,
        h - 12.0,
        5.5,
        &ui::alpha(l.faint, 0.5),
        700.0,
    );
}

const DEVICE: Device = Device {
    name: "Lo-Fi",
    base: "63-dly-lo-fi",
    caption: "8-bit sampler",
    title: (draw::family::VT323, 400.0, 24.0, 0.04),
    title_w: 84.0,
    front: [
        ("feedback", "Repeats"),
        ("high_cut", "Tone"),
        ("mod_depth", "Warble"),
        ("level", "Level"),
    ],
    phone: [
        ("feedback", "Repeats"),
        ("high_cut", "Tone"),
        ("level", "Level"),
    ],
    look,
    dress,
    picture,
};

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    shell::face_at(&DEVICE, dark, tier)
}

#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    shell::back_at(&DEVICE, dark, tier)
}

#[must_use]
pub fn tall(dark: bool) -> Face {
    shell::tall(&DEVICE, dark)
}
