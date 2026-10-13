//! The desk units after the cab: a four-band EQ and a compressor, navy
//! steel plates on copper feet with screws at the corners.
//!
//! - `eq` — HPF and LPF (OFF at their stops), four frequency knobs with
//!   their printed frequencies, four ±12 dB gains, a bypass toggle and its
//!   green lamp, a strip of white tape marked by hand. Tabletop rack,
//!   1200 × 306.
//! - `comp` — COMPRESSOR in wide capitals, a 16-lamp gain-reduction row,
//!   INPUT and OUTPUT dials in a framed box, bypass and lamp. Tabletop
//!   rack, 1200 × 200.

use document_proto::{CornerRadii, NodeId};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Binding, Range, Unit};
use surface_widgets::Builder;

use super::parts::{Cap, PLUGIN, knob_scaled, marker, print, print_left, toggle};
use surface_kit::materials::{light, paint};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 1200.0;
const INK: &str = "#e2dcd6";

/// A navy steel plate `w × h` filling `root`, copper feet showing under it.
fn plate(b: &mut Builder<'_>, root: NodeId, h: f64) -> NodeId {
    ui::fit_root(b, root, W, h + 14.0);
    for x in [100.0, W - 200.0] {
        b.add(root, draw::rounded(draw::rect("foot", x, h - 10.0, 100.0, 24.0, draw::linear(90.0, &[(0.0, "#8a5234"), (0.3, "#e8a57a"), (0.55, "#f6c9a4"), (1.0, "#7a4228")])), 4.0));
    }
    let mut p = draw::frame("Plate", 0.0, 0.0, W, h);
    p.corner_radii = CornerRadii::uniform(22.0);
    p.stroke = document_proto::Stroke { paints: vec![draw::linear(90.0, &[(0.0, "#b8704e"), (0.5, "#4a3434"), (1.0, "#b8704e")])], weight: 2.5, ..document_proto::Stroke::default() };
    p.effects = vec![draw::drop_shadow("#000000c0", 0.0, 6.0, 14.0)];
    let p = b.add(root, p);
    // Navy powder-coat steel, its rolled edge lit from the upper left.
    paint::fill(b, p, (0.0, 0.0, W, h), 22.0, &paint::Material::Powder { color: "#30334a".into(), coarse: false });
    for e in light::edge("plate edge", (0.0, 0.0, W, h), 22.0, 5.0, 1.1, true) {
        b.add(p, e);
    }
    for (x, y) in [(29.0, 29.0), (W - 29.0, 29.0), (29.0, h - 29.0), (W - 29.0, h - 29.0)] {
        b.add(p, draw::with_effects(draw::circle("screw", x, y, 9.0, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#5a5f70"), (1.0, "#15171f")])), vec![draw::inner_shadow("#00000090", 0.0, 1.0, 2.0)]));
        b.add(p, draw::circle("screw head", x, y, 5.5, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#8a8f9c"), (1.0, "#2a2d36")])));
    }
    p
}

/// Bypass: a bat toggle over its green lamp.
fn bypass(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, ty: f64, jy: f64) {
    toggle(b, parent, addr, "Bypass", 24.0, cx, ty);
    let p = Place::new("Lamp").size(30.0).variant("green").bind(Binding::param(addr)).legend("").touch(TOUCH_MIN);
    let j = ui::put(b, parent, "button.jewel", &p, cx, jy);
    ui::strip_legend(b, j);
}

/// A filter's slope glyph (a knee, rising or falling).
fn slope(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, rising: bool) {
    let s = if rising { 1.0 } else { -1.0 };
    let pts = vec![(cx - 8.0 * s, cy + 4.0), (cx - 3.0 * s, cy - 3.0), (cx + 8.0 * s, cy - 3.0)];
    b.add(parent, ui::strokes("slope", &[pts], 1.8, draw::solid(INK)));
}

/// The filter knobs' lit rim: a teal lamp ring round the cap's foot,
/// glowing through the gap between knob and plate.
fn rim(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, d: f64) {
    let r = d / 2.0;
    b.add(parent, draw::circle("rim halo", cx, cy + 1.5, r * 1.22, draw::radial(&[(0.0, "#2fd8c000"), (0.8, "#2fd8c000"), (0.87, "#2fd8c066"), (1.0, "#2fd8c000")])));
    let ring = draw::stroked(draw::circle("rim", cx, cy + 1.0, r * 1.04, draw::solid("#00000000")), "#7ff4e0", 2.2);
    b.add(parent, ring);
}

/// A frequency knob: address, name, its printed marks, range, x.
type Band = (&'static str, &'static str, [&'static str; 9], f64, f64, f64);

const FREQS: [Band; 4] = [
    ("eq/f1", "LO freq", ["20", "22", "28", "40", "60", "80", "100", "150", "200"], 20.0, 200.0, 318.0),
    ("eq/f2", "LO MID freq", ["100", "110", "140", "200", "260", "350", "500", "800", "1k"], 100.0, 1000.0, 506.0),
    ("eq/f3", "HI MID freq", ["500", "550", "700", "1k", "1k2", "2k", "3k", "4k", "5k"], 500.0, 5000.0, 692.0),
    ("eq/f4", "HI freq", ["2k", "2k2", "2k8", "4k", "6k", "8k", "12k", "16k", "20k"], 2000.0, 20000.0, 879.0),
];
const GAINS: [(&str, &str, f64); 4] = [("eq/g1", "LO", 265.0), ("eq/g2", "LO MID", 485.0), ("eq/g3", "HI MID", 714.0), ("eq/g4", "HI", 933.0)];
const DB: [&str; 9] = ["-12", "-9", "-6", "-3", "0", "3", "6", "9", "12"];

pub fn eq() -> Face {
    const H: f64 = 306.0;
    let scene = canvas("Desk EQ", |b, root| {
        let p = plate(b, root, H);
        for (addr, name, cx, rising) in [("eq/hpf", "High-pass", 117.0, true), ("eq/lpf", "Low-pass", 1081.0, false)] {
            print(b, p, "OFF", cx, 22.0, 11.0, INK, 500.0, 0.14);
            rim(b, p, cx, 76.0, 66.0);
            knob_scaled(b, p, Cap::DeskKnurl, addr, name, 62.0, cx, 76.0, &[], INK);
            slope(b, p, cx, 136.0, rising);
        }
        for (addr, name, scale, _, _, cx) in FREQS {
            knob_scaled(b, p, Cap::DeskKnurl, addr, name, 58.0, cx, 70.0, &scale, INK);
        }
        for (addr, name, cx) in GAINS {
            knob_scaled(b, p, Cap::DeskKnurl, addr, name, 84.0, cx, 212.0, &DB, INK);
            print(b, p, name, cx, 284.0, 11.5, INK, 600.0, 0.14);
        }
        bypass(b, p, "eq/on", 1081.0, 196.0, 256.0);
        // The unit's name, and a strip of white tape saying which amp.
        print_left(b, p, "EQ", 68.0, 262.0, 36.0, "#efe6e0", 600.0, 0.02);
        b.add(p, draw::rotated(draw::with_effects(draw::rect("tape", 60.0, 170.0, 118.0, 44.0, draw::linear(180.0, &[(0.0, "#f4f2ee"), (1.0, "#dcd9d2")])), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)]), -12.0));
        let amp = marker(b, p, "Amp 1", 119.0, 188.0, 26.0, "#111111");
        if let Some(n) = b.scene.node_mut(amp) {
            *n = draw::rotated(n.clone(), -12.0);
        }
    });
    let mut params = vec![
        ui::param("eq/hpf", "High-pass", Range::log(20.0, 800.0, Unit::Hz), 20.0),
        ui::param("eq/lpf", "Low-pass", Range::log(2000.0, 20000.0, Unit::Hz), 20000.0),
        ui::toggle("eq/on", "EQ", true).featured(),
    ];
    for (addr, name, scale, lo, hi, _) in FREQS {
        // The knob follows its print: evenly spaced marks, uneven hertz.
        let marks: Vec<f64> = scale.iter().map(|s| hz(s)).collect();
        params.push(ui::param(addr, name, ui::printed(&marks, Unit::Hz), marks[4].clamp(lo, hi)));
    }
    for (addr, name, _) in GAINS {
        params.push(ui::param(addr, &format!("{} gain", name.to_lowercase()), Range::linear(-12.0, 12.0, Unit::Db), 0.0).bipolar().featured());
    }
    Face { file: "12-rack-desk-eq", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// A printed frequency (`"1k2"` = 1200 Hz).
fn hz(s: &str) -> f64 {
    match s.split_once('k') {
        Some((a, b)) => a.parse::<f64>().unwrap_or(0.0) * 1000.0 + if b.is_empty() { 0.0 } else { b.parse::<f64>().unwrap_or(0.0) * 100.0 },
        None => s.parse().unwrap_or(0.0),
    }
}

const LADDER: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "12", "14", "17", "20", "23", "26"];
const LADDER_COLORS: &str = "colors=#7ef060,#7ef060,#8ef05a,#9ef058,#aef056,#bef052,#cef04e,#dcee4a,#e8e446,#f0d840,#f6c43a,#f8b036,#fa9a30,#fa8a2c,#fa7a28,#fa6a24";

pub fn comp() -> Face {
    const H: f64 = 200.0;
    let scene = canvas("Desk Compressor", |b, root| {
        let p = plate(b, root, H);
        print_left(b, p, "COMPRESSOR", 56.0, 58.0, 48.0, "#efe6e0", 600.0, 0.06);
        // Each lamp sits in a black socket, so an unlit lamp reads dark.
        for i in 0..LADDER.len() {
            b.add(p, draw::circle("socket", 62.0 + 23.07 * i as f64, 118.0, 4.6, draw::radial_at(0.5, 0.4, 1.0, &[(0.0, "#050507"), (1.0, "#15161c")])));
        }
        let ladder = Place::new("Gain Reduction").size(23.1 * LADDER.len() as f64).scale(LADDER).options(&["gr", LADDER_COLORS]).reversed().ink(surface_kit::Ink::Color(INK.into())).keep_case().bind(Binding::stream("comp/gr"));
        let lad = ui::put(b, p, "meter.led_ladder", &ladder, 235.0, 118.0);
        for id in b.scene.walk(lad) {
            if let Some(n) = b.scene.node_mut(id)
                && n.name == "threshold"
            {
                n.visible = false;
            }
        }
        // The ladder's figures, printed over its lamps (26 at the left).
        for (i, n) in LADDER.iter().rev().enumerate() {
            print(b, p, n, 62.0 + 23.07 * i as f64, 92.0, 11.5, INK, 500.0, 0.0);
        }
        print(b, p, "GAIN REDUCTION", 247.0, 162.0, 11.5, INK, 600.0, 0.16);
        // The framed box round the two dials, gapped for their names.
        let (x0, y0, x1, y1) = (629.0, 14.0, 1137.0, 178.0);
        // The box's rule: rounded corners, broken under each dial's name.
        let r = 14.0;
        let arc = |cx: f64, cy: f64, from: f64| -> Vec<(f64, f64)> {
            (0..=8).map(|i| {
                let a = (from + 90.0 * f64::from(i) / 8.0).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            }).collect()
        };
        let (g1, g2) = (714.0, 933.0);
        let mut ring = vec![(g1 - 48.0, y1), (x0 + r, y1)];
        ring.extend(arc(x0 + r, y1 - r, 90.0));
        ring.push((x0, y0 + r));
        ring.extend(arc(x0 + r, y0 + r, 180.0));
        ring.push((x1 - r, y0));
        ring.extend(arc(x1 - r, y0 + r, 270.0));
        ring.push((x1, y1 - r));
        ring.extend(arc(x1 - r, y1 - r, 0.0));
        ring.push((g2 + 48.0, y1));
        b.add(p, ui::strokes("box", &[ring, vec![(g1 + 48.0, y1), (g2 - 48.0, y1)]], 1.6, draw::solid(INK)));
        for (addr, name, cx) in [("comp/input", "INPUT", 714.0), ("comp/output", "OUTPUT", 933.0)] {
            b.add(p, draw::rect("tick", cx - 0.8, y0 - 3.0, 1.6, 10.0, draw::solid(INK)));
            let k = knob_scaled(b, p, Cap::GreyDial, addr, name, 118.0, cx, 94.0, &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"], INK);
            // The dial prints its own index above; this one reads against the box's tick.
            for id in b.scene.walk(k) {
                if let Some(n) = b.scene.node_mut(id)
                    && n.name == "panel index"
                {
                    n.visible = false;
                }
            }
            print(b, p, name, cx, y1, 12.0, INK, 600.0, 0.16);
        }
        bypass(b, p, "comp/on", 1081.0, 58.0, 122.0);
    });
    let params = vec![
        ui::param("comp/input", "Input", Range::linear(0.0, 10.0, Unit::None), 5.0).featured(),
        ui::param("comp/output", "Output", Range::linear(0.0, 10.0, Unit::None), 5.0).featured(),
        ui::toggle("comp/on", "Compressor", true).featured(),
    ];
    let streams = vec![surface_proto::StreamInfo { id: surface_proto::EntryId::new("comp/gr"), name: "Gain Reduction".into(), kind: surface_proto::StreamKind::GainReduction, rate_hz: 30.0 }];
    Face { file: "13-rack-desk-comp", plugin: PLUGIN, scene, params, streams }
}
