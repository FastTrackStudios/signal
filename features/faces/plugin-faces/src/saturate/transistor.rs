//! The transistor stage, as the box it's best known in: a fuzz pedal.
//!
//! Hammered silver enclosure, black print. Under the name, a scope window
//! with what the circuit is doing, live: at the left its transfer curve —
//! the straight line of the transistor's gain bending into the flat of
//! the rails, steeper with GAIN, its corner harder with EDGE, pushed off
//! centre (one side clipping first) with SAG, the starved dead band round
//! zero widening with GATE; at the right a sine going through it, the
//! ghost of the clean wave behind the clipped one, the tops squaring off
//! as the gain comes up, fizzing with TONE, breaking up at the zero
//! crossings with GATE. Then the three knobs every fuzz has — GAIN, TONE,
//! LEVEL (output) — and the circuit's trims: EDGE, GATE, SAG and MIX. The
//! mode toggle picks the family's two profiles (a clean transistor drive,
//! or the fuzz). One stomp switch with its LED, and the part itself
//! printed beside it — a PNP transistor's symbol.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{Ink as KitInk, Place, draw, motion};
use frame::surface::widgets::Builder;

use crate::delay::art;
use crate::delay::parts::{self, Ink, bind};
use crate::face::{Face, canvas};
use crate::lane::Shape;

const W: f64 = 760.0;
const H: f64 = 520.0;
const INK: &str = "#17181a";
const DIM: &str = "#3d4045";

fn a(s: &str) -> String {
    format!("transistor/{s}")
}

/// A PNP transistor's schematic symbol, printed at (`cx`, `cy`), radius `r`.
fn symbol(b: &mut Builder<'_>, lid: NodeId, (cx, cy): (f64, f64), r: f64, ink: &str) {
    let wt = 2.2;
    // The envelope: a circle, stroked.
    let ring: Vec<(f64, f64)> = (0..=72)
        .map(|i| draw::polar(cx, cy, r, f64::from(i) * 5.0))
        .collect();
    b.add(
        lid,
        parts::stroke_path(
            "envelope",
            (cx - r - wt, cy - r - wt, 2.0 * (r + wt), 2.0 * (r + wt)),
            &[ring],
            wt,
            ink,
            true,
        ),
    );
    // Base: a lead in from the left to the bar.
    let bar_x = cx - r * 0.28;
    let (ex, ey) = (cx + r * 0.42, cy - r * 0.62);
    let (kx, ky) = (cx + r * 0.42, cy + r * 0.62);
    let leads = [
        vec![(cx - r - 16.0, cy), (bar_x, cy)],
        // Collector and emitter: diagonals out to the right, then up and down.
        vec![(bar_x, cy - r * 0.22), (ex, ey), (ex, cy - r - 16.0)],
        vec![(bar_x, cy + r * 0.22), (kx, ky), (kx, cy + r + 16.0)],
    ];
    let mut leads = parts::lines_node("leads", &leads, wt, ink);
    leads.stroke.cap = frame::document::proto::StrokeCap::None;
    b.add(lid, leads);
    let mut bar = parts::lines_node(
        "base bar",
        &[vec![(bar_x, cy - r * 0.5), (bar_x, cy + r * 0.5)]],
        wt * 2.0,
        ink,
    );
    bar.stroke.cap = frame::document::proto::StrokeCap::None;
    b.add(lid, bar);
    // PNP: the emitter's arrow points in, toward the bar.
    let (mx, my) = ((bar_x + ex) / 2.0 - 1.0, (cy - r * 0.22 + ey) / 2.0 + 0.5);
    let (ux, uy) = ((bar_x - ex), (cy - r * 0.22 - ey));
    let len = ux.hypot(uy);
    let (ux, uy) = (ux / len, uy / len);
    let (px, py) = (-uy, ux);
    let s = 7.0;
    let arrow = [
        (mx + ux * s, my + uy * s),
        (
            mx - ux * s * 0.3 + px * s * 0.6,
            my - uy * s * 0.3 + py * s * 0.6,
        ),
        (
            mx - ux * s * 0.3 - px * s * 0.6,
            my - uy * s * 0.3 - py * s * 0.6,
        ),
    ];
    let (ax, ay) = (mx - s, my - s);
    let local: Vec<(f64, f64)> = arrow.iter().map(|(x, y)| (x - ax, y - ay)).collect();
    b.add(
        lid,
        draw::path(
            "emitter arrow",
            ax,
            ay,
            2.0 * s,
            2.0 * s,
            draw::polygon(&local),
            draw::solid(ink),
        ),
    );
    let t = Ink::new(8.0, ink).weight(800.0).track(0.0);
    parts::silk(b, lid, "E", ex + 10.0, cy - r - 10.0, t);
    parts::silk(b, lid, "C", kx + 10.0, cy + r + 10.0, t);
    parts::silk(b, lid, "B", cx - r - 22.0, cy - 10.0, t);
}

