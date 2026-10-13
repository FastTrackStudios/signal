//! The spring: an amp's reverb tank, open on a green chassis with walnut
//! cheeks.
//!
//! Up top, the tank as you'd see it with the pan's cover off: a galvanised
//! tray on its rubber grommets, the springs (two, or however many the
//! SPRINGS selector picks) stretched between the input transducer and the
//! pickup, each end a black can with its copper winding showing. Below,
//! the amp's row: cream chicken-heads for DWELL, TONE and MIX — the three
//! a spring is set by — black bakelite for the rest, the springs selector,
//! the IN bat and a jewel pilot.
//!
//! The springs move: each is a struck resonator (motion) that the input
//! level rings — it bounces and settles over the time DWELL and DAMPING
//! give it, at a pitch TENSION sets, and rings not at all with the reverb
//! switched out.

use document_proto::motion::{Dynamics, Motion, Num, Target};
use document_proto::{Node, NodeId, Stroke, StrokeCap, StrokeJoin};
use surface_kit::{Place, draw, motion};
use surface_widgets::Builder;

use super::ui::{self, Dial, bind};
use crate::animate;
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 350.0;
const INK: &str = "#ece3c6";
const DIM: &str = "#a7b29c";

/// One spring seen from the side, `n` loops of a helix between `x0..x1`
/// at `y`, `r` tall, back halves dark and front halves bright — and alive:
/// the spring's frame carries two struck resonators, `ring` (the
/// fundamental, bending the coil sideways) and `ripple` (the second mode,
/// running along it). Rises of the input level (and, weaker, the output's
/// ring-back) strike them — harder with more DWELL, not at all with the
/// reverb off; TENSION tunes them and DWELL / DAMPING set how long they
/// ring. The three strokes are redrawn from the `coil` shape provider.
#[allow(clippy::too_many_arguments)]
fn spring(b: &mut Builder<'_>, tank: NodeId, a: &Springs, i: usize, (w, h): (f64, f64), x0: f64, x1: f64, y: f64, r: f64) {
    let frame = b.add(tank, draw::frame(&format!("Spring {}", i + 1), 0.0, 0.0, w, h));
    // Each spring a little apart in pitch, as real ones are.
    let k = 1.0 + 0.13 * i as f64;
    let freq = format!("{:.3} + {:.3}*norm({})", 2.2 * k, 4.5 * k, a.tension);
    let decay = format!("(0.35 + 2.8*norm({})) * (1.15 - norm({}))", a.dwell, a.damping);
    let excite = format!("(or(level({}), 0) + 0.5*or(level({}), 0)) * (0.35 + norm({})) * norm({})", a.input, a.output, a.dwell, a.on);
    let excite = excite.as_str();
    let resonator = |name: &str, freq: String, decay: String, gain: f64| {
        Motion::new(Target::None).named(name).dynamics(Dynamics::Oscillator {
            freq: Num::Expr(freq),
            decay: Some(Num::Expr(decay)),
            damping: None,
            excite: excite.into(),
            gain: Num::Value(gain),
        })
    };
    motion::add(b, frame, resonator("ring", freq.clone(), decay.clone(), 1.0));
    motion::add(b, frame, resonator("ripple", format!("2.6 * ({freq})"), format!("0.5 * {decay}"), 0.6));

    // Its shadow on the floor, then the coil.
    b.add(frame, draw::with_opacity(draw::rounded(draw::rect("shadow", x0, y + 7.0, x1 - x0, 5.0, draw::solid("#00000040")), 2.5), 1.0));
    let fixed = [("x0", x0), ("x1", x1), ("y", y), ("r", r), ("pitch", 5.5)];
    let ripple = format!("motion(ripple) * (1.5 + 5*norm({}))", a.drip);
    let live = [("bend", "motion(ring) * 12"), ("ripple", ripple.as_str()), ("squeeze", "motion(ripple) * 2")];
    let rest = [("bend", 0.0), ("ripple", 0.0), ("squeeze", 0.0)];
    let stroke = |mut n: Node, weight: f64, color: &str| {
        n.stroke = Stroke { paints: vec![draw::solid(color)], weight, cap: StrokeCap::Round, join: StrokeJoin::Round, ..Stroke::default() };
        n
    };
    let part = |name: &str, part: &str| animate::shape_node(name, "coil", part, (w, h), &fixed, &live, &rest);
    b.add(frame, stroke(part("coil back", "back"), 1.2, "#5e6368"));
    // The front halves: steel, with the light running along their tops.
    b.add(frame, stroke(part("coil front", "front"), 1.6, "#9aa0a5"));
    b.add(frame, stroke(part("coil light", "light"), 0.6, "#f2f4f5"));
}

