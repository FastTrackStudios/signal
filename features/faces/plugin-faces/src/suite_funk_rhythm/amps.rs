//! The three amp heads (Head form, 1200 wide; the height is the head's
//! own aspect with its handle and feet). Each faceplate strip is the
//! control surface, legends lowercase over the knobs (rendered knobs at the
//! angles the reference photos show), and a cartoon maker's mark — two
//! heavy brush-lettered words with a thick outline — at the right.
//!
//! - `06-critic-head-1200x580`: *The Tone Critic* — pale blue tolex with
//!   black moulded corners and a moulded carry handle on chrome posts, a
//!   cut blue plate whose bars are stacked zigzags — regular nested
//!   chevrons rising and hanging in turn across it — over glowing
//!   power valves, a cream strip: volume, bright, bass, middle, treble,
//!   master, presence, output, drive, power.
//! - `07-engine-head-1200x600`: *The Clean Engine* — a walnut box, a cream
//!   perforated grille, a green strip with white knurled knobs: volume,
//!   bright, bass, middle, treble, presence, output, power.
//! - `08-desk-head-1200x596`: *Studio Funk Desk* — a white vented chassis
//!   on wooden legs with the valves glowing through the slots, a cream
//!   strip with slate pointer knobs: comp (slow/fast), tube sat, high
//!   pass, low pass, lows, mids, highs (each with a frequency toggle),
//!   output, power.
//!
//! The valves' glow flickers (a motion on their light), the pilot LED
//! follows the power switch.

use document_proto::NodeId;
use surface_kit::materials::{Dir, Grain, Material, fill};
use surface_kit::{Place, TOUCH_MIN, draw, motion};
use surface_proto::ParamInfo;
use surface_widgets::Builder;

use super::parts::{self, HEAD_W, KnobLook, Type};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

/// A knob on a strip: address, legend, x.
struct Knob<'a>(&'a str, &'a str, f64);

/// How a strip's knobs look.
struct Strip<'a> {
    look: KnobLook<'a>,
    d: f64,
    ink: &'a str,
    knob_y: f64,
    legend_y: f64,
}

fn strip_knobs(b: &mut Builder<'_>, parent: NodeId, s: &Strip<'_>, ks: &[Knob<'_>]) {
    for Knob(addr, legend, x) in ks {
        parts::knob3d(b, parent, &s.look, addr, legend, s.d, (*x, s.knob_y), s.ink);
        parts::centred(b, parent, legend, (*x, s.legend_y), Type::new(12.0, s.ink, 600.0));
    }
}

/// A cream push power button with the pilot LED above it.
fn power(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, y): (f64, f64), led_y: f64, d: f64, led: &str) {
    let p = Place::new("Power").size(d).variant("cream").bind(ui::bind(addr)).options(&["nolamp"]).legend("").touch(TOUCH_MIN);
    let k = surface_kit::place(b, parent, "button.round_cap", &p).unwrap_or_else(|e| panic!("{e}"));
    ui::strip_legend(b, k);
    parts::centre_on(b, k, (x, y));
    parts::status_led(b, parent, addr, 5.0, (x, led_y), led, "dark", None);
}

/// The maker's mark: two stacked words in a heavy brush face, filled in
/// the plate's colour with a thick dark outline, the model under them.
#[allow(clippy::too_many_arguments)]
fn mark(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), fill: &str, outline: &str, model: &[&str], model_ink: &str) {
    let t = Type::new(27.0, fill, 400.0).family(draw::family::KNEWAVE).upper().track(-0.02);
    parts::outlined(b, parent, "Funk", (cx, cy - 14.0), t, outline, 2.2);
    parts::outlined(b, parent, "Works", (cx, cy + 12.0), t, outline, 2.2);
    for (i, l) in model.iter().enumerate() {
        parts::centred(b, parent, l, (cx, cy + 37.0 + 11.0 * i as f64), Type::new(10.5, model_ink, 700.0));
    }
}

