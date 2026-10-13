//! Ice: a granular echo that freezes. The loop chops what it hears into
//! grains and throws them back pitched up, so each repeat comes back as a
//! spray of crystal shards. The picture, on a frosted pane: the note a
//! six-armed crystal glowing at the left; each repeat a delay time
//! further on, a cluster of thin shards climbing a little higher than the
//! last and smaller by the feedback; the Mod knob throws the shards
//! wider (the grains' scatter). Frost is etched into the pane.
//!
//! Two finishes, both anodised aluminium catching the light: light,
//! frosted silver; dark, glacier blue.

use document_proto::NodeId;
use document_proto::motion::{Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Device, Look};
use crate::face::Face;
use crate::reverb::ui;
use crate::reverb::jitter;
use crate::rig_time::kit::{self, DLY};

/// The shards' light.
const ICE: &str = "#cfe6ff";

fn look(dark: bool) -> Look {
    if dark {
        Look {
            ink: "#e4eef8",
            dim: "#8499ad",
            panel: [(0.0, "#22364b"), (0.6, "#182839"), (1.0, "#101c29")],
            edge: "#36506a",
            well: [(0.0, "#0a1a2c"), (1.0, "#06111e")],
            knob: ["color=#d8e3ee", "pointer=#1e3a8a", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0c1622"],
            wheel: ["color=#c9d6e3", "pointer=#1e3a8a", "index=dot", "skirt=plain", "skirt_color=#0c1622"],
            accent: kit::BLUE_LIGHT,
            scrim: "#182839e6",
            light: ICE,
            faint: "#a8cdf0",
            metal: true,
        }
    } else {
        Look {
            ink: "#17283a",
            dim: "#5f7387",
            panel: [(0.0, "#f1f5f9"), (0.6, "#dfe7ef"), (1.0, "#cdd8e3")],
            edge: "#b3c2d1",
            well: [(0.0, "#0a1a2c"), (1.0, "#06111e")],
            knob: ["color=#f6f9fc", "pointer=#1e3a8a", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#9fb0c2"],
            wheel: ["color=#e6edf4", "pointer=#1e3a8a", "index=dot", "skirt=plain", "skirt_color=#8293a6"],
            accent: kit::BLUE_DEEP,
            scrim: "#dfe7efe6",
            light: ICE,
            faint: "#a8cdf0",
            metal: true,
        }
    }
}

/// Brushing across the anodised plate.
fn dress(b: &mut Builder<'_>, panel: NodeId, (w, h): (f64, f64), l: &Look) {
    let lines: Vec<Vec<(f64, f64)>> = (0..(h / 2.5) as u32).map(|i| {
        let y = 1.0 + f64::from(i) * 2.5;
        vec![(0.0, y), (w, y)]
    }).collect();
    b.add(panel, ui::strokes("brush", &lines, 0.5, draw::solid(&ui::alpha(l.ink, 0.035))));
}

/// A six-armed crystal `r` across at (`cx`, `cy`): its arms and their
/// barbs.
fn crystal(cx: f64, cy: f64, r: f64) -> Vec<Vec<(f64, f64)>> {
    let mut v = Vec::new();
    for k in 0..6 {
        let a = 60.0 * f64::from(k);
        v.push(vec![(cx, cy), draw::polar(cx, cy, r, a)]);
        for (at, barb) in [(0.5, 0.3), (0.75, 0.2)] {
            let p = draw::polar(cx, cy, r * at, a);
            v.push(vec![draw::polar(p.0, p.1, r * barb, a - 45.0), p, draw::polar(p.0, p.1, r * barb, a + 45.0)]);
        }
    }
    v
}

/// A thin shard: a long rhombus `len` long at angle `a` (degrees from
/// up) about (`cx`, `cy`).
fn shard(cx: f64, cy: f64, len: f64, a: f64) -> String {
    let (tx, ty) = draw::polar(0.0, 0.0, len / 2.0, a);
    let (sx, sy) = draw::polar(0.0, 0.0, len * 0.13, a + 90.0);
    shell::closed(&[(cx + tx, cy + ty), (cx + sx, cy + sy), (cx - tx, cy - ty), (cx - sx, cy - sy)])
}

fn picture(b: &mut Builder<'_>, v: NodeId, (w, h): (f64, f64), l: &Look) {
    // Frost etched into the pane: faint crystals scattered over it.
    let mut frost = Vec::new();
    let n = (w / 40.0) as u32;
    for k in 0..n {
        let fk = f64::from(k);
        let (x, y) = (w * jitter(fk * 3.1 + 0.4), 18.0 + (h - 24.0) * jitter(fk * 7.7 + 1.3));
        frost.extend(crystal(x, y, 4.0 + 7.0 * jitter(fk * 1.9)));
    }
    b.add(v, ui::strokes("frost", &frost, 0.6, draw::solid(&ui::alpha(l.faint, 0.1))));
    let small = w < 300.0;
    let lead = if small { 22.0 } else { 30.0 };
    let span = w - lead - 24.0;
    let window = shell::window(span);
    let mid = h * 0.62;
    let r = if small { 10.0 } else { 15.0 };
    // The note: a crystal, glowing.
    b.add(v, ui::glowing(ui::strokes("crystal", &crystal(lead, mid, r), 1.4, draw::solid("#ffffff")), ICE, 1.3, kit::BLOOM));
    // The repeats: shard clusters, each pitched a little higher.
    let cw = if small { 22.0 } else { 40.0 };
    let ch = h * 0.6;
    for i in 1..=8_u32 {
        let fi = f64::from(i);
        let cy = mid - (fi * h * 0.03).min(h * 0.16);
        let g = b.add(v, draw::frame(&format!("repeat {i}"), lead - cw / 2.0, cy - ch / 2.0, cw, ch));
        let spray = b.add(g, draw::frame("spray", 0.0, 0.0, cw, ch));
        let mut d = String::new();
        for s in 0..7 {
            let fs = f64::from(s) + fi * 5.0;
            let x = cw * (0.15 + 0.7 * jitter(fs * 2.7));
            let y = ch * (0.25 + 0.5 * jitter(fs * 5.3 + 0.5));
            let len = ch * (0.3 + 0.35 * jitter(fs * 1.1 + 0.2));
            d.push_str(&shard(x, y, len, -40.0 + 80.0 * jitter(fs * 4.1)));
            d.push(' ');
        }
        b.add(spray, ui::glowing(draw::path("shards", 0.0, 0.0, cw, ch, d, draw::linear(180.0, &[(0.0, "#ffffff"), (1.0, &ui::alpha(ICE, 0.6))])), ICE, 1.0, kit::GLOW));
        motion::add(b, spray, Motion::new(Target::StretchX).input(format!("0.6 + 1.2 * norm({DLY}/mod_depth)")).range(0.0, 1.0).pivot(0.5, 0.5).dynamics(shell::spring()));
        shell::slide(b, g, i, window, span);
        motion::add(b, g, Motion::new(Target::Scale).input(shell::loudness(i)).range(0.12, 1.0).pivot(0.5, 0.5).dynamics(shell::spring()));
    }
    ui::silk_right(b, v, &shell::ms_label(window), w - 8.0, h - 7.0, 5.0, &ui::alpha(l.faint, 0.5), 700.0);
}

const DEVICE: Device = Device {
    name: "Ice",
    base: "66-dly-ice",
    caption: "Crystal grains",
    title: (draw::family::MICHROMA, 400.0, 16.0, 0.12),
    title_w: 74.0,
    front: [("feedback", "Repeats"), ("mod_depth", "Scatter"), ("high_cut", "Tone"), ("level", "Level")],
    phone: [("feedback", "Repeats"), ("mod_depth", "Scatter"), ("level", "Level")],
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