/// A knob on the lid: its param, legend, centre x and printed marks.
type KnobAt = (
    &'static str,
    &'static str,
    f64,
    &'static [(f64, &'static str)],
);

/// The scope's phosphor.
const TRACE: &str = "#ffb347";

/// A transfer curve over input −`span`..`span` (output −1..1), drawn in a
/// `w` × `h` box: the clean line of gain `g` bent into the rails — softly
/// (tanh) or with a hard corner (`hard`).
fn transfer(w: f64, h: f64, span: f64, g: f64, hard: bool) -> String {
    let n = 120;
    let mut d = String::new();
    for k in 0..=n {
        let x = -span + 2.0 * span * f64::from(k) / f64::from(n);
        let y = if hard {
            (g * x).clamp(-1.0, 1.0)
        } else {
            (g * x).tanh()
        };
        let (px, py) = (w * (x + span) / (2.0 * span), h / 2.0 - y * h * 0.4);
        d += &format!("{} {px:.2} {py:.2} ", if k == 0 { "M" } else { "L" });
    }
    d
}

/// A sine through a clipper of gain `g` over `cycles` cycles, `w` × `h`.
fn clipped(w: f64, h: f64, cycles: f64, g: f64) -> String {
    let n = 360;
    let mut d = String::new();
    let norm = g.tanh();
    for k in 0..=n {
        let u = f64::from(k) / f64::from(n);
        let x = (u * cycles * std::f64::consts::TAU).sin();
        let y = (g * x).tanh() / norm;
        d += &format!(
            "{} {:.2} {:.2} ",
            if k == 0 { "M" } else { "L" },
            w * u,
            h / 2.0 - y * h * 0.36
        );
    }
    d
}

/// A stroked path node of `d` in a `w` × `h` box.
fn trace(
    name: &str,
    (w, h): (f64, f64),
    d: String,
    color: &str,
    weight: f64,
) -> frame::document::proto::Node {
    draw::stroked(
        draw::path(name, 0.0, 0.0, w, h, d, draw::solid("#00000000")),
        color,
        weight,
    )
}

