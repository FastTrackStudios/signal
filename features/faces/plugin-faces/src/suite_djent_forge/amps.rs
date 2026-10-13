//! Djent Forge amps: three heads, each drawn straight on as the plugin's
//! amp page shows it — the faceplate *is* the control surface. Form: Head
//! (1200 wide; the height follows each head's own aspect, handle and feet
//! included).
//!
//! - **Ridge Clean** (1200 × 507): a black tolex head, a brushed dark grille
//!   over a recess lit blue from below round a dark pyramid ridge; a black
//!   strip with chrome end bars, BOOST and VOICE toggles, seven ticked
//!   knobs, a power toggle with three white pilot dots.
//! - **Mech Rhythm** (1200 × 551): a white machined head with engraved
//!   panel lines and an orange-lit mechanism (a turbine, a belt, pulleys)
//!   showing through a cut-out; white cup knobs with orange index dots and
//!   an orange power rocker.
//! - **Slat Lead** (1200 × 595): a dark cast head with a cage of vertical
//!   slats over glowing valves; flat slotted knobs with their names printed
//!   on them and a square orange power button.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw, motion};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::PLUGIN;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, KnobStyle};

pub fn faces() -> Vec<Face> {
    vec![ridge_clean(), mech_rhythm(), slat_lead()]
}

/// The seven controls every head carries, in panel order.
const TONE_STACK: [(&str, &str, &str); 7] = [
    ("gain", "Gain", "Gain"),
    ("bass", "Bass", "Bass"),
    ("middle", "Middle", "Mid"),
    ("treble", "Treble", "Treble"),
    ("presence", "Presence", "Pres"),
    ("master", "Master", "Master"),
    ("output", "Output", "Output"),
];

fn tone_params(ns: &str) -> Vec<surface_proto::ParamInfo> {
    let defaults = [5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0];
    TONE_STACK
        .iter()
        .zip(defaults)
        .map(|((addr, name, _), d)| {
            let p = rk::dial10(&format!("{ns}/{addr}"), name, d);
            if matches!(*addr, "gain" | "master") { p.featured() } else { p }
        })
        .collect()
}

// ── shared hardware ──────────────────────────────────────────────────────

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as f64 / f64::from(u32::MAX >> 1)
    }
}

/// A scatter of fine specks over (`x, y, w, h`): a cast or tolex surface's
/// tooth.
fn specks(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), seed: u64, density: f64, light: f64, dark: f64) {
    let mut rng = Lcg(seed);
    for (c, a) in [("#ffffff", light), ("#000000", dark)] {
        if a <= 0.0 {
            continue;
        }
        let mut d = String::new();
        for _ in 0..((w * h) / density) as usize {
            let (px, py) = (x + rng.next() * w, y + rng.next() * h);
            let s = 0.5 + rng.next() * 0.9;
            d.push_str(&format!("M {px:.1} {py:.1} h {s:.1} v {s:.1} h {:.1} Z ", -s));
        }
        b.add(parent, draw::path("specks", 0.0, 0.0, x + w, y + h, d, draw::solid(&rk::hexa(c, a))));
    }
}

/// Brushed streaks running down (`x, y, w, h`).
fn brushed(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), seed: u64, alpha: f64) {
    let mut rng = Lcg(seed);
    for (c, a, step) in [("#ffffff", alpha, 2.2), ("#000000", alpha * 1.3, 3.1), ("#ffffff", alpha * 0.7, 9.0)] {
        let mut d = String::new();
        let mut px = x + rng.next() * step;
        while px < x + w {
            let t = 0.4 + rng.next() * 0.9;
            let (y0, y1) = (y + rng.next() * h * 0.3, y + h - rng.next() * h * 0.3);
            d.push_str(&format!("M {px:.1} {y0:.1} h {t:.1} V {y1:.1} h {:.1} Z ", -t));
            px += step * (0.5 + rng.next());
        }
        b.add(parent, draw::path("brushing", 0.0, 0.0, x + w, y + h, d, draw::solid(&rk::hexa(c, a))));
    }
}

/// A panel screw head.
fn screw(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, light: bool) {
    let (hi, lo) = if light { ("#f4f5f6", "#8e9196") } else { ("#77797d", "#141416") };
    b.add(parent, draw::with_effects(draw::circle("screw", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &[(0.0, hi), (1.0, lo)])), vec![draw::drop_shadow("#00000066", 0.0, 0.6, 1.0)]));
    b.add(parent, draw::circle("screw hole", cx, cy, r * 0.42, draw::solid(if light { "#55585d" } else { "#050506" })));
}