/// A transducer can with its winding, centred at (`cx`, `cy`).
fn transducer(b: &mut Builder<'_>, tank: NodeId, cx: f64, cy: f64, label: &str) {
    let can = draw::rounded(draw::rect("transducer", cx - 26.0, cy - 30.0, 52.0, 60.0, draw::linear(90.0, &[(0.0, "#1b1c1e"), (0.45, "#3a3c40"), (1.0, "#101112")])), 4.0);
    b.add(tank, draw::with_effects(can, vec![draw::drop_shadow("#000000a0", 0.0, 3.0, 5.0)]));
    let wind = draw::rounded(draw::rect("winding", cx - 12.0, cy - 18.0, 24.0, 36.0, draw::linear(90.0, &[(0.0, "#7a3f16"), (0.3, "#e0a266"), (0.55, "#b8753a"), (1.0, "#6a3410")])), 3.0);
    b.add(tank, wind);
    let turns: Vec<Vec<(f64, f64)>> = (0..16)
        .map(|i| {
            let y = cy - 16.7 + f64::from(i) * 2.2;
            vec![(cx - 12.0, y), (cx + 12.0, y)]
        })
        .collect();
    b.add(tank, ui::bars("turns", &turns, 0.6, "#4a220a88"));
    ui::silk(b, tank, label, cx, cy + 40.0, 6.0, "#3b3f43", 800.0);
}

/// What the springs ring from: the params and levels a tank reads.
pub struct Springs {
    pub dwell: &'static str,
    pub damping: &'static str,
    pub tension: &'static str,
    pub drip: &'static str,
    pub on: &'static str,
    pub input: &'static str,
    pub output: &'static str,
}

impl Springs {
    /// The spring reverb face's own.
    pub const FACE: Self = Self {
        dwell: "spring/dwell",
        damping: "spring/damping",
        tension: "spring/tension",
        drip: "spring/drip",
        on: "spring/on",
        input: "spring/in",
        output: "spring/out",
    };
}

fn tank_art(b: &mut Builder<'_>, tank: NodeId, w: f64, h: f64, springs: usize) {
    tank_art_with(b, tank, &Springs::FACE, w, h, springs, "Type 4  ·  Long decay  ·  Horizontal");
}

