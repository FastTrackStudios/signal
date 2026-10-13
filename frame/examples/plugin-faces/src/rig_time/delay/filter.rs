//! Filter: a delay with a resonant filter in its loop. At the left, the
//! loop itself — the line, the filter after it, and the feedback path back
//! round, its dashes running once a delay time. At the right, what the loop
//! does to the sound, on a frequency plot: the filter's peak at the cutoff
//! (the high cut), swept back and forth by the modulation (its rate and
//! depth), and under it each repeat's spectrum — the filter once more each
//! time round, so each narrower than the last, and lower by the feedback.
//! The low cut shades the bottom of the plot.
//!
//! A synth module: ivory, or black.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::machine::{self, Look, Machine, Room, ON, PERIOD};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const DIM: &str = "#8a90a0";
const GRID: &str = "#9fb0c8";
/// The plot's span, octaves (20 Hz … 20 kHz), and the filter's Q.
const OCTAVES: f64 = 10.0;
const Q: f64 = 2.2;
/// Passes drawn: the first, then the repeats.
const PASSES: u32 = 5;

const MACHINE: Machine = Machine {
    name: "Filter",
    file: "72-dly-filter",
    light: Look {
        ink: "#16181c",
        dim: "#62666f",
        panel: [(0.0, "#f1efe7"), (0.6, "#e6e3d8"), (1.0, "#d6d2c4")],
        edge: "#bdb8a8",
        texture: "#00000009",
        well: [(0.0, "#0d1117"), (1.0, "#080a0e")],
        knob: ["color=#16181c", "pointer=#f1efe7", "index=line", "index_w=0.11", "skirt=plain", "skirt_color=#a9a493"],
        wheel: ["color=#16181c", "pointer=#7fb2ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#c9c4b4"],
        accent: kit::BLUE_DEEP,
        scrim: "#e6e3d8e6",
    },
    dark: Look {
        ink: "#ecebe6",
        dim: "#8e9099",
        panel: [(0.0, "#1e1f22"), (0.6, "#17181a"), (1.0, "#101113")],
        edge: "#2e3034",
        texture: "#ffffff07",
        well: [(0.0, "#080b10"), (1.0, "#05070a")],
        knob: ["color=#2a2c30", "pointer=#ecebe6", "index=line", "index_w=0.11", "skirt=plain", "skirt_color=#0b0c0d"],
        wheel: ["color=#232529", "pointer=#bcd6ff", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#45484e"],
        accent: kit::BLUE_LIGHT,
        scrim: "#17181ae6",
    },
    title: (draw::family::OSWALD, 500.0, 19.0, 0.06, 0.48),
    front: [("feedback", "Repeats"), ("high_cut", "Cutoff"), ("mod_depth", "Sweep"), ("level", "Level")],
    phone: [("feedback", "Repeats"), ("high_cut", "Cutoff"), ("level", "Level")],
    main: [("feedback", "Repeats"), ("high_cut", "Cutoff"), ("mod_depth", "Sweep"), ("level", "Level")],
    picture,
    glass: true,
};

/// The filter's gain `d` octaves from its peak: a resonant band-pass.
fn gain(d: f64) -> f64 {
    let r = 2f64.powf(d);
    1.0 / (1.0 + Q * Q * (r - 1.0 / r).powi(2)).sqrt()
}

/// Where the peak sits on the plot now, 0…1: the high cut (500 Hz…20 kHz
/// of the plot's 20 Hz…20 kHz), swept by the modulation.
fn peak() -> String {
    format!("clamp(0.466 + 0.534 * norm({DLY}/high_cut) + 0.14 * param({DLY}/mod_depth) * sin(time * 6.2832 * param({DLY}/mod_rate)), 0.02, 0.98)")
}

/// The plot in (`x, y, w, h`).
fn plot(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64), labels: bool) {
    let mut f = draw::frame("Plot", x, y, w, h);
    f.clips_content = true;
    let f = b.add(well, f);
    let base = h - if labels { 10.0 } else { 2.0 };
    let top = 6.0;
    // Decades and a few levels.
    let decade = w / 3.0;
    let mut grid: Vec<Vec<(f64, f64)>> = (0..=3).map(|i| vec![(f64::from(i) * decade, 0.0), (f64::from(i) * decade, base)]).collect();
    for k in 1..9 {
        for i in 0..3 {
            let xx = (f64::from(i) + (f64::from(k) + 1.0).log10()) * decade;
            grid.push(vec![(xx, base - 3.0), (xx, base)]);
        }
    }
    for lv in [0.25, 0.5, 0.75] {
        grid.push(vec![(0.0, base - (base - top) * lv), (w, base - (base - top) * lv)]);
    }
    b.add(f, ui::strokes("grid", &grid, 0.5, draw::solid(&ui::alpha(GRID, 0.14))));
    b.add(f, ui::strokes("floor", &[vec![(0.0, base), (w, base)]], 0.8, draw::solid(&ui::alpha(GRID, 0.4))));
    if labels {
        for (i, s) in ["20", "200", "2k", "20k"].iter().enumerate() {
            let xx = i as f64 * decade;
            if i == 0 {
                ui::silk_left(b, f, s, xx + 2.0, h - 4.0, 5.0, DIM, 700.0);
            } else if i == 3 {
                ui::silk_right(b, f, s, xx - 2.0, h - 4.0, 5.0, DIM, 700.0);
            } else {
                ui::silk(b, f, s, xx, h - 4.0, 5.0, DIM, 700.0);
            }
        }
    }
    // The low cut: the bottom of the plot shaded up to it.
    let shade = b.add(f, draw::rect("low cut", 0.0, top, w, base - top, draw::linear(90.0, &[(0.0, "#00000000"), (0.7, &ui::alpha(GRID, 0.05)), (1.0, &ui::alpha(GRID, 0.22))])));
    motion::add(b, shade, Motion::new(Target::StretchX).input(format!("0.551 * pow(param({DLY}/high_pass) / 900, 0.3)")).range(0.0, 1.0).pivot(0.0, 0.5).dynamics(machine::spring()));
    // The passes, drawn about their peak at the sweep's x, moved with it.
    let per_oct = w / OCTAVES;
    let half = OCTAVES * per_oct;
    let sweep = b.add(f, draw::frame("sweep", -half, 0.0, 2.0 * half, base));
    motion::add(b, sweep, Motion::new(Target::X).input(peak()).range(0.0, w).dynamics(machine::spring()));
    let hh = base - top;
    for n in (1..=PASSES).rev() {
        let pts: Vec<(f64, f64)> = (0..=90).map(|k| {
            let d = -OCTAVES + 2.0 * OCTAVES * f64::from(k) / 90.0;
            (half + d * per_oct, hh * (1.0 - gain(d).powi(n as i32)))
        }).collect();
        let g = b.add(sweep, draw::frame(&format!("pass {n}"), 0.0, top, 2.0 * half, hh));
        let first = n == 1;
        let color = if first { kit::BLUE_LIGHT } else { kit::BLUE };
        if first {
            let mut d = format!("M 0 {hh:.1} ");
            for (px, py) in &pts {
                d += &format!("L {px:.1} {py:.1} ");
            }
            d += &format!("L {:.1} {hh:.1} Z", 2.0 * half);
            b.add(g, draw::path("fill", 0.0, 0.0, 2.0 * half, hh, d, draw::linear(180.0, &[(0.0, &ui::alpha(kit::BLUE, 0.4)), (1.0, &ui::alpha(kit::BLUE, 0.0))])));
        }
        // The filter's own curve gives off light; the repeats' are drawn.
        let curve = ui::strokes("response", &[pts], if first { 1.8 } else { 1.2 }, draw::solid(color));
        b.add(g, if first { ui::glowing(curve, color, 1.3, kit::GLOW) } else { curve });
        // Each repeat lower by the feedback; the whole dimmer while
        // bypassed.
        let fall = format!("pow(param({DLY}/feedback), {}) * (0.3 + 0.7 * {ON})", n - 1);
        motion::add(b, g, Motion::new(Target::StretchY).input(fall).range(0.02, 1.0).pivot(0.5, 1.0).dynamics(machine::spring()));
    }
    // The cutoff's marker on the floor.
    let mark = b.add(f, draw::frame("cutoff", -4.0, base - 6.0, 8.0, 6.0));
    b.add(mark, draw::path("marker", 0.0, 0.0, 8.0, 6.0, draw::polygon(&[(0.0, 6.0), (4.0, 0.0), (8.0, 6.0)]), draw::solid(kit::BLUE_LIGHT)));
    motion::add(b, mark, Motion::new(Target::X).input(peak()).range(0.0, w).dynamics(machine::spring()));
}