/// Power valves behind a grille: glass envelopes with glowing plates and
/// filaments whose light flickers.
fn valves(b: &mut Builder<'_>, parent: NodeId, xs: &[f64], y: f64, w: f64, h: f64) {
    for (i, x) in xs.iter().enumerate() {
        let glass = draw::rounded(draw::rect("valve", x - w / 2.0, y, w, h, draw::linear(90.0, &[(0.0, "#2a2320"), (0.3, "#4a403a"), (0.5, "#221c19"), (1.0, "#15110f")])), w / 2.0);
        b.add(parent, draw::with_effects(glass, vec![draw::drop_shadow("#00000099", 0.0, 2.0, 6.0)]));
        b.add(parent, draw::rect("plate", x - w * 0.32, y + h * 0.25, w * 0.64, h * 0.5, draw::linear(90.0, &[(0.0, "#3b3a38"), (0.5, "#8a8986"), (1.0, "#3b3a38")])));
        let glow = draw::rect("filament", x - w * 0.18, y + h * 0.3, w * 0.36, h * 0.4, draw::radial(&[(0.0, "#ffd08a"), (0.5, "#ff8a2a"), (1.0, "#ff6a1000")]));
        let glow = b.add(parent, draw::emissive(glow, draw::hex("#ff8a2a"), 0.8, w * 0.9));
        motion::add(b, glow, motion::flicker(0.12 + 0.03 * i as f64));
        b.add(parent, draw::rect("glint", x - w * 0.36, y + h * 0.08, w * 0.08, h * 0.8, draw::solid("#ffffff24")));
    }
}