/// The transfer curve panel in (`x`, `y`, `w`, `h`) of the scope.
fn curve_panel(b: &mut Builder<'_>, scope: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let mut p = draw::frame("Transfer", x, y, w, h);
    p.clips_content = true;
    let p = b.add(scope, p);
    let faint = "#ffb34724";
    b.add(
        p,
        parts::lines_node(
            "axes",
            &[
                vec![(w / 2.0, 4.0), (w / 2.0, h - 4.0)],
                vec![(4.0, h / 2.0), (w - 4.0, h / 2.0)],
            ],
            0.8,
            faint,
        ),
    );
    b.add(
        p,
        parts::lines_node(
            "rails",
            &[
                vec![(0.0, h * 0.1), (w, h * 0.1)],
                vec![(0.0, h * 0.9), (w, h * 0.9)],
            ],
            0.6,
            faint,
        ),
    );
    // The starved band round zero: wider with the gate.
    let band = b.add(
        p,
        draw::rect(
            "gate band",
            w / 2.0 - 14.0,
            0.0,
            28.0,
            h,
            draw::solid("#ff6a3d22"),
        ),
    );
    motion::add(
        b,
        band,
        Motion::new(Target::Opacity)
            .input(format!("min(max(norm({}) - 0.5, 0) * 8, 1)", a("gate")))
            .range(0.0, 1.0),
    );
    motion::add(
        b,
        band,
        Motion::new(Target::StretchX)
            .input(format!("max(norm({}) - 0.5, 0) * 2", a("gate")))
            .range(0.05, 1.0)
            .pivot(0.5, 0.5)
            .dynamics(Dynamics::spring(220.0, 26.0)),
    );
    // The curve, drawn three times the box wide: GAIN squeezes it about
    // the middle (steeper), SAG slides it off centre, EDGE hardens its
    // corner.
    let wide = 3.0 * w;
    let g = b.add(p, draw::frame("curve", -w, 0.0, wide, h));
    let soft = transfer(wide, h, 3.0, 1.2, false);
    let hard = transfer(wide, h, 3.0, 1.2, true);
    let c = b.add(
        g,
        crate::reverb::ui::glowing(
            trace("transfer", (wide, h), soft.clone(), TRACE, 1.8),
            TRACE,
            0.9,
            6.0,
        ),
    );
    motion::add(
        b,
        c,
        Motion::new(Target::Morph {
            shapes: vec![soft, hard],
        })
        .input(format!("norm({})", a("edge")))
        .range(0.0, 1.0),
    );
    motion::add(
        b,
        g,
        Motion::new(Target::StretchX)
            .input(format!("norm({})", a("drive")))
            .range(1.4, 0.25)
            .pivot(0.5, 0.5)
            .dynamics(Dynamics::spring(220.0, 26.0)),
    );
    motion::add(
        b,
        g,
        Motion::new(Target::X)
            .input(format!("norm({}) - 0.5", a("sag")))
            .range(0.0, w * 0.3)
            .dynamics(Dynamics::spring(220.0, 26.0)),
    );
    parts::silk(
        b,
        p,
        "In",
        w - 9.0,
        h / 2.0 - 7.0,
        Ink::new(5.5, "#ffb34799").weight(800.0).track(0.0),
    );
    parts::silk(
        b,
        p,
        "Out",
        w / 2.0 + 12.0,
        9.0,
        Ink::new(5.5, "#ffb34799").weight(800.0).track(0.0),
    );
}