/// The loop in (`x, y, w, h`): in, the line, the filter, out — and the
/// feedback path back round, running once a delay time.
fn loop_diagram(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let f = b.add(well, draw::frame("Loop", x, y, w, h));
    let mid = h * 0.42;
    let bw = (w - 44.0) / 2.0;
    let (dx, fx) = (14.0, 30.0 + bw);
    let line = ui::alpha(GRID, 0.7);
    // The signal path.
    b.add(f, ui::strokes("path", &[vec![(0.0, mid), (dx, mid)], vec![(dx + bw, mid), (fx, mid)], vec![(fx + bw, mid), (w, mid)]], 1.2, draw::solid(&line)));
    for (bx, name) in [(dx, "Delay"), (fx, "Filter")] {
        let bx_node = draw::rounded(draw::rect(name, bx, mid - 13.0, bw, 26.0, draw::solid("#ffffff08")), 4.0);
        b.add(f, draw::stroked(bx_node, &line, 1.0));
        ui::silk(b, f, name, bx + bw / 2.0, mid - 5.0, 5.5, DIM, 800.0);
    }
    // The filter's own little peak, and the line's taps.
    let icon: Vec<(f64, f64)> = (0..=24).map(|k| {
        let d = -3.0 + 6.0 * f64::from(k) / 24.0;
        (fx + bw / 2.0 + d * bw * 0.12, mid + 9.0 - 9.0 * gain(d))
    }).collect();
    b.add(f, ui::glowing(ui::strokes("peak", &[icon], 1.2, draw::solid(kit::BLUE_LIGHT)), kit::BLUE_LIGHT, 1.0, kit::GLOW));
    let taps: Vec<Vec<(f64, f64)>> = (0..5).map(|i| {
        let tx = dx + 6.0 + (bw - 12.0) * f64::from(i) / 4.0;
        vec![(tx, mid + 3.0), (tx, mid + 9.0)]
    }).collect();
    b.add(f, ui::strokes("cells", &taps, 1.0, draw::solid(&ui::alpha(GRID, 0.5))));
    // The feedback path: out of the filter, under both, back into the line.
    let low = h - 10.0;
    let mut fb = ui::strokes("feedback", &[vec![(fx + bw + 6.0, mid), (fx + bw + 6.0, low), (dx - 6.0, low), (dx - 6.0, mid)]], 1.6, draw::solid(kit::BLUE));
    fb.stroke.dash_pattern = vec![4.0, 3.0];
    let fb = b.add(f, ui::glowing(fb, kit::BLUE, 0.8, kit::GLOW));
    motion::add(b, fb, Motion::new(Target::DashOffset).input(format!("fract(time * 1000 / {PERIOD})")).range(0.0, -14.0));
    motion::add(b, fb, Motion::new(Target::Opacity).input(format!("(0.25 + 0.75 * param({DLY}/feedback) / 0.95) * (0.3 + 0.7 * {ON})")).range(0.0, 1.0).dynamics(machine::spring()));
    b.add(f, draw::path("arrow", 0.0, 0.0, 0.0, 0.0, draw::polygon(&[(dx - 9.0, mid + 7.0), (dx - 6.0, mid + 1.0), (dx - 3.0, mid + 7.0)]), draw::solid(kit::BLUE)));
    ui::silk_left(b, f, "In", 0.0, mid - 7.0, 5.0, DIM, 800.0);
    ui::silk_right(b, f, "Out", w, mid - 7.0, 5.0, DIM, 800.0);
    ui::silk(b, f, "Feedback", (dx + fx + bw) / 2.0, low + 6.0, 5.0, DIM, 800.0);
}

fn picture(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), _l: &Look, room: Room, _dark: bool) {
    match room {
        Room::Phone => plot(b, well, (8.0, 18.0, w - 16.0, h - 22.0), false),
        Room::Lane => {
            loop_diagram(b, well, (12.0, 22.0, 150.0, h - 34.0));
            plot(b, well, (182.0, 12.0, w - 196.0, h - 18.0), true);
        }
        Room::Tall => {
            loop_diagram(b, well, (12.0, 20.0, 150.0, h - 30.0));
            plot(b, well, (182.0, 46.0, w - 196.0, h - 52.0), true);
        }
    }
    ui::silk_left(b, well, "Filter in the loop", 8.0, 9.0, 5.5, DIM, 800.0);
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