/// The open tank, `w × h`, its springs ringing from `a`.
pub fn tank_art_with(b: &mut Builder<'_>, tank: NodeId, a: &Springs, w: f64, h: f64, springs: usize, caption: &str) {
    // The pan: galvanised steel with a rolled lip.
    let pan = draw::rounded(draw::rect("pan", 0.0, 0.0, w, h, draw::linear(170.0, &[(0.0, "#d4d7d8"), (0.25, "#b0b5b8"), (0.5, "#c9cdcf"), (0.75, "#a3a8ac"), (1.0, "#bfc3c6")])), 6.0);
    b.add(tank, pan);
    // Spangle: the zinc crystals of galvanising, faint.
    let mut spangle = String::new();
    for i in 0..90 {
        let fi = f64::from(i);
        let (x, y) = (super::jitter(fi) * w, super::jitter(fi + 91.3) * h);
        spangle.push_str(&draw::regular(x, y, 4.0 + super::jitter(fi + 7.7) * 9.0, 6, super::jitter(fi + 3.1) * 60.0));
        spangle.push(' ');
    }
    b.add(tank, ui::area("spangle", spangle, w, h, draw::solid("#ffffff14")));
    let lip = draw::stroked(draw::rounded(draw::frame("lip", 3.0, 3.0, w - 6.0, h - 6.0), 5.0), "#eef0f1", 2.0);
    b.add(tank, draw::with_effects(lip, vec![draw::drop_shadow("#00000040", 0.0, 1.0, 1.0)]));
    let floor = draw::rounded(draw::rect("floor", 14.0, 14.0, w - 28.0, h - 28.0, draw::linear(180.0, &[(0.0, "#8c9295"), (1.0, "#a9aeb1")])), 3.0);
    b.add(tank, draw::with_effects(floor, vec![draw::inner_shadow("#00000070", 0.0, 3.0, 6.0)]));
    // Grommets.
    for (x, y) in [(9.0, 9.0), (w - 9.0, 9.0), (9.0, h - 9.0), (w - 9.0, h - 9.0)] {
        b.add(tank, draw::circle("grommet", x, y, 6.5, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3a3a3c"), (1.0, "#0b0b0c")])));
        b.add(tank, draw::circle("eyelet", x, y, 2.4, draw::solid("#c9ccce")));
    }
    // Springs between the transducers.
    let (tx0, tx1) = (64.0, w - 64.0);
    let gap = (h - 60.0) / springs as f64;
    for i in 0..springs {
        let y = 30.0 + gap * (i as f64 + 0.5);
        // Hooks from the transducers.
        ui::line(b, tank, "hook", &[(tx0 + 24.0, y), (tx0 + 46.0, y)], 1.6, "#e2e5e7");
        ui::line(b, tank, "hook", &[(tx1 - 46.0, y), (tx1 - 24.0, y)], 1.6, "#e2e5e7");
        // The spring: shadow and coil, ringing.
        spring(b, tank, a, i, (w, h), tx0 + 44.0, tx1 - 44.0, y, (gap * 0.3).min(13.0));
    }
    transducer(b, tank, tx0, h / 2.0, "Input");
    transducer(b, tank, tx1, h / 2.0, "Output");
    ui::silk(b, tank, caption, w / 2.0, h - 22.0, 6.0, "#4a4f54", 800.0);
}

pub fn face() -> Face {
    let scene = canvas("Spring Reverb", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.desktop", "green", W, H, "wood");

        // ── The tank ───────────────────────────────────────────────────
        let (tx, ty, tw, th) = (64.0, 44.0, 832.0, 150.0);
        ui::plain(b, panel, "Spring Reverb", 146.0, 26.0, 14.0, INK, 800.0);
        ui::silk_right(b, panel, "Two-spring tank · tube driven", tx + tw, 26.0, 6.5, DIM, 700.0);
        let mut tank = draw::frame("Tank", tx, ty, tw, th);
        tank.clips_content = true;
        tank.corner_radii = document_proto::CornerRadii::uniform(6.0);
        tank.effects = vec![draw::drop_shadow("#000000b0", 0.0, 4.0, 10.0)];
        let tank = b.add(panel, tank);
        tank_art(b, tank, tw, th, 2);

        // ── The amp's row ──────────────────────────────────────────────
        let row = 262.0;
        let legend_y = row + 56.0;
        let cream = Dial::new("knob.chicken_head", "cream", 56.0, INK, DIM).scale(&["0", "", "", "", "", "10"]).legend_at(legend_y);
        let black = Dial::new("knob.bakelite", "black", 40.0, INK, DIM).legend_at(legend_y);
        ui::dial(b, panel, "spring/dwell", "Dwell", 110.0, row, &cream);
        ui::dial(b, panel, "spring/tone", "Tone", 210.0, row, &cream);
        ui::dial(b, panel, "spring/drip", "Drip", 302.0, row, &black);
        ui::dial(b, panel, "spring/tension", "Tension", 380.0, row, &black);
        ui::dial(b, panel, "spring/damping", "Damp", 458.0, row, &black);
        ui::dial(b, panel, "spring/warble", "Warble", 536.0, row, &black);
        ui::dial(b, panel, "spring/mix", "Mix", 632.0, row, &cream);
        // The springs selector: three detents.
        let sel = Dial::new("switch.rotary", "black", 44.0, INK, DIM).options(&["1", "2", "3"]).sweep(120.0).legend_at(legend_y);
        ui::dial(b, panel, "spring/springs", "Springs", 730.0, row, &sel);

        // IN bat and the pilot.
        let bat = Place::new("In").size(42.0).variant("chrome").legend("").bind(bind("spring/on")).options(&["On", "Off"]);
        let t = ui::put(b, panel, "switch.toggle_bat", &bat, 812.0, row - 4.0);
        ui::strip_legend(b, t);
        ui::silk(b, panel, "Reverb", 812.0, legend_y, 9.0, INK, 800.0);
        ui::lamp(b, panel, 872.0, row - 4.0, 13.0, "#e8452c", true);
        ui::silk(b, panel, "Pilot", 872.0, legend_y, 9.0, INK, 800.0);
    });

    let params = vec![
        super::pct("spring/dwell", "Dwell", 55.0).featured(),
        super::pct("spring/tone", "Tone", 50.0).featured(),
        super::pct("spring/drip", "Drip", 40.0),
        super::pct("spring/tension", "Tension", 50.0),
        super::pct("spring/damping", "Damping", 30.0),
        super::pct("spring/warble", "Warble", 15.0),
        super::mix("spring/mix", 35.0),
        ui::choice("spring/springs", "Springs", &["1", "2", "3"], 1).featured(),
        ui::toggle("spring/on", "Reverb", true),
    ];
    Face { file: "05-spring", plugin: "reverb", scene, params, streams: super::levels("spring") }
}

/// The spring reverb for a rig row, on a rig's reverb block: the same
/// green chassis and walnut cheeks, the open tank with two ringing springs
/// across the top, and the three a spring is set by — DWELL (the block's
/// decay), TONE, MIX (its level) — on cream chicken-heads, DAMP on black
/// bakelite, the IN bat and the pilot. `w × h`.
#[must_use]
pub fn rig_face(file: &'static str, plugin: &'static str, w: f64, h: f64) -> Face {
    let a = Springs {
        dwell: "spring/decay",
        damping: "spring/damping",
        tension: "spring/tension",
        drip: "spring/modulation",
        on: "spring/on",
        input: "spring/in",
        output: "spring/out",
    };
    let scene = canvas("Spring Reverb", |b, root| {
        ui::fit_root(b, root, w, h);
        let panel = ui::panel(b, root, "panel.desktop", "green", w, h, "wood");
        let (tx, ty, tw, th) = (46.0, 20.0, w - 92.0, h * 0.46);
        let mut tank = draw::frame("Tank", tx, ty, tw, th);
        tank.clips_content = true;
        tank.corner_radii = document_proto::CornerRadii::uniform(6.0);
        tank.effects = vec![draw::drop_shadow("#000000b0", 0.0, 4.0, 10.0)];
        let tank = b.add(panel, tank);
        tank_art_with(b, tank, &a, tw, th, 2, "");
        let row = ty + th + (h - ty - th) * 0.44;
        let legend_y = h - 18.0;
        let d = ((h - ty - th) * 0.52).min(78.0);
        let cream = Dial::new("knob.chicken_head", "cream", d, INK, DIM).legend_at(legend_y);
        let black = Dial::new("knob.bakelite", "black", d * 0.72, INK, DIM).legend_at(legend_y);
        let x = |f: f64| tx + tw * f;
        ui::dial(b, panel, "spring/decay", "Dwell", x(0.1), row, &cream);
        ui::dial(b, panel, "spring/tone", "Tone", x(0.3), row, &cream);
        ui::dial(b, panel, "spring/damping", "Damp", x(0.48), row, &black);
        ui::dial(b, panel, "spring/level", "Mix", x(0.66), row, &cream);
        let bat = Place::new("In").size(34.0).variant("chrome").legend("").bind(bind("spring/on")).options(&["On", "Off"]);
        let t = ui::put(b, panel, "switch.toggle_bat", &bat, x(0.84), row - 4.0);
        ui::strip_legend(b, t);
        ui::silk(b, panel, "Reverb", x(0.84), legend_y, 9.0, INK, 800.0);
        ui::lamp(b, panel, x(0.96), row - 4.0, 12.0, "#e8452c", true);
    });
    let params = vec![
        ui::param("spring/decay", "Dwell", surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None), 0.35).featured(),
        ui::param("spring/tone", "Tone", surface_proto::Range::linear(-1.0, 1.0, surface_proto::Unit::None), 0.0).featured(),
        ui::param("spring/damping", "Damping", surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None), 0.3),
        ui::param("spring/level", "Mix", surface_proto::Range::linear(-60.0, 12.0, surface_proto::Unit::Db), -16.5).featured(),
        ui::param("spring/tension", "Tension", surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None), 0.5),
        ui::param("spring/modulation", "Drip", surface_proto::Range::linear(0.0, 1.0, surface_proto::Unit::None), 0.2),
        ui::toggle("spring/on", "Reverb", true),
    ];
    Face { file, plugin, scene, params, streams: Vec::new() }
}