/// The wave panel in (`x`, `y`, `w`, `h`) of the scope: the clean sine as
/// a ghost, the clipped one over it, rolling by while the pedal is on.
fn wave_panel(b: &mut Builder<'_>, scope: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let mut p = draw::frame("Wave", x, y, w, h);
    p.clips_content = true;
    let p = b.add(scope, p);
    let grid: Vec<Vec<(f64, f64)>> = (1..8)
        .map(|i| {
            vec![
                (w * f64::from(i) / 8.0, 4.0),
                (w * f64::from(i) / 8.0, h - 4.0),
            ]
        })
        .collect();
    b.add(p, parts::lines_node("grid", &grid, 0.6, "#ffb34714"));
    b.add(
        p,
        parts::lines_node(
            "zero",
            &[vec![(0.0, h / 2.0), (w, h / 2.0)]],
            0.6,
            "#ffb34730",
        ),
    );
    let cycles = 4.0;
    let period = w / cycles;
    // Two periods more than the window, rolled by one period at a time.
    let long = w + 2.0 * period;
    let lc = cycles + 2.0;
    let roll = b.add(p, draw::frame("roll", -period, 0.0, long, h));
    motion::add(
        b,
        roll,
        Motion::new(Target::X)
            .input(format!("fract(time * 0.6) * param({})", a("on")))
            .range(0.0, period),
    );
    b.add(
        roll,
        trace(
            "clean",
            (long, h),
            clipped(long, h, lc, 0.05),
            "#ffb34740",
            1.2,
        ),
    );
    let shapes: Vec<String> = [0.6, 2.0, 5.0, 16.0]
        .iter()
        .map(|g| clipped(long, h, lc, *g))
        .collect();
    let out = b.add(
        roll,
        crate::reverb::ui::glowing(
            trace("clipped", (long, h), shapes[0].clone(), TRACE, 2.0),
            TRACE,
            0.9,
            7.0,
        ),
    );
    motion::add(
        b,
        out,
        Motion::new(Target::Morph { shapes })
            .input(format!("norm({}) * 3", a("drive")))
            .range(0.0, 1.0),
    );
    // The fizz on the flat tops: brighter with the tone, only when they
    // are flat.
    let mut fizz = Vec::new();
    for k in 0..(lc as u32 * 2) {
        let cx = period * (f64::from(k) / 2.0 + 0.25);
        let top = if k % 2 == 0 {
            h / 2.0 - h * 0.36
        } else {
            h / 2.0 + h * 0.36
        };
        let pts: Vec<(f64, f64)> = (0..=10)
            .map(|j| {
                (
                    cx - period * 0.18 + period * 0.036 * f64::from(j),
                    top + crate::reverb::jitter(f64::from(k * 11 + j)) * 3.0,
                )
            })
            .collect();
        fizz.push(pts);
    }
    let fz = b.add(roll, parts::lines_node("fizz", &fizz, 0.9, "#fff0d0"));
    motion::add(
        b,
        fz,
        Motion::new(Target::Opacity)
            .input(format!(
                "norm({}) * norm({}) * param({})",
                a("tone"),
                a("drive"),
                a("on")
            ))
            .range(0.0, 0.9),
    );
    // The gate's spitting: gaps at the zero crossings, wider with the gate.
    let mut gaps = Vec::new();
    for k in 0..=(lc as u32 * 2) {
        let gx = period * f64::from(k) / 2.0;
        let gap = b.add(
            roll,
            draw::rect(
                "starved",
                gx - 7.0,
                h / 2.0 - 9.0,
                14.0,
                18.0,
                draw::solid("#0b0907"),
            ),
        );
        gaps.push(gap);
    }
    for gap in gaps {
        // Past noon (the circuit as designed) the gate starves it.
        motion::add(
            b,
            gap,
            Motion::new(Target::Opacity)
                .input(format!("min(max(norm({}) - 0.5, 0) * 8, 1)", a("gate")))
                .range(0.0, 1.0),
        );
        motion::add(
            b,
            gap,
            Motion::new(Target::StretchX)
                .input(format!("max(norm({}) - 0.5, 0) * 2", a("gate")))
                .range(0.05, 1.0)
                .pivot(0.5, 0.5)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
    }
    parts::silk(
        b,
        p,
        "Clean",
        22.0,
        10.0,
        Ink::new(5.5, "#ffb34780").weight(800.0).track(0.0),
    );
    parts::silk(
        b,
        p,
        "Out",
        52.0,
        10.0,
        Ink::new(5.5, TRACE).weight(800.0).track(0.0),
    );
}

/// The scope window at `(sx, sy, sw, sh)` of the lid (`size`), in the
/// picture layer; the emphasis EQ in its place while that shows.
fn scope(
    b: &mut Builder<'_>,
    lid: NodeId,
    (sx, sy, sw, sh): (f64, f64, f64, f64),
    size: (f64, f64),
) {
    let pic = super::picture(b, lid, "transistor", size);
    let scope = crate::reverb::ui::display(b, pic, "Scope", sx, sy, sw, sh, TRACE, "crt");
    curve_panel(b, scope, (8.0, 6.0, sh - 12.0, sh - 12.0));
    b.add(
        scope,
        parts::lines_node(
            "divider",
            &[vec![(sh + 2.0, 8.0), (sh + 2.0, sh - 8.0)]],
            0.8,
            "#ffb34733",
        ),
    );
    wave_panel(b, scope, (sh + 10.0, 6.0, sw - sh - 18.0, sh - 12.0));
    crate::reverb::ui::glass(b, scope);
    super::emphasis_eq(b, lid, "transistor", (sx, sy, sw, sh), TRACE);
}

pub fn face() -> Face {
    let scene = canvas("Transistor", |b, root| {
        parts::fit_root(b, root, W, H);
        let lid = art::enclosure(
            b,
            root,
            (W, H),
            &[(0.0, "#d6dbe0"), (0.5, "#b4bbc2"), (1.0, "#8c949c")],
            (0.03, 0.05),
        );

        // ── Title ──────────────────────────────────────────────────────
        parts::silk(
            b,
            lid,
            "Transistor",
            W / 2.0 - 4.0,
            36.0,
            Ink::new(22.0, INK).weight(900.0).track(0.12),
        );
        parts::silk(
            b,
            lid,
            "Fuzz · Germanium PNP",
            W / 2.0 - 4.0,
            58.0,
            Ink::new(7.0, DIM).weight(800.0).track(0.3),
        );

        // ── The scope; or the emphasis EQ ──────────────────────────────
        scope(b, lid, (56.0, 74.0, W - 112.0, 128.0), (W, H));
        super::emphasis_key(b, lid, "transistor", (646.0, 36.0), 64.0, "black", TRACE);

        let scale = Ink::new(7.0, DIM).weight(800.0).track(0.0).plain();
        let legend = Ink::new(10.5, INK).weight(900.0).track(0.18);
        let value = Ink::new(7.5, DIM).weight(700.0).track(0.02).plain();

        // ── The three every fuzz has ───────────────────────────────────
        let r1 = 268.0;
        let big: [KnobAt; 3] = [
            ("drive", "Gain", 176.0, parts::PCT_MARKS),
            ("tone", "Tone", 376.0, super::TRIM_MARKS),
            ("output", "Level", 576.0, super::OUT_MARKS),
        ];
        for (addr, name, x, marks) in big {
            let k = parts::knob(
                b,
                lid,
                "knob.bakelite",
                "black",
                &a(addr),
                name,
                (x, r1),
                66.0,
            );
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, r1), 40.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, r1 + 58.0, legend);
            parts::value_at(b, k, r1 + 73.0, value);
        }

        // ── The circuit's trims ────────────────────────────────────────
        let r2 = 384.0;
        let small: [KnobAt; 4] = [
            ("edge", "Edge", 150.0, super::TRIM_MARKS),
            ("gate", "Gate", 290.0, super::TRIM_MARKS),
            ("sag", "Sag", 430.0, super::TRIM_MARKS),
            ("mix", "Mix", 590.0, parts::PCT_MARKS),
        ];
        for (addr, name, x, marks) in small {
            let k = parts::knob(
                b,
                lid,
                "knob.bakelite",
                "black",
                &a(addr),
                name,
                (x, r2),
                42.0,
            );
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, r2), 28.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, r2 + 42.0, legend.size(9.0));
        }
        parts::vrule(b, lid, 512.0, r2 - 30.0, r2 + 48.0, 1.0, "#17181a40");

        // ── Foot ───────────────────────────────────────────────────────
        let fy = 474.0;
        let mode = Place::new("Mode")
            .size(34.0)
            .variant("chrome")
            .legend("Mode")
            .options(&["Fuzz", "Drive"])
            .bind(bind(&a("fuzz")))
            .ink(KitInk::Color(INK.into()));
        let sw = parts::put(b, lid, "switch.toggle_bat", &mode, 170.0, fy - 14.0);
        parts::caption_style(b, sw, Ink::new(7.5, INK).weight(900.0).track(0.18));
        let bypass = Place::new("Bypass")
            .size(34.0)
            .variant("red")
            .bind(bind(&a("on")));
        let on = parts::footswitch(b, lid, &bypass, (376.0, fy));
        parts::label_at(
            b,
            on,
            "On",
            (406.0, fy - 36.0),
            Ink::new(7.5, DIM).weight(900.0).track(0.2),
        );
        symbol(b, lid, (650.0, fy - 12.0), 20.0, INK);
    });

    Face {
        file: "04-transistor",
        plugin: "saturate",
        scene,
        params: params(),
        streams: Vec::new(),
    }
}