/// A moulded carry handle: a flattened arch `thick` px deep from `x0` to
/// `x1`, its feet on `y`, rising `rise`, with grip ribs along its top and
/// a chrome post and screw under each end.
fn moulded_handle(b: &mut Builder<'_>, root: NodeId, (x0, x1): (f64, f64), y: f64, rise: f64, thick: f64) {
    for x in [x0 + 16.0, x1 - 16.0] {
        let post = draw::rounded(draw::rect("handle post", x - 7.0, y - 20.0, 14.0, 24.0, draw::linear(90.0, &[(0.0, "#6d6f72"), (0.35, "#f4f5f6"), (0.6, "#a4a7ab"), (1.0, "#56585b")])), 3.0);
        b.add(root, draw::with_effects(post, vec![draw::drop_shadow("#00000080", 1.0, 2.0, 3.0)]));
        b.add(root, draw::circle("post screw", x, y - 16.0, 3.4, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#ffffff"), (1.0, "#6a6c70")])));
    }
    // The centreline: steep at the ends, flat across the grip.
    let n = 48;
    let centre: Vec<(f64, f64)> = (0..=n)
        .map(|i| {
            let u = f64::from(i) / f64::from(n);
            let lift = (u * std::f64::consts::PI).sin().powf(0.3);
            (x0 + (x1 - x0) * u, y - 18.0 - rise * lift)
        })
        .collect();
    let normal = |i: usize| {
        let (a, c) = (centre[i.saturating_sub(1)], centre[(i + 1).min(n as usize)]);
        let (dx, dy) = (c.0 - a.0, c.1 - a.1);
        let l = dx.hypot(dy).max(1e-6);
        (-dy / l, dx / l)
    };
    // Thicker across the grip than at the ends.
    let half = |i: usize| thick * 0.5 * (0.8 + 0.2 * (i as f64 / f64::from(n) * std::f64::consts::PI).sin());
    let mut outline: Vec<(f64, f64)> = (0..=n as usize).map(|i| {
        let (nx, ny) = normal(i);
        (centre[i].0 - nx * half(i), centre[i].1 - ny * half(i))
    }).collect();
    outline.extend((0..=n as usize).rev().map(|i| {
        let (nx, ny) = normal(i);
        (centre[i].0 + nx * half(i), centre[i].1 + ny * half(i))
    }));
    let (w, h) = (HEAD_W, y + 10.0);
    let body = draw::path("handle", 0.0, 0.0, w, h, draw::polygon(&outline), draw::linear(180.0, &[(0.0, "#3a3b3f"), (0.45, "#1d1e21"), (1.0, "#0b0b0c")]));
    b.add(root, draw::with_effects(body, vec![draw::drop_shadow("#00000099", 2.0, 4.0, 6.0)]));
    // The top edge catches the light; ribs across the grip.
    let top: Vec<(f64, f64)> = (4..=n as usize - 4).map(|i| {
        let (nx, ny) = normal(i);
        (centre[i].0 + nx * half(i) * 0.7, centre[i].1 + ny * half(i) * 0.7)
    }).collect();
    b.add(root, ui::strokes("handle light", &[top], 2.0, draw::solid("#ffffff3a")));
    let ribs: Vec<Vec<(f64, f64)>> = (14..=n as usize - 14).step_by(2).map(|i| {
        let (nx, ny) = normal(i);
        let (cx, cy) = centre[i];
        vec![(cx + nx * half(i) * 0.15, cy + ny * half(i) * 0.15), (cx + nx * half(i) * 0.6, cy + ny * half(i) * 0.6)]
    }).collect();
    b.add(root, ui::strokes("grip ribs", &ribs, 2.2, draw::solid("#00000070")));
}

/// The grille's bars: identical zigzags stacked `pitch` apart, so they
/// read as nested chevrons — rising stacks (`^`) at the zigzag's peaks,
/// hanging ones (`v`) midway between — every bar `slope` steep. Peaks at
/// `peak0 + k·period` across a grille `w × h`.
fn zigzag_bars((peak0, period): (f64, f64), slope: f64, pitch: f64, w: f64, h: f64) -> Vec<Vec<(f64, f64)>> {
    let half = period / 2.0;
    let drop = slope * half;
    // A peak at or left of the edge, then a corner every half period.
    let first = peak0 - period * (peak0 / period).ceil();
    let corners: Vec<(f64, f64)> = (0..).map(|i| (first + half * f64::from(i), if i % 2 == 0 { 0.0 } else { drop })).take_while(|(x, _)| *x < w + half).collect();
    let (n0, n1) = (-(drop / pitch).ceil() as i32 - 1, (h / pitch).ceil() as i32 + 1);
    (n0..=n1).map(|k| corners.iter().map(|(x, dy)| (*x, f64::from(k) * pitch + dy)).collect()).collect()
}

fn face(file: &'static str, scene: document_proto::Scene, params: Vec<ParamInfo>) -> Face {
    ui::formatted(Face { file, plugin: PLUGIN, scene, params, streams: Vec::new() })
}

// ── The Tone Critic ──────────────────────────────────────────────────────

pub(super) fn critic() -> Face {
    let (w, h) = (HEAD_W, 580.0);
    let scene = canvas("The Tone Critic", |b, root| {
        parts::page(b, root, w, h);
        let (top, bottom) = (64.0, 545.0);
        // Feet and handle.
        for x in [100.0, w - 170.0] {
            let foot = draw::rounded(draw::rect("foot", x, bottom - 10.0, 70.0, h - bottom + 10.0, draw::linear(90.0, &[(0.0, "#1a1a1c"), (0.3, "#3a3a3e"), (1.0, "#0d0d0e")])), 8.0);
            b.add(root, draw::with_effects(foot, vec![draw::drop_shadow("#00000080", 0.0, 3.0, 5.0)]));
        }
        moulded_handle(b, root, (w / 2.0 - 190.0, w / 2.0 + 190.0), top + 2.0, 30.0, 24.0);
        // The tolex box and its corners.
        let mut shell = draw::rounded(draw::frame("tolex box", 0.0, top, w, bottom - top), 10.0);
        shell.effects = vec![draw::drop_shadow("#00000080", 0.0, 6.0, 14.0)];
        let shell = b.add(root, shell);
        fill(b, shell, (0.0, 0.0, w, bottom - top), 10.0, &Material::Tolex { grain: Grain::Levant, color: "#7896c2".into(), scale: 1.4 });
        for n in surface_kit::materials::light::edge("tolex edge", (0.0, 0.0, w, bottom - top), 10.0, 6.0, 0.8, true) {
            b.add(shell, n);
        }
        for (x, y, fx, fy) in [(0.0, top, false, false), (w - 64.0, top, true, false), (w - 64.0, bottom - 64.0, true, true), (0.0, bottom - 64.0, false, true)] {
            // A moulded corner: an L of black plastic round the box's edge,
            // mirrored into each corner.
            let m = |px: f64, py: f64| (if fx { 64.0 - px } else { px }, if fy { 64.0 - py } else { py });
            let l = draw::polygon(&[m(0.0, 0.0), m(64.0, 0.0), m(64.0, 16.0), m(16.0, 16.0), m(16.0, 64.0), m(0.0, 64.0)]);
            let c = draw::path("corner", x, y, 64.0, 64.0, l, draw::linear(160.0, &[(0.0, "#4a4a4e"), (0.4, "#1b1b1d"), (1.0, "#060607")]));
            b.add(root, draw::with_effects(c, vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)]));
            let (sx, sy) = m(8.0, 8.0);
            b.add(root, draw::circle("corner rivet", x + sx, y + sy, 2.6, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#d9dadc"), (1.0, "#4a4b4e")])));
        }
        // The grille: a blue cut plate over glowing valves, its bars
        // stacked zigzags: nested chevrons rising and hanging in turn (the
        // reference, measured).
        let (gx, gy, gw, gh) = (30.0, top + 26.0, w - 60.0, 282.0);
        let plate = "#315982";
        let mut grille = draw::frame("grille", gx, gy, gw, gh);
        grille.fills = vec![draw::solid(plate)];
        let grille = b.add(root, grille);
        let (ow, oh) = (gw - 16.0, gh - 24.0);
        let mut opening = draw::frame("grille opening", 8.0, 5.0, ow, oh);
        opening.fills = vec![draw::linear(180.0, &[(0.0, "#07080a"), (1.0, "#101114")])];
        opening.clips_content = true;
        let opening = b.add(grille, opening);
        valves(b, opening, &[250.0, 380.0, 530.0, 670.0, 820.0], 60.0, 64.0, 170.0);
        // Peaks 456 apart from x = 110; a bar every 47 px down a column,
        // 20 px of it bar, 0.95 steep; the first bar's peak 39 px down.
        let bars: Vec<Vec<(f64, f64)>> = zigzag_bars((110.0, 456.0), 0.95, 47.0, ow, oh).into_iter().map(|l| l.into_iter().map(|(x, y)| (x, y + 39.0)).collect()).collect();
        let off = |dx: f64, dy: f64| bars.iter().map(|l| l.iter().map(|(x, y)| (x + dx, y + dy)).collect()).collect::<Vec<Vec<(f64, f64)>>>();
        let thick = 20.0 / 0.95_f64.hypot(1.0);
        b.add(opening, ui::strokes("bar shade", &off(2.0, 4.0), thick, draw::solid("#00000099")));
        b.add(opening, ui::strokes("bars", &bars, thick, draw::solid(plate)));
        b.add(opening, ui::strokes("bar light", &off(0.0, -thick * 0.5 + 1.0), 1.6, draw::solid("#8fb0dc55")));
        for n in surface_kit::materials::light::edge("grille recess", (0.0, 0.0, ow, oh), 0.0, 4.0, 1.0, false) {
            b.add(opening, n);
        }
        for n in surface_kit::materials::light::edge("plate edge", (0.0, 0.0, gw, gh), 0.0, 3.0, 0.7, true) {
            b.add(grille, n);
        }
        for (sx, sy) in [(5.0, 4.0), (gw - 5.0, 4.0), (5.0, gh - 12.0), (gw - 5.0, gh - 12.0)] {
            parts::screw(b, grille, sx, sy, 2.6, "#2a3440");
        }
        // The cream strip.
        let (sx, sy, sw, sh) = (30.0, top + 310.0, w - 60.0, 140.0);
        let strip = draw::rect("faceplate", sx, sy, sw, sh, draw::linear(180.0, &[(0.0, "#f1e8d4"), (1.0, "#ddd0b5")]));
        let strip = b.add(root, draw::with_effects(strip, vec![draw::drop_shadow("#00000066", 0.0, 1.0, 2.0)]));
        let ink = "#232120";
        let knob_y = 66.0;
        let s = Strip { look: KnobLook { cap: "knurled", opts: &["color=#18181a", "pointer=#f4f2ee", "index=dot", "index_w=0.09", "index_in=0.66", "index_out=0.66", "cap_r=0.92", "dome=0.1", "edge=0.12", "rough=0.28", "ticks=dots"] }, d: 44.0, ink, knob_y, legend_y: 20.0 };
        strip_knobs(b, strip, &s, &[
            Knob("critic/volume", "volume", 199.0),
            Knob("critic/bass", "bass", 338.0),
            Knob("critic/middle", "middle", 440.0),
            Knob("critic/treble", "treble", 540.0),
            Knob("critic/master", "master", 642.0),
            Knob("critic/presence", "presence", 742.0),
            Knob("critic/output", "output", 842.0),
        ]);
        for (addr, legend, x) in [("critic/bright", "bright", 267.0), ("critic/drive", "drive", 913.0)] {
            parts::toggle(b, strip, addr, legend, 17.0, "black", (x, knob_y));
            parts::centred(b, strip, legend, (x, 28.0), Type::new(12.0, ink, 600.0));
        }
        power(b, strip, "critic/power", (968.0, knob_y), 36.0, 24.0, "#8ef0ff");
        mark(b, strip, (1054.0, 54.0), "#f1e8d4", "#1c1b1b", &["The Tone", "Critic"], ink);
        parts::cable(b, strip, (89.0, knob_y), 12.0, &[(80.0, 110.0), (40.0, 170.0), (-28.0, 190.0)]);
    });
    // The knobs as the reference photo turns them.
    let params = vec![
        parts::dial10("critic/volume", "Volume", 5.0),
        parts::dial10("critic/bass", "Bass", 6.0),
        parts::dial10("critic/middle", "Middle", 5.0),
        parts::dial10("critic/treble", "Treble", 5.0),
        parts::dial10("critic/master", "Master", 4.7),
        parts::dial10("critic/presence", "Presence", 5.5),
        parts::dial10("critic/output", "Output", 5.0),
        parts::switch("critic/bright", "Bright", false),
        parts::switch("critic/drive", "Drive", false),
        parts::switch("critic/power", "Power", true),
    ];
    face("06-critic-head-1200x580", scene, params)
}

// ── The Clean Engine ─────────────────────────────────────────────────────

pub(super) fn engine() -> Face {
    let (w, h) = (HEAD_W, 600.0);
    let scene = canvas("The Clean Engine", |b, root| {
        parts::page(b, root, w, h);
        let bottom = 558.0;
        for x in [70.0, w - 150.0] {
            let foot = draw::rounded(draw::rect("foot", x, bottom - 10.0, 80.0, h - bottom + 10.0, draw::linear(90.0, &[(0.0, "#1c1c1e"), (0.3, "#3c3c40"), (1.0, "#0e0e0f")])), 8.0);
            b.add(root, draw::with_effects(foot, vec![draw::drop_shadow("#00000080", 0.0, 3.0, 5.0)]));
        }
        // The walnut box.
        let mut shell = draw::rounded(draw::frame("walnut box", 0.0, 0.0, w, bottom), 26.0);
        shell.effects = vec![draw::drop_shadow("#00000080", 0.0, 6.0, 14.0)];
        let wood = b.add(root, shell);
        fill(b, wood, (0.0, 0.0, w, bottom), 26.0, &Material::Wood { color: "#8f4a28".into(), dir: Dir::Horizontal });
        for n in surface_kit::materials::light::edge("box edge", (0.0, 0.0, w, bottom), 26.0, 10.0, 1.0, true) {
            b.add(wood, n);
        }
        // The cream perforated grille.
        let (gx, gy, gw, gh) = (35.0, 36.0, w - 70.0, 317.0);
        let mut grille = draw::frame("grille", gx, gy, gw, gh);
        grille.fills = vec![draw::linear(180.0, &[(0.0, "#eee1c3"), (1.0, "#dccba6")])];
        grille.clips_content = true;
        let grille = b.add(root, grille);
        let mut holes = String::new();
        let (cols, rows) = (40, 10);
        for r in 0..rows {
            for c in 0..cols {
                let (x, y) = (60.0 + f64::from(c) * 25.75, 50.0 + f64::from(r) * 25.0);
                holes.push_str(&format!("M {:.2} {y:.2} a 4 4 0 1 0 8 0 a 4 4 0 1 0 -8 0 Z ", x - 4.0));
            }
        }
        b.add(grille, draw::with_effects(draw::path("holes", 0.0, 0.0, gw, gh, holes, draw::solid("#1a1410")), vec![draw::drop_shadow("#ffffff66", 0.4, 1.0, 0.5)]));
        for n in surface_kit::materials::light::edge("grille recess", (0.0, 0.0, gw, gh), 0.0, 5.0, 1.0, false) {
            b.add(grille, n);
        }
        for (sx, sy) in [(12.0, 14.0), (gw - 12.0, 14.0), (12.0, gh - 14.0), (gw - 12.0, gh - 14.0)] {
            parts::screw(b, grille, sx, sy, 7.0, "#2a2a2a");
        }
        // The green strip.
        let (sx, sy, sw, sh) = (35.0, 357.0, w - 70.0, 165.0);
        let strip = draw::rect("faceplate", sx, sy, sw, sh, draw::linear(180.0, &[(0.0, "#27634a"), (1.0, "#1d4f3b")]));
        let strip = b.add(root, strip);
        fill(b, strip, (0.0, 0.0, sw, sh), 0.0, &Material::Powder { color: "#235a44".into(), coarse: false });
        let ink = "#f4f1e8";
        let knob_y = 80.0;
        let s = Strip { look: KnobLook { cap: "white_knurl", opts: &["ticks=dots"] }, d: 54.0, ink, knob_y, legend_y: 28.0 };
        strip_knobs(b, strip, &s, &[
            Knob("engine/volume", "volume", 205.0),
            Knob("engine/bass", "bass", 357.0),
            Knob("engine/middle", "middle", 477.0),
            Knob("engine/treble", "treble", 597.0),
            Knob("engine/presence", "presence", 717.0),
            Knob("engine/output", "output", 838.0),
        ]);
        parts::toggle(b, strip, "engine/bright", "bright", 18.0, "chrome", (281.0, knob_y));
        parts::centred(b, strip, "bright", (281.0, 40.0), Type::new(12.0, ink, 600.0));
        power(b, strip, "engine/power", (955.0, knob_y), 42.0, 26.0, "#ff5a3c");
        mark(b, strip, (1047.0, 60.0), "#235a44", "#f7f3ea", &["The Clean", "Engine"], ink);
        parts::cable(b, strip, (93.0, knob_y), 13.0, &[(88.0, 125.0), (60.0, 190.0), (-30.0, 250.0)]);
    });
    let params = vec![
        parts::dial10("engine/volume", "Volume", 5.0),
        parts::dial10("engine/bass", "Bass", 5.0),
        parts::dial10("engine/middle", "Middle", 5.0),
        parts::dial10("engine/treble", "Treble", 5.0),
        parts::dial10("engine/presence", "Presence", 5.0),
        parts::dial10("engine/output", "Output", 5.0),
        parts::switch("engine/bright", "Bright", false),
        parts::switch("engine/power", "Power", true),
    ];
    face("07-engine-head-1200x600", scene, params)
}

// ── Studio Funk Desk ─────────────────────────────────────────────────────

pub(super) fn desk() -> Face {
    let (w, h) = (HEAD_W, 596.0);
    let scene = canvas("Studio Funk Desk", |b, root| {
        parts::page(b, root, w, h);
        let (top, bottom) = (65.0, 560.0);
        // Wooden legs, the chrome handle.
        for (x, lean) in [(62.0, -1.0), (w - 118.0, 1.0)] {
            let leg = draw::polygon(&[(x, bottom - 6.0), (x + 56.0, bottom - 6.0), (x + 56.0 + 14.0 * lean, h), (x + 14.0 * lean, h)]);
            b.add(root, draw::path("leg", 0.0, 0.0, w, h, leg, draw::linear(90.0, &[(0.0, "#a86e3c"), (0.5, "#d49a5e"), (1.0, "#9c6534")])));
        }
        let bar = ui::strokes("handle", &[vec![(443.0, top + 4.0), (443.0, 8.0), (750.0, 8.0), (750.0, top + 4.0)]], 10.0, draw::solid("#d4d6d9"));
        b.add(root, draw::with_effects(bar, vec![draw::drop_shadow("#00000066", 0.0, 2.0, 3.0)]));
        // The white chassis.
        let mut body = draw::rounded(draw::frame("chassis", 0.0, top, w, bottom - top), 30.0);
        body.effects = vec![draw::drop_shadow("#0000008c", 0.0, 6.0, 14.0)];
        let body = b.add(root, body);
        fill(b, body, (0.0, 0.0, w, bottom - top), 30.0, &Material::Powder { color: "#f3f3f0".into(), coarse: false });
        for n in surface_kit::materials::light::edge("chassis edge", (0.0, 0.0, w, bottom - top), 30.0, 10.0, 1.0, true) {
            b.add(body, n);
        }
        // Valves behind the middle vent, then the three vents' slots.
        let mut well = draw::frame("valve well", 440.0, 38.0, 322.0, 262.0);
        well.fills = vec![draw::solid("#0e0d0c")];
        let well = b.add(body, well);
        valves(b, well, &[95.0, 228.0], 40.0, 64.0, 200.0);
        for (x0, x1) in [(28.0, 404.0), (440.0, 762.0), (853.0, 1173.0)] {
            for r in 0..10 {
                let y = 42.0 + f64::from(r) * 25.7;
                // Wall between slots over the well, the slot's dark mouth elsewhere.
                if x0 > 430.0 && x0 < 450.0 {
                    continue;
                }
                let slot = draw::rounded(draw::rect("slot", x0, y, x1 - x0, 10.0, draw::linear(180.0, &[(0.0, "#000000"), (1.0, "#26262a")])), 5.0);
                b.add(body, draw::with_effects(slot, vec![draw::drop_shadow("#ffffffb0", 0.0, 1.0, 0.0)]));
            }
        }
        for r in 0..11 {
            let y = 32.0 + f64::from(r) * 25.7;
            b.add(body, draw::rect("vent bar", 440.0, y, 322.0, 10.0, draw::linear(180.0, &[(0.0, "#ffffff"), (1.0, "#e2e2de")])));
        }
        // The cream strip.
        let (sx, sy, sw, sh) = (28.0, 315.0, w - 56.0, 148.0);
        let mut strip = draw::rounded(draw::frame("faceplate", sx, sy, sw, sh), 12.0);
        strip.fills = vec![draw::linear(180.0, &[(0.0, "#f2ecdd"), (1.0, "#e6dcc6")])];
        let strip = b.add(body, strip);
        for n in surface_kit::materials::light::edge("strip recess", (0.0, 0.0, sw, sh), 12.0, 5.0, 1.0, false) {
            b.add(strip, n);
        }
        let ink = "#262422";
        let knob_y = 78.0;
        let s = Strip { look: KnobLook { cap: "domed", opts: &["color=#4f6a74", "pointer=#f2f0ea", "index=line", "cap_r=0.9", "dome=0.16", "ribs=24", "rough=0.35", "ticks=dots"] }, d: 42.0, ink, knob_y, legend_y: 33.0 };
        strip_knobs(b, strip, &s, &[
            Knob("desk/comp", "comp", 132.0),
            Knob("desk/sat", "tube sat", 262.0),
            Knob("desk/hpf", "high pass", 357.0),
            Knob("desk/lpf", "low pass", 453.0),
            Knob("desk/lows", "lows", 551.0),
            Knob("desk/mids", "mids", 681.0),
            Knob("desk/highs", "highs", 809.0),
            Knob("desk/output", "output", 938.0),
        ]);
        for (addr, up, down, x) in [
            ("desk/fast", "slow", "fast", 197.0),
            ("desk/low_f", "150 Hz", "225 Hz", 618.0),
            ("desk/mid_f", "600 Hz", "900 Hz", 746.0),
            ("desk/high_f", "2.5 kHz", "4 kHz", 872.0),
        ] {
            parts::toggle(b, strip, addr, up, 16.0, "chrome", (x, knob_y));
            parts::centred(b, strip, up, (x, 49.0), Type::new(10.5, ink, 600.0));
            parts::centred(b, strip, down, (x, 108.0), Type::new(10.5, ink, 600.0));
        }
        power(b, strip, "desk/power", (1012.0, knob_y), 44.0, 24.0, "#8ef0ff");
        mark(b, strip, (1081.0, 58.0), "#f2ecdd", "#2a2826", &["Studio Funk", "Desk"], ink);
        parts::cable(b, strip, (56.0, knob_y), 12.0, &[(50.0, 125.0), (20.0, 200.0), (-40.0, 250.0)]);
    });
    // Every knob at noon, as the reference sets them.
    let params = vec![
        parts::pct("desk/comp", "Comp", 50.0),
        parts::pct("desk/sat", "Tube Sat", 50.0),
        parts::hz("desk/hpf", "High Pass", 20.0, 500.0, parts::log_at(20.0, 500.0, 0.5)),
        parts::hz("desk/lpf", "Low Pass", 2000.0, 20000.0, parts::log_at(2000.0, 20000.0, 0.5)),
        parts::db("desk/lows", "Lows", -12.0, 12.0, 0.0),
        parts::db("desk/mids", "Mids", -12.0, 12.0, 0.0),
        parts::db("desk/highs", "Highs", -12.0, 12.0, 0.0),
        parts::db("desk/output", "Output", -24.0, 12.0, -6.0),
        parts::switch("desk/fast", "Comp Speed", false),
        parts::switch("desk/low_f", "Lows Frequency", false),
        parts::switch("desk/mid_f", "Mids Frequency", false),
        parts::switch("desk/high_f", "Highs Frequency", false),
        parts::switch("desk/power", "Power", true),
    ];
    face("08-desk-head-1200x596", scene, params)
}