/// An input jack: a chrome nut and the plug's black barrel, and its cable
/// hanging off the panel's lower left.
fn jack_and_cable(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, (w, h): (f64, f64), cable: &str, light_cable: bool) {
    let nut = draw::circle("jack nut", cx, cy, r, draw::conic(20.0, &[(0.0, "#f2f2f0"), (0.25, "#77787a"), (0.5, "#e2e2e0"), (0.75, "#5f6062"), (1.0, "#f2f2f0")]));
    b.add(parent, draw::with_effects(nut, vec![draw::drop_shadow("#00000080", 0.0, 1.5, 2.5)]));
    b.add(parent, draw::circle("jack barrel", cx, cy, r * 0.62, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#6d6e72"), (1.0, "#101012")])));
    // The cable: a heavy line out and down, with a lighter core.
    let pts = [(cx, cy), (cx - r * 0.6, cy + r * 3.0), (cx - r * 2.4, cy + r * 5.2), (cx - r * 5.5, h - 2.0)];
    rk::curve(b, parent, "cable", (w, h), &pts, r * 0.95, cable);
    rk::curve(b, parent, "cable sheen", (w, h), &pts, r * 0.22, if light_cable { "#ffffff66" } else { "#ffffff26" });
}

/// A foot under the head: a turned cylinder.
fn foot(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), cone: bool) {
    if cone {
        let d = format!("M {x:.1} {y:.1} L {:.1} {y:.1} L {:.1} {:.1} L {:.1} {:.1} Z", x + w, x + w * 0.58, y + h, x + w * 0.42, y + h);
        b.add(parent, draw::path("foot", 0.0, 0.0, x + w, y + h, d, draw::linear(90.0, &[(0.0, "#0c0c0d"), (0.5, "#3a3b3e"), (1.0, "#0c0c0d")])));
    } else {
        b.add(parent, draw::rounded(draw::rect("foot", x, y, w, h, draw::linear(90.0, &[(0.0, "#1a1b1d"), (0.35, "#8a8b8e"), (0.55, "#3e3f42"), (1.0, "#141516")])), 2.0));
    }
}

/// A two-position mini toggle for a head (chrome bat on a nut), its legend
/// above at `ly`.
#[allow(clippy::too_many_arguments)]
fn head_toggle(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), size: f64, legend: Option<(f64, Ink<'_>)>) -> NodeId {
    let t = rk::mini_toggle(b, parent, name, addr, (cx, cy), size, "chrome");
    if let Some((ly, ink)) = legend {
        rk::label(b, t, name, (cx, ly), ink);
    }
    t
}

// ── 06 · Ridge Clean ─────────────────────────────────────────────────────

/// Ridge Clean — Head, 1200 × 507. See the module docs.
#[allow(clippy::too_many_lines)]
fn ridge_clean() -> Face {
    let ns = "dfclean";
    let a = |s: &str| format!("{ns}/{s}");
    let (w, h) = (1200.0, 507.0);
    let scene = canvas("Ridge Clean", |b, root| {
        parts::fit_root(b, root, w, h);
        // Handle.
        let hd = "M 433 31 L 445 12 Q 452 3 464 3 L 736 3 Q 748 3 755 12 L 767 31 L 748 31 Q 740 20 730 20 L 470 20 Q 460 20 452 31 Z";
        b.add(root, draw::with_effects(draw::path("handle", 0.0, 0.0, w, h, hd.into(), draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#0f1011")])), vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)]));
        // Feet, then the cabinet.
        foot(b, root, (80.0, 474.0, 36.0, 30.0), true);
        foot(b, root, (1084.0, 474.0, 36.0, 30.0), true);
        let cab = draw::with_effects(
            draw::rounded(draw::rect("cabinet", 3.0, 31.0, 1194.0, 444.0, draw::linear(180.0, &[(0.0, "#2b2c2f"), (0.5, "#1c1d1f"), (1.0, "#141416")])), 24.0),
            vec![draw::drop_shadow("#00000099", 0.0, 8.0, 18.0), draw::inner_shadow("#ffffff1c", 0.0, 2.0, 2.0)],
        );
        b.add(root, draw::stroked(cab, "#050506", 1.5));
        specks(b, root, (10.0, 36.0, 1180.0, 432.0), 0xc1ea, 60.0, 0.03, 0.10);
        // The front recess.
        let front = parts::well(b, root, "front", (35.0, 63.0, 1130.0, 379.0), draw::solid("#0d0e10"), 6.0);
        // Upper grille: brushed dark metal.
        b.add(front, draw::rect("grille", 0.0, 0.0, 1130.0, 140.0, draw::linear(180.0, &[(0.0, "#2e2f33"), (1.0, "#1f2023")])));
        brushed(b, front, (0.0, 0.0, 1130.0, 140.0), 0xb12e, 0.05);
        b.add(front, draw::rect("grille edge", 0.0, 139.0, 1130.0, 2.0, draw::solid("#0a0a0b")));
        // The lit recess: blue light rising from below behind the ridge.
        let glow = draw::rect("backlight", 0.0, 141.0, 1130.0, 132.0, draw::linear(180.0, &[(0.0, "#101216"), (0.45, "#151b25"), (0.8, "#23364f"), (1.0, "#5a88c0")]));
        let glow = b.add(front, parts::emit(glow, "#5c8fd0", 0.45, 30.0));
        motion::add(b, glow, motion::pulse(0.25, 0.8));
        brushed(b, front, (0.0, 141.0, 1130.0, 132.0), 0xb13e, 0.035);
        // The ridge: a dark pyramid, its edges lit.
        let (ax, ay, bl, br, by) = (565.0, 144.0, 295.0, 835.0, 273.0);
        let ridge = format!("M {ax} {ay} L {br} {by} L {bl} {by} Z");
        b.add(front, draw::path("ridge", 0.0, 0.0, 1130.0, 379.0, ridge, draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#121315")])));
        let edge = parts::lines_node("ridge edge", &[vec![(bl, by), (ax, ay), (br, by)]], 1.6, "#d5e6ff");
        b.add(front, parts::emit(edge, "#8fbaf0", 0.9, 10.0));
        // The control strip, with chrome end bars.
        let strip = draw::rect("strip", 0.0, 273.0, 1130.0, 106.0, draw::linear(180.0, &[(0.0, "#1e1f22"), (1.0, "#141517")]));
        b.add(front, draw::with_effects(strip, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 1.0)]));
        specks(b, front, (0.0, 273.0, 1130.0, 106.0), 0x57a1, 40.0, 0.03, 0.08);
        for x in [26.0, 1085.0] {
            b.add(front, draw::rect("end bar", x, 273.0, 19.0, 106.0, draw::linear(90.0, &[(0.0, "#6b6d71"), (0.4, "#d8dadd"), (0.6, "#9a9ca0"), (1.0, "#55575b")])));
        }
        // Controls, in root coordinates (the strip's middle at y 393).
        let ink = Ink::new(8.2, "#e9eaec").weight(700.0).track(0.12);
        let cy = 393.0;
        jack_and_cable(b, root, (138.0, cy), 13.0, (w, h), "#1c1d20", false);
        head_toggle(b, root, "Boost", &a("boost"), (190.0, cy), 16.0, Some((362.0, ink)));
        head_toggle(b, root, "Voice", &a("voice"), (243.0, cy), 16.0, Some((362.0, ink)));
        let style = super::knob("flat", 38.0, "#d8d9dc", &["color=#17181b", "ticks=lines", "skirt=none", "cap_r=0.92", "height=0.55", "edge=0.06", "rough=0.35", "coat=0.3", "pointer=#b9bbbf", "index=line", "index_w=0.07", "index_in=0.62", "index_out=0.96"]);
        for (i, (addr, name, _)) in TONE_STACK.iter().enumerate() {
            let x = 323.0 + 92.3 * i as f64;
            rk::knob_lt(b, root, &style, &a(addr), name, (x, cy + 2.0), 355.0, ink);
        }
        // Power, with its three pilot dots.
        head_toggle(b, root, "Power", &a("power"), (1027.0, cy), 16.0, Some((362.0, ink)));
        for (dx, dy) in [(0.0, -4.0), (-4.0, 3.0), (4.0, 3.0)] {
            rk::led(b, root, (1002.0 + dx, cy + dy), 4.2, "#f4f7ff", true);
        }
    });
    let mut params = vec![rk::toggle(&a("boost"), "Boost", false), rk::toggle(&a("voice"), "Voice", false)];
    params.extend(tone_params(ns));
    params.push(rk::toggle(&a("power"), "Power", true).featured());
    Face { file: "06-amp-head-ridge-clean", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 07 · Mech Rhythm ─────────────────────────────────────────────────────

/// Mech Rhythm — Head, 1200 × 551. See the module docs.
#[allow(clippy::too_many_lines)]
fn mech_rhythm() -> Face {
    let ns = "dfrhythm";
    let a = |s: &str| format!("{ns}/{s}");
    let (w, h) = (1200.0, 551.0);
    const ORANGE: &str = "#ff7a2e";
    let scene = canvas("Mech Rhythm", |b, root| {
        parts::fit_root(b, root, w, h);
        // The handle: a flat bent strap.
        let hd = "M 405 30 L 420 12 Q 426 4 438 4 L 545 4 L 556 10 L 644 10 L 655 4 L 762 4 Q 774 4 780 12 L 795 30 L 783 30 L 770 16 L 660 16 L 649 22 L 551 22 L 540 16 L 430 16 L 417 30 Z";
        b.add(root, draw::with_effects(draw::stroked(draw::path("handle", 0.0, 0.0, w, h, hd.into(), draw::linear(180.0, &[(0.0, "#fbfbfc"), (1.0, "#b9bcc2")])), "#9ea2a9", 0.8), vec![draw::drop_shadow("#00000040", 0.0, 2.0, 3.0)]));
        foot(b, root, (90.0, 522.0, 69.0, 27.0), false);
        foot(b, root, (1032.0, 522.0, 72.0, 27.0), false);
        let body = draw::rounded(draw::rect("cabinet", 3.0, 30.0, 1194.0, 495.0, draw::linear(180.0, &[(0.0, "#f1f2f3"), (0.5, "#e2e3e5"), (1.0, "#cfd1d4")])), 26.0);
        b.add(root, draw::stroked(draw::with_effects(body, vec![draw::drop_shadow("#00000040", 0.0, 8.0, 16.0), draw::inner_shadow("#ffffffb0", 0.0, 2.0, 1.0)]), "#a9acb2", 1.0));
        // The inset face.
        let face = draw::rect("face", 29.0, 57.0, 1141.0, 444.0, draw::linear(180.0, &[(0.0, "#dcdde0"), (1.0, "#d2d4d7")]));
        b.add(root, draw::with_effects(draw::rounded(face, 8.0), vec![draw::inner_shadow("#0000002e", 0.0, 2.0, 5.0)]));
        specks(b, root, (29.0, 57.0, 1141.0, 444.0), 0x3ec4, 45.0, 0.12, 0.05);
        // The dark cavities at the ends of the strip's shelf.
        for (x0, x1) in [(29.0, 140.0), (1050.0, 1170.0)] {
            b.add(root, draw::rect("cavity", x0, 327.0, x1 - x0, 66.0, draw::linear(180.0, &[(0.0, "#2b2d31"), (1.0, "#15161a")])));
        }
        // The mechanism window, lit orange from inside.
        let win = "M 542 243 L 657 243 L 672 262 L 798 276 L 849 363 L 351 363 L 402 276 L 527 262 Z";
        let inside = draw::path("mechanism", 0.0, 0.0, w, h, win.into(), draw::radial_at(0.5, 0.75, 0.7, &[(0.0, "#ffb07a"), (0.35, ORANGE), (0.75, "#9a3b12"), (1.0, "#3a1a0c")]));
        let lit = b.add(root, parts::emit(inside, ORANGE, 0.7, 40.0));
        motion::add(b, lit, motion::flicker(0.08));
        // The machined housings inside, the light spilling round their edges.
        let steel = |a: &str, z: &str| draw::linear(180.0, &[(0.0, a), (1.0, z)]);
        for (pts, a0, a1) in [
            ("M 410 282 L 520 270 L 540 300 L 540 363 L 372 363 Z", "#8d9096", "#4d5056"),
            ("M 790 282 L 680 270 L 660 300 L 660 363 L 828 363 Z", "#8d9096", "#4d5056"),
            ("M 562 243 L 638 243 L 646 300 L 554 300 Z", "#a9acb1", "#6d7076"),
            ("M 470 350 L 730 350 L 700 363 L 500 363 Z", "#5d6066", "#34363a"),
        ] {
            let n = draw::path("housing", 0.0, 0.0, w, h, pts.into(), steel(a0, a1));
            b.add(root, draw::with_effects(n, vec![draw::drop_shadow("#ff7a2e99", 0.0, 0.0, 6.0)]));
        }
        for (x0, x1) in [(360.0, 520.0), (680.0, 840.0)] {
            rk::line(b, root, &[(x0, 292.0), (x1, 292.0)], 1.0, "#ffb27a");
        }
        for bar in ["M 506 290 Q 520 266 566 262", "M 694 290 Q 680 266 634 262"] {
            let mut n = draw::path("strut", 0.0, 0.0, w, h, bar.into(), draw::solid("#dcdde0"));
            n.fills = Vec::new();
            n.stroke = document_proto::Stroke { paints: vec![draw::solid("#dcdde0")], weight: 5.0, cap: document_proto::StrokeCap::Round, ..document_proto::Stroke::default() };
            b.add(root, n);
        }
        // The top notch, lit the same.
        let notch = "M 480 57 L 717 57 L 690 111 L 507 111 Z";
        let n = draw::path("notch", 0.0, 0.0, w, h, notch.into(), draw::radial_at(0.5, 0.2, 0.8, &[(0.0, "#ffa36a"), (0.5, "#c9571e"), (1.0, "#4a200d")]));
        b.add(root, parts::emit(n, ORANGE, 0.5, 20.0));
        for x in [578.0, 618.0] {
            b.add(root, draw::circle("pivot", x, 80.0, 9.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#e8e9eb"), (1.0, "#7d8086")])));
            b.add(root, draw::circle("pivot pin", x, 80.0, 3.0, draw::solid("#3a3c40")));
        }
        rk::line(b, root, &[(598.0, 111.0), (598.0, 243.0)], 1.2, "#e88a4e");
        // The belt, its pulleys, the turbine.
        b.add(root, draw::rounded(draw::rect("belt", 384.0, 300.0, 426.0, 48.0, draw::linear(180.0, &[(0.0, "#c7c9cd"), (0.5, "#9c9fa5"), (1.0, "#6d7076")])), 24.0));
        for x in [410.0, 786.0] {
            b.add(root, draw::circle("pulley", x, 324.0, 21.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#e2e3e6"), (1.0, "#8b8e94")])));
            b.add(root, draw::circle("pulley hub", x, 324.0, 6.0, draw::solid("#5a5d63")));
        }
        for (x, rot) in [(500.0, 35.0), (700.0, -35.0)] {
            b.add(root, draw::rotated(draw::rounded(draw::rect("link", x - 22.0, 316.0, 44.0, 16.0, draw::linear(180.0, &[(0.0, "#b7b9be"), (1.0, "#6c6f75")])), 8.0), rot));
        }
        b.add(root, draw::with_effects(draw::circle("turbine ring", 597.0, 324.0, 44.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f0f1f3"), (1.0, "#8f9298")])), vec![draw::drop_shadow("#00000059", 0.0, 2.0, 4.0)]));
        for i in 0..12 {
            let (x, y) = draw::polar(597.0, 324.0, 39.0, f64::from(i) * 30.0);
            b.add(root, draw::circle("bolt", x, y, 2.2, draw::solid("#55585e")));
        }
        b.add(root, draw::circle("turbine well", 597.0, 324.0, 32.0, draw::solid("#1a1b1e")));
        let blades: String = (0..28)
            .map(|i| {
                let a0 = f64::from(i) * 360.0 / 28.0;
                let (p0, p1) = (draw::polar(597.0, 324.0, 14.0, a0), draw::polar(597.0, 324.0, 30.0, a0 + 9.0));
                let (p2, p3) = (draw::polar(597.0, 324.0, 30.0, a0 + 13.0), draw::polar(597.0, 324.0, 14.0, a0 + 5.0));
                format!("{} ", draw::polygon(&[p0, p1, p2, p3]))
            })
            .collect();
        let fan = b.add(root, draw::path("turbine blades", 0.0, 0.0, w, h, blades, draw::solid("#b9bcc2")));
        motion::add(b, fan, motion::spin("0.15"));
        b.add(root, draw::circle("turbine hub", 597.0, 324.0, 12.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f4f5f6"), (1.0, "#80838a")])));
        // The engraved panel lines and the screws.
        let lines = [
            vec![(29.0, 144.0), (91.0, 144.0), (123.0, 84.0), (243.0, 84.0), (258.0, 57.0)],
            vec![(1170.0, 144.0), (1110.0, 144.0), (1078.0, 84.0), (957.0, 84.0), (942.0, 57.0)],
            vec![(111.0, 363.0), (1089.0, 363.0)],
            vec![(29.0, 363.0), (72.0, 327.0)],
            vec![(1170.0, 363.0), (1128.0, 327.0)],
        ];
        for l in &lines {
            rk::line(b, root, l, 1.1, "#9fa3aa");
        }
        for (x, y) in [(50.0, 74.0), (220.0, 74.0), (978.0, 74.0), (1148.0, 74.0), (70.0, 134.0), (1128.0, 134.0), (70.0, 302.0), (1128.0, 302.0), (70.0, 432.0), (1128.0, 432.0), (48.0, 482.0), (1147.0, 482.0), (520.0, 257.0), (674.0, 257.0)] {
            screw(b, root, (x, y), 4.8, true);
        }
        // The shelf the controls sit on.
        let shelf = draw::rect("strip", 111.0, 363.0, 978.0, 138.0, draw::linear(180.0, &[(0.0, "#e6e7e9"), (1.0, "#d8dadd")]));
        b.add(root, draw::with_effects(shelf, vec![draw::inner_shadow("#ffffffa0", 0.0, 1.5, 0.0), draw::drop_shadow("#00000026", 0.0, -1.0, 2.0)]));
        // Controls.
        let ink = Ink::new(8.6, "#3b3e44").weight(700.0).track(0.12);
        let (cy, ly) = (453.0, 412.0);
        jack_and_cable(b, root, (131.0, cy), 13.0, (w, h), "#8d9096", true);
        rk::print(b, root, "Input", (131.0, ly), ink);
        rk::line(b, root, &[(131.0, 419.0), (131.0, 428.0)], 1.2, ORANGE);
        head_toggle(b, root, "Voice", &a("voice"), (200.0, cy), 16.0, Some((ly, ink)));
        rk::line(b, root, &[(200.0, 419.0), (200.0, 428.0)], 1.2, ORANGE);
        let style = super::knob("stepped_cream", 50.0, "#3b3e44", &["color=#f2f2f1", "pointer=#ef7a2c", "ticks=none", "shadow=0.8", "rough=0.4", "height=0.3", "step_r=0.66", "step_h=0.05", "index_w=0.07", "index_in=0.84", "index_out=0.84", "fallback=domed"]);
        for (i, (addr, name, _)) in TONE_STACK.iter().enumerate() {
            rk::knob_lt(b, root, &style, &a(addr), name, (287.0 + 103.5 * i as f64, cy), ly, ink);
        }
        let power = lit_rocker(b, root, &a("power"), (1078.0, cy - 3.0), (19.0, 46.0));
        rk::label(b, power, "Power", (1078.0, ly), ink);
    });
    let mut params = vec![rk::toggle(&a("voice"), "Voice", false)];
    params.extend(tone_params(ns));
    params.push(rk::toggle(&a("power"), "Power", true).featured());
    Face { file: "07-amp-head-mech-rhythm", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 08 · Slat Lead ───────────────────────────────────────────────────────

/// A valve seen through the cage: its glass, the plate, the glowing
/// heater; the heater flickers.
fn valve(b: &mut Builder<'_>, parent: NodeId, (cx, top, bottom): (f64, f64, f64), wd: f64) {
    let glass = draw::rounded(draw::rect("valve glass", cx - wd / 2.0, top, wd, bottom - top, draw::linear(90.0, &[(0.0, "#ffffff40"), (0.12, "#5a4a4088"), (0.35, "#ffffff2a"), (0.7, "#2a201a90"), (0.9, "#ffffff38"), (1.0, "#ffffff10")])), wd / 2.0);
    b.add(parent, glass);
    b.add(parent, draw::rect("valve plate", cx - wd * 0.28, top + (bottom - top) * 0.22, wd * 0.56, (bottom - top) * 0.5, draw::linear(90.0, &[(0.0, "#3b3230"), (0.5, "#7d6a60"), (1.0, "#3b3230")])));
    let heat = draw::rounded(draw::rect("heater", cx - wd * 0.12, top + (bottom - top) * 0.28, wd * 0.24, (bottom - top) * 0.42, draw::linear(90.0, &[(0.0, "#c24a12"), (0.5, "#ffc38a"), (1.0, "#c24a12")])), 2.0);
    let heat = b.add(parent, parts::emit(heat, "#ff7a2e", 1.0, wd * 1.2));
    motion::add(b, heat, motion::flicker(0.12));
}

/// Slat Lead — Head, 1200 × 595. See the module docs.
#[allow(clippy::too_many_lines)]
fn slat_lead() -> Face {
    let ns = "dflead";
    let a = |s: &str| format!("{ns}/{s}");
    let (w, h) = (1200.0, 595.0);
    let scene = canvas("Slat Lead", |b, root| {
        parts::fit_root(b, root, w, h);
        // Handle: a round bar in an arch.
        let hd = "M 406 43 Q 418 6 470 4 L 730 4 Q 782 6 794 43 L 784 43 Q 772 16 730 14 L 470 14 Q 428 16 416 43 Z";
        b.add(root, draw::with_effects(draw::path("handle", 0.0, 0.0, w, h, hd.into(), draw::linear(180.0, &[(0.0, "#6a6b6e"), (1.0, "#2a2b2d")])), vec![draw::drop_shadow("#00000066", 0.0, 2.0, 3.0)]));
        foot(b, root, (141.0, 559.0, 66.0, 30.0), false);
        foot(b, root, (993.0, 559.0, 66.0, 30.0), false);
        let body = draw::rounded(draw::rect("cabinet", 4.0, 43.0, 1192.0, 518.0, draw::linear(180.0, &[(0.0, "#46474a"), (1.0, "#303134")])), 18.0);
        b.add(root, draw::stroked(draw::with_effects(body, vec![draw::drop_shadow("#00000073", 0.0, 8.0, 16.0), draw::inner_shadow("#ffffff26", 0.0, 2.0, 1.0)]), "#1a1b1d", 1.5));
        specks(b, root, (8.0, 46.0, 1184.0, 512.0), 0x1ead, 30.0, 0.05, 0.12);
        // The cage: a dark cavity with valves, the slats over it.
        let cage = parts::well(b, root, "cage", (21.0, 58.0, 1159.0, 377.0), draw::solid("#3a3b3e"), 4.0);
        specks(b, cage, (0.0, 0.0, 1159.0, 377.0), 0x51a7, 30.0, 0.05, 0.12);
        let n = 23;
        let (x0, pitch, sw, sy0, sy1) = (35.0, 49.55, 17.0, 35.0, 338.0);
        // The cavity behind the slots.
        b.add(cage, draw::rect("cavity", x0 - 4.0, sy0, pitch * f64::from(n) - 20.0, sy1 - sy0, draw::linear(180.0, &[(0.0, "#070707"), (0.5, "#1a120e"), (1.0, "#070707")])));
        // Valves: pairs of power tubes and small preamp tubes.
        for (cx, t, bt, wd) in [(263.0, 142.0, 318.0, 34.0), (332.0, 110.0, 318.0, 34.0), (447.0, 150.0, 318.0, 30.0), (505.0, 108.0, 318.0, 34.0), (660.0, 108.0, 318.0, 34.0), (716.0, 150.0, 318.0, 30.0), (842.0, 110.0, 318.0, 34.0), (900.0, 142.0, 318.0, 34.0)] {
            valve(b, cage, (cx, t, bt), wd);
        }
        // The bars between the slots (drawn over the cavity).
        let mut bars = String::new();
        let mut x = 0.0;
        for i in 0..=n {
            let slot_x = x0 + pitch * f64::from(i);
            let end = if i < n { slot_x - sw / 2.0 } else { 1159.0 };
            bars.push_str(&format!("M {x:.1} 0 L {end:.1} 0 L {end:.1} 377 L {x:.1} 377 Z "));
            x = slot_x + sw / 2.0;
        }
        bars.push_str(&format!("M 0 0 L 1159 0 L 1159 {sy0} L 0 {sy0} Z M 0 {sy1} L 1159 {sy1} L 1159 377 L 0 377 Z"));
        let bars = draw::path("slats", 0.0, 0.0, 1159.0, 377.0, bars, draw::linear(90.0, &[(0.0, "#3d3e41"), (0.5, "#454649"), (1.0, "#3a3b3e")]));
        b.add(cage, draw::with_effects(bars, vec![draw::drop_shadow("#000000a0", 0.0, 0.0, 3.0)]));
        specks(b, cage, (0.0, 0.0, 1159.0, 377.0), 0x51a8, 36.0, 0.04, 0.08);
        // Each slot's rounded, bevelled edge.
        for i in 0..n {
            let cx = x0 + pitch * f64::from(i);
            let slot = draw::rounded(draw::rect("slot edge", cx - sw / 2.0, sy0 - 12.0, sw, sy1 - sy0 + 24.0, draw::solid("#00000000")), sw / 2.0);
            b.add(cage, draw::stroked(slot, "#28292b", 1.4));
        }
        for (x, y) in [(15.0, 13.0), (1144.0, 13.0), (15.0, 364.0), (1144.0, 364.0)] {
            screw(b, cage, (x, y), 5.0, false);
        }
        // The control strip.
        let strip = parts::well(b, root, "strip", (21.0, 439.0, 1159.0, 108.0), draw::linear(180.0, &[(0.0, "#3d3e41"), (1.0, "#343538")]), 4.0);
        specks(b, strip, (0.0, 0.0, 1159.0, 108.0), 0x57b2, 30.0, 0.05, 0.1);
        for (x, y) in [(14.0, 12.0), (1145.0, 12.0), (14.0, 96.0), (1145.0, 96.0)] {
            screw(b, strip, (x, y), 4.2, false);
        }
        let cy = 492.0;
        jack_and_cable(b, root, (69.0, cy), 18.0, (w, h), "#1c1d20", false);
        // Voice: a small red-tipped bat, its dots and legend.
        let voice = rk::mini_toggle(b, root, "Voice", &a("voice"), (132.0, cy - 6.0), 11.0, "black");
        rk::label(b, voice, "Voice", (132.0, 531.0), Ink::new(10.0, "#d4d5d8").weight(400.0).track(0.02).plain());
        b.add(root, draw::circle("dot", 146.0, cy - 20.0, 1.8, draw::solid("#e6e7ea")));
        b.add(root, draw::stroked(draw::circle("dot", 146.0, cy + 10.0, 1.8, draw::solid("#00000000")), "#e6e7ea", 0.8));
        let style = KnobStyle::new("knob.slotted", "black", 74.0, "#d9dadc");
        for (i, (addr, name, short)) in TONE_STACK.iter().enumerate() {
            let p = Place::new(*name).size(style.d).variant("black").legend(*short).keep_case().bind(rk::bind(&a(addr))).touch(TOUCH_MIN);
            let k = parts::put(b, root, "knob.slotted", &p, 214.0 + 128.2 * i as f64, cy);
            turn_legend(b, k, short, style.d);
        }
        // Power: a square orange button with a lit eye.
        power_button(b, root, &a("power"), (1114.0, cy), 58.0);
        rk::print(b, root, "0/I", (1128.0, 531.0), Ink::new(10.0, "#d4d5d8").weight(400.0).track(0.02).plain());
    });
    let mut params = vec![rk::toggle(&a("voice"), "Voice", false)];
    params.extend(tone_params(ns));
    params.push(rk::toggle(&a("power"), "Power", true).featured());
    Face { file: "08-amp-head-slat-lead", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// Re-cut a slotted knob's own legend as lettering turned to read bottom
/// to top along the slot (the renderer doesn't turn a text run), in its
/// turning body, as the control's `Label`.
fn turn_legend(b: &mut Builder<'_>, knob: NodeId, s: &str, d: f64) {
    let Some(ctl) = b.controls.get(&knob) else { return };
    let body = ctl.slots_of(SlotRole::Needle).next().map(NodeId);
    let old = ctl.slots_of(SlotRole::Label).next().map(NodeId);
    let (Some(body), Some(old)) = (body, old) else { return };
    let _ = b.scene.apply(document_proto::Op::Remove { id: old });
    let c = b.scene.node(body).map_or(0.0, |n| n.size.width / 2.0);
    let k = d / 48.0;
    let size = (8.5 * k).max(7.0);
    let id = rk::lettering::letter_turned(b, body, s, (c - 5.0 * k, c), -90.0, rk::Letter::new(size, 450.0, "#d9dadc").track(0.02));
    if let Some(ctl) = b.controls.get_mut(&knob) {
        ctl.slots.retain(|s| s.role != SlotRole::Label);
        ctl.slots.push(surface_proto::Slot { role: SlotRole::Label, node: id.0 });
    }
}

/// A tall orange rocker lit from inside while on (the white head's power).
fn lit_rocker(b: &mut Builder<'_>, parent: NodeId, addr: &str, (cx, cy): (f64, f64), (rw, rh): (f64, f64)) -> NodeId {
    let bw = rw.max(TOUCH_MIN);
    let bh = rh.max(TOUCH_MIN);
    let root = b.add(parent, draw::frame("Power", cx - bw / 2.0, cy - bh / 2.0, bw, bh));
    let (x, y) = ((bw - rw) / 2.0, (bh - rh) / 2.0);
    let bezel = draw::rounded(draw::rect("bezel", x - 2.0, y - 2.0, rw + 4.0, rh + 4.0, draw::solid("#2a2b2e")), 4.0);
    b.add(root, draw::with_effects(bezel, vec![draw::drop_shadow("#00000066", 0.0, 2.0, 3.0)]));
    b.add(root, draw::rounded(draw::rect("rocker", x, y, rw, rh, draw::linear(180.0, &[(0.0, "#7a2a10"), (0.3, "#c24a1a"), (1.0, "#e0612a")])), 3.0));
    let glow = draw::rounded(draw::rect("lamp", x + 1.5, y + rh * 0.35, rw - 3.0, rh * 0.6, draw::radial_at(0.5, 0.6, 0.7, &[(0.0, "#ffe2c8"), (0.4, "#ff9a5a"), (1.0, "#e0612a")])), 3.0);
    let glow = b.add(root, parts::emit(glow, "#ff8a45", 0.8, rw));
    b.add(root, draw::rounded(draw::rect("rocker top", x + 2.0, y + 2.0, rw - 4.0, rh * 0.28, draw::linear(180.0, &[(0.0, "#3a1a0c"), (1.0, "#8a3514")])), 2.0));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, glow.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A square orange power button whose centre lights while on.
fn power_button(b: &mut Builder<'_>, parent: NodeId, addr: &str, (cx, cy): (f64, f64), s: f64) -> NodeId {
    let root = b.add(parent, draw::frame("Power", cx - s / 2.0, cy - s / 2.0, s, s));
    let cap = draw::rounded(draw::rect("cap", 0.0, 0.0, s, s, draw::linear(180.0, &[(0.0, "#ff8a55"), (0.5, "#e8562a"), (1.0, "#b8401c")])), s * 0.14);
    b.add(root, draw::with_effects(cap, vec![draw::drop_shadow("#00000080", 0.0, 3.0, 5.0), draw::inner_shadow("#ffffff55", 0.0, 1.5, 1.0)]));
    b.add(root, draw::circle("dish", s / 2.0, s / 2.0, s * 0.36, draw::radial_at(0.55, 0.6, 0.7, &[(0.0, "#f2703d"), (1.0, "#c44a20")])));
    let eye = draw::circle("eye", s / 2.0, s / 2.0, s * 0.1, draw::radial(&[(0.0, "#ffffff"), (0.5, "#ffd0b0"), (1.0, "#ff8a55")]));
    let eye = b.add(root, parts::emit(eye, "#ffb08a", 0.9, s * 0.4));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, eye.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