fn params() -> Vec<frame::surface::proto::ParamInfo> {
    vec![
        parts::percent(&a("drive"), "Gain", 60.0).featured(),
        parts::trim(&a("tone"), "Tone").featured(),
        super::output(&a("output")),
        parts::trim(&a("edge"), "Edge"),
        parts::trim(&a("gate"), "Gate"),
        parts::trim(&a("sag"), "Sag"),
        parts::percent(&a("mix"), "Mix", 100.0),
        parts::toggle(&a("fuzz"), "Fuzz", true),
        parts::toggle(&a("on"), "Bypass", true).featured(),
    ]
    .into_iter()
    .chain(super::emphasis_params("transistor"))
    .collect()
}

/// The pedal alone in a lane ([`Shape::Tall`], 760 × 328): the name, the
/// mode toggle and the stomp switch at the left, the scope across the top
/// with the emphasis key and the part's symbol beside it, and the seven
/// knobs in one row below — the three every fuzz has, then the trims.
pub fn tall_face() -> Face {
    let (w, h) = Shape::Tall.size();
    let scene = canvas("Transistor (tall)", |b, root| {
        parts::fit_root(b, root, w, h);
        let lid = art::enclosure(
            b,
            root,
            (w, h),
            &[(0.0, "#d6dbe0"), (0.5, "#b4bbc2"), (1.0, "#8c949c")],
            (0.03, 0.05),
        );
        let (lw, lh) = (w - 8.0, h - 12.0);

        // ── Title, mode, stomp ─────────────────────────────────────────
        parts::silk_left(
            b,
            lid,
            "Transistor",
            30.0,
            32.0,
            Ink::new(18.0, INK).weight(900.0).track(0.12),
        );
        parts::silk_left(
            b,
            lid,
            "Fuzz · Germanium PNP",
            30.0,
            52.0,
            Ink::new(6.5, DIM).weight(800.0).track(0.3),
        );
        let mode = Place::new("Mode")
            .size(30.0)
            .variant("chrome")
            .legend("Mode")
            .options(&["Fuzz", "Drive"])
            .bind(bind(&a("fuzz")))
            .ink(KitInk::Color(INK.into()));
        let sw = parts::put(b, lid, "switch.toggle_bat", &mode, 70.0, 112.0);
        parts::caption_style(b, sw, Ink::new(7.0, INK).weight(900.0).track(0.18));
        let bypass = Place::new("Bypass")
            .size(30.0)
            .variant("red")
            .bind(bind(&a("on")));
        let on = parts::footswitch(b, lid, &bypass, (150.0, 118.0));
        parts::label_at(
            b,
            on,
            "On",
            (174.0, 86.0),
            Ink::new(7.0, DIM).weight(900.0).track(0.2),
        );

        // ── The scope; or the emphasis EQ ──────────────────────────────
        scope(b, lid, (204.0, 18.0, 458.0, 136.0), (lw, lh));
        super::emphasis_key(b, lid, "transistor", (704.0, 46.0), 52.0, "black", TRACE);
        symbol(b, lid, (712.0, 122.0), 15.0, INK);

        // ── The knobs ──────────────────────────────────────────────────
        let row = 238.0;
        let scale = Ink::new(7.0, DIM).weight(800.0).track(0.0).plain();
        let legend = Ink::new(9.5, INK).weight(900.0).track(0.18);
        let value = Ink::new(7.5, DIM).weight(700.0).track(0.02).plain();
        let knobs: [(&str, &str, f64, f64, &[(f64, &str)]); 7] = [
            ("drive", "Gain", 70.0, 62.0, parts::PCT_MARKS),
            ("tone", "Tone", 172.0, 62.0, super::TRIM_MARKS),
            ("output", "Level", 274.0, 62.0, super::OUT_MARKS),
            ("edge", "Edge", 380.0, 52.0, super::TRIM_MARKS),
            ("gate", "Gate", 468.0, 52.0, super::TRIM_MARKS),
            ("sag", "Sag", 556.0, 52.0, super::TRIM_MARKS),
            ("mix", "Mix", 666.0, 52.0, parts::PCT_MARKS),
        ];
        for (addr, name, x, d, marks) in knobs {
            let k = parts::knob(
                b,
                lid,
                "knob.bakelite",
                "black",
                &a(addr),
                name,
                (x, row),
                d,
            );
            let sweep = parts::sweep_of(b, k);
            parts::marks(b, lid, (x, row), d / 2.0 + 7.0, sweep, marks, 11, scale);
            parts::legend_at(b, k, name, row + 50.0, legend);
            parts::value_at(b, k, row + 64.0, value);
        }
        parts::rule(b, lid, 24.0, lw - 24.0, 168.0, 1.0, "#17181a30");
        parts::vrule(b, lid, 327.0, row - 46.0, row + 66.0, 1.0, "#17181a40");
        parts::vrule(b, lid, 611.0, row - 46.0, row + 66.0, 1.0, "#17181a40");
    });
    Face {
        file: Shape::Tall.file("04-transistor"),
        plugin: "saturate",
        scene,
        params: params(),
        streams: Vec::new(),
    }
}
