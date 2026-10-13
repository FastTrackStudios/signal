//! NonLinear: the gated reverb of the eighties — the big drum room cut
//! dead. A digital rack unit in the manner of the first non-linear
//! programs: a brushed faceplate, and across most of it a phosphor scope
//! of what the program does to a hit.
//!
//! The scope is the algorithm. The hit, a white spike at the left; after
//! the pre-delay, the burst — a dense wall of reflections that builds (the
//! larger the room, the slower) to a flat top and holds there, every
//! reflection as loud as the last, until the gate slams it shut. The gate
//! is the bright edge with its tab: drag it (the decay) and the wall grows
//! or shortens; beyond it, dashed, the tail a natural room would have let
//! ring and the gate cut off. While the reverb plays a trace runs along the
//! top of the wall to the gate and starts again.
//!
//! Two finishes: light, the pale blue-grey enamel of an eighties studio
//! rack; dark, black anodising.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{draw, motion};
use surface_proto::{Axis, Control, ControlKind, HandleAxis};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

/// The algorithm this face draws, as `VERB_ALGOS` names it.
const NAME: &str = "NonLinear";
/// The scope's phosphor: the section's purple.
const LED: &str = kit::PURPLE_LAMP;
/// The hit's flash, whiter than the burst.
const HIT: &str = "#f3ecff";

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    plate: [(f64, &'static str); 3],
    brush: &'static str,
    /// A rule's shadow and its light edge.
    rules: (&'static str, &'static str),
    /// The data wheel (the Time knob) and the small knobs.
    wheel: [&'static str; 5],
    cap: [&'static str; 6],
    accent: &'static str,
}

const LIGHT: Look = Look {
    ink: "#1d2330",
    dim: "#5c6577",
    plate: [(0.0, "#e6eaf0"), (0.55, "#d3d9e2"), (1.0, "#bec6d2")],
    brush: "#0000000b",
    rules: ("#00000030", "#ffffff90"),
    wheel: [
        "color=#23222a",
        "pointer=#efe9ff",
        "index=dot",
        "skirt=plain",
        "skirt_color=#3a3944",
    ],
    cap: [
        "color=#eef1f5",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#1d2330",
    ],
    accent: "#6a3fc4",
};

const DARK: Look = Look {
    ink: "#e9e6f0",
    dim: "#8b879b",
    plate: [(0.0, "#25242b"), (0.55, "#1a191f"), (1.0, "#111015")],
    brush: "#ffffff06",
    rules: ("#000000a8", "#ffffff12"),
    wheel: [
        "color=#18171d",
        "pointer=#d9c8ff",
        "index=dot",
        "skirt=plain",
        "skirt_color=#0b0a0e",
    ],
    cap: [
        "color=#2b2a32",
        "pointer=#c4b5fd",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#0b0a0e",
    ],
    accent: kit::PURPLE_LAMP,
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// Where the envelope sits in the scope (its own coordinates, `w` × `h`):
/// the hit, the baseline and the wall's top, how far the pre-delay can push
/// the burst, the gate's travel, and the reflections' pitch.
#[derive(Clone, Copy)]
struct Scope {
    w: f64,
    h: f64,
    hit: f64,
    base: f64,
    top: f64,
    pre: f64,
    gate: (f64, f64),
    pitch: f64,
}

impl Scope {
    fn new(w: f64, h: f64, compact: bool) -> Self {
        let hit = if compact { 12.0 } else { 18.0 };
        let pre = if compact { 16.0 } else { 40.0 };
        Self {
            w,
            h,
            hit,
            base: h - 15.0,
            top: 24.0,
            pre,
            gate: (hit + 30.0, w - if compact { 14.0 } else { 70.0 }),
            pitch: if compact { 3.0 } else { 3.4 },
        }
    }

    /// Where the burst starts with no pre-delay.
    fn start(&self) -> f64 {
        self.hit + 6.0
    }

    /// The burst's shift for the pre-delay now, px (an expression).
    fn pre_x(&self) -> String {
        format!("({:.1} * norm({VERB}/predelay))", self.pre)
    }

    /// The gate's x now, px (an expression): where the handle's centre is.
    fn gate_x(&self) -> String {
        format!(
            "({:.1} + {:.1} * norm({VERB}/decay))",
            self.gate.0,
            self.gate.1 - self.gate.0
        )
    }
}

/// The faceplate, `w` wide: brushed enamel or anodising, a pinstripe pair
/// in the section's purples along the foot. Returns the plate.
fn faceplate(b: &mut Builder<'_>, root: NodeId, w: f64, l: &Look) -> NodeId {
    let mut plate = draw::frame("Faceplate", 0.0, 0.0, w, kit::H);
    plate.fills = vec![draw::linear(180.0, &l.plate)];
    plate.corner_radii = document_proto::CornerRadii::uniform(4.0);
    let plate = b.add(
        root,
        draw::with_effects(
            plate,
            vec![
                draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0),
                draw::inner_shadow("#00000070", 0.0, -2.0, 3.0),
            ],
        ),
    );
    let brush: Vec<Vec<(f64, f64)>> = (0..80)
        .map(|i| {
            vec![
                (0.0, 1.0 + f64::from(i) * 2.05),
                (w, 1.0 + f64::from(i) * 2.05),
            ]
        })
        .collect();
    b.add(
        plate,
        ui::strokes("brush", &brush, 0.5, draw::solid(l.brush)),
    );
    b.add(
        plate,
        draw::rect(
            "stripe",
            0.0,
            kit::H - 6.0,
            w,
            2.0,
            draw::solid(kit::PURPLE),
        ),
    );
    b.add(
        plate,
        draw::rect(
            "stripe",
            0.0,
            kit::H - 3.0,
            w,
            1.0,
            draw::solid(kit::PURPLE_DEEP),
        ),
    );
    plate
}

/// The scope's furniture: its graticule, the axis and its words.
fn graticule(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    let faint = ui::alpha(LED, 0.1);
    let mut grid: Vec<Vec<(f64, f64)>> = (0..3)
        .map(|i| {
            let y = s.top + (s.base - s.top) * f64::from(i) / 3.0;
            vec![(0.0, y), (s.w, y)]
        })
        .collect();
    let mut x = s.hit;
    while x < s.w {
        grid.push(vec![(x, 6.0), (x, s.base)]);
        x += 36.0;
    }
    b.add(
        win,
        ui::strokes("graticule", &grid, 0.6, draw::solid(&faint)),
    );
    ui::line(
        b,
        win,
        "axis",
        &[(4.0, s.base), (s.w - 4.0, s.base)],
        0.8,
        &ui::alpha(LED, 0.35),
    );
    let label = ui::alpha(LED, 0.6);
    ui::lit_text(
        b,
        win,
        "Envelope",
        s.hit + 8.0,
        10.0,
        80.0,
        5.5,
        &label,
        TextAlign::Left,
        700.0,
    );
    ui::lit_text(
        b,
        win,
        "Hit",
        s.hit - 8.0,
        s.base + 8.0,
        30.0,
        5.0,
        &label,
        TextAlign::Left,
        700.0,
    );
    ui::lit_text(
        b,
        win,
        "Time",
        s.w - 44.0,
        s.base + 8.0,
        40.0,
        5.0,
        &label,
        TextAlign::Right,
        700.0,
    );
}

/// The hit: a white spike at the left, flashing as each hit lands.
fn hit(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    let spike = draw::rect(
        "hit",
        s.hit - 1.5,
        6.0,
        3.0,
        s.base - 6.0,
        draw::linear(180.0, &[(0.0, HIT), (1.0, &ui::alpha(HIT, 0.6))]),
    );
    let spike = b.add(win, ui::glowing(spike, LED, 1.2, kit::GLOW));
    let flash =
        format!("(0.55 + 0.45 * pow(1 - fract(time * 0.8), 6)) * (0.3 + 0.7 * param({VERB}/on))");
    motion::add(
        b,
        spike,
        Motion::new(Target::Opacity).input(flash).range(0.0, 1.0),
    );
}

/// The burst: a wall of reflections from the hit, one bar a `pitch` apart,
/// each a touch higher or lower than the next. It builds over the room's
/// size, holds flat, and is lit only up to the gate; past it each bar
/// leaves a dim stub. The whole wall moves right with the pre-delay.
fn burst(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    let wall = b.add(win, draw::frame("Burst", 0.0, 0.0, s.w, s.h));
    motion::add(
        b,
        wall,
        Motion::new(Target::X)
            .input(s.pre_x())
            .range(0.0, 1.0)
            .dynamics(Dynamics::spring(260.0, 30.0)),
    );
    let tall = s.base - s.top;
    // The wall's light, behind it: as long as the gate lets it be.
    let span = s.w - s.start();
    let glow = b.add(
        wall,
        draw::frame("light", s.start(), s.top - 6.0, span, tall + 6.0),
    );
    // It gives off light too, as far as the gate lets it: the wall glows
    // (one emitter for the whole wall, not one per reflection).
    b.add(
        glow,
        ui::glowing(
            draw::rect(
                "glow",
                0.0,
                0.0,
                span,
                tall + 6.0,
                draw::linear(
                    180.0,
                    &[(0.0, &ui::alpha(LED, 0.0)), (1.0, &ui::alpha(LED, 0.26))],
                ),
            ),
            LED,
            0.22,
            kit::BLOOM,
        ),
    );
    let open = format!(
        "clamp(({} - {:.1} - {}) / {span:.1}, 0, 1)",
        s.gate_x(),
        s.start(),
        s.pre_x()
    );
    motion::add(
        b,
        glow,
        Motion::new(Target::StretchX)
            .input(open)
            .range(0.0, 1.0)
            .pivot(0.0, 0.5),
    );
    motion::add(
        b,
        glow,
        Motion::new(Target::Opacity)
            .input(format!("0.3 + 0.7 * param({VERB}/on)"))
            .range(0.0, 1.0),
    );
    let count = (span / s.pitch).floor();
    let mut i = 0.0;
    while i < count {
        let x = s.start() + i * s.pitch;
        let j = crate::reverb::jitter(i * 1.37);
        let hgt = tall * (0.9 + 0.08 * j);
        let bar = draw::rect(
            "reflection",
            x,
            s.base - hgt,
            (s.pitch * 0.5).max(1.2),
            hgt,
            draw::linear(180.0, &[(0.0, LED), (1.0, &ui::alpha(LED, 0.45))]),
        );
        let bar = b.add(wall, bar);
        // The build: the bigger the room, the longer to the full wall.
        let d = x - s.start();
        let build = format!("clamp({d:.1} / (4 + 70 * norm({VERB}/size)), 0.06, 1)");
        motion::add(
            b,
            bar,
            Motion::new(Target::StretchY)
                .input(build)
                .range(0.0, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(Dynamics::spring(240.0, 28.0)),
        );
        // Lit up to the gate (where this bar is now, the pre-delay added),
        // a dim stub past it; a faint shimmer while the reverb plays.
        let open = format!("clamp(({} - {x:.1} - {}) / 3, 0, 1)", s.gate_x(), s.pre_x());
        let live = format!(
            "(0.05 + 0.95 * {open}) * (0.35 + 0.65 * param({VERB}/on)) * (0.86 + 0.14 * sin(time * {:.2} + {:.2}))",
            5.0 + 3.0 * j.abs(),
            i * 0.7
        );
        motion::add(
            b,
            bar,
            Motion::new(Target::Opacity).input(live).range(0.0, 1.0),
        );
        i += 1.0;
    }
}

/// What the gate cut: past the gate, dashed, the tail a natural room would
/// have let ring — riding with the gate.
fn ghost(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    let g = b.add(win, draw::frame("Cut tail", 0.0, 0.0, s.w, s.h));
    motion::add(
        b,
        g,
        Motion::new(Target::X)
            .input(s.gate_x())
            .range(0.0, 1.0)
            .dynamics(Dynamics::spring(320.0, 34.0)),
    );
    let tall = s.base - s.top;
    let pts: Vec<(f64, f64)> = (0..=48)
        .map(|k| {
            let u = f64::from(k) / 48.0;
            (u * 200.0, s.base - tall * (-3.2 * u).exp())
        })
        .collect();
    let mut fill = format!("M 0 {:.1} ", s.base);
    for (x, y) in &pts {
        fill += &format!("L {x:.1} {y:.1} ");
    }
    fill += &format!("L 200 {:.1} Z", s.base);
    b.add(
        g,
        draw::path(
            "cut",
            0.0,
            0.0,
            s.w,
            s.h,
            fill,
            draw::linear(
                180.0,
                &[(0.0, &ui::alpha(LED, 0.12)), (1.0, &ui::alpha(LED, 0.02))],
            ),
        ),
    );
    let edge = ui::line(b, g, "would have rung", &pts, 1.0, &ui::alpha(LED, 0.45));
    if let Some(n) = b.scene.node_mut(edge) {
        n.stroke.dash_pattern = vec![3.0, 3.0];
    }
}

/// The trace: a bright point running along the top of the wall from the
/// burst's start to the gate, again and again while the reverb plays.
fn trace(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    let dot = draw::circle("trace", 0.0, s.top - 1.0, 2.6, draw::solid(HIT));
    let dot = b.add(win, ui::glowing(dot, LED, 1.3, kit::GLOW));
    let from = format!("({:.1} + {})", s.start(), s.pre_x());
    let run = format!(
        "{from} + fract(time * 0.8) * max({} - {from}, 0)",
        s.gate_x()
    );
    motion::add(b, dot, Motion::new(Target::X).input(run).range(0.0, 1.0));
    motion::add(
        b,
        dot,
        Motion::new(Target::Opacity)
            .input(format!(
                "param({VERB}/on) * (1 - pow(fract(time * 0.8), 8))"
            ))
            .range(0.0, 1.0),
    );
}

/// The gate: the bright edge with its tab, a handle over the decay — drag
/// it to lengthen or cut the wall.
fn gate(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    let area = b.add(
        win,
        draw::frame("gate travel", s.gate.0, 0.0, s.gate.1 - s.gate.0, s.h),
    );
    let hw = 24.0;
    let g = b.add(area, draw::frame("Gate", 0.0, 0.0, hw, s.h));
    let c = hw / 2.0;
    // The cliff, top to baseline, and its glow.
    let cliff = draw::rect(
        "edge",
        c - 1.0,
        s.top - 4.0,
        2.0,
        s.base - s.top + 4.0,
        draw::solid(HIT),
    );
    b.add(g, ui::glowing(cliff, LED, 1.1, kit::BLOOM));
    // The tab: a flag over the wall that says what it is.
    let tab = draw::rounded(
        draw::rect("tab", c - 11.0, 4.0, 22.0, 12.0, draw::solid(LED)),
        2.5,
    );
    b.add(g, ui::glowing(tab, LED, 0.7, kit::GLOW));
    b.add(
        g,
        draw::path(
            "point",
            c - 4.0,
            16.0,
            8.0,
            4.0,
            "M 0 0 L 8 0 L 4 4 Z".into(),
            draw::solid(LED),
        ),
    );
    ui::plain(b, g, "Gate", c, 10.0, 5.5, "#140a24", 800.0);
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls
        .insert(g, Control::new(kind).bound(ui::bind(&a("decay"))));
}

/// The scope's contents, `s.w` × `s.h`.
fn scope(b: &mut Builder<'_>, win: NodeId, s: &Scope) {
    graticule(b, win, s);
    ghost(b, win, s);
    burst(b, win, s);
    hit(b, win, s);
    trace(b, win, s);
    gate(b, win, s);
}

/// The nameplate's title: "NonLinear" in a wide eighties face.
fn title(b: &mut Builder<'_>, t: NodeId, (w, h): (f64, f64), size: f64, ink: &str) {
    let l = draw::Lettering::family(draw::family::ORBITRON).weight(700.0);
    b.add(
        t,
        draw::styled_text(
            "title",
            NAME,
            0.0,
            h / 2.0 - size * 0.68,
            w - 12.0,
            size,
            ink,
            TextAlign::Left,
            &l,
        ),
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: one design — the faceplate, its scope and its
/// lettering — laid out for the room. On a phone the scope narrows to the
/// left of the lane and the knobs give way to the standard Time knob and
/// two knobs (Size, Level); the gate stays a handle on the scope.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let plate = faceplate(b, root, w, l);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        let mark = draw::Lettering::family(draw::family::ORBITRON).weight(600.0);

        // ── The scope ──────────────────────────────────────────────────
        let (dx, dy) = (16.0, 30.0);
        let dw = if small_room {
            kit::PHONE_PICTURE_R - dx - 4.0
        } else {
            436.0
        };
        let dh = kit::H - dy - 16.0;
        b.add(
            plate,
            draw::styled_text(
                "maker",
                if small_room {
                    "Gated Reverb"
                } else {
                    "Non-Linear Reverberation"
                },
                dx,
                11.0,
                240.0,
                7.0,
                l.ink,
                TextAlign::Left,
                &mark,
            ),
        );
        if !small_room {
            ui::silk_left(
                b,
                plate,
                "Gate · Burst · Program 10",
                dx + 214.0,
                16.0,
                5.5,
                l.dim,
                700.0,
            );
        }
        let win = ui::display(b, plate, "Scope", dx, dy, dw, dh, LED, "crt");
        scope(b, win, &Scope::new(dw, dh, small_room));
        ui::glass(b, win);

        if small_room {
            kit::nameplate_top_right(
                b,
                plate,
                VERB,
                tier,
                (6.0, 30.0),
                112.0,
                210.0,
                12.0,
                (l.accent, l.dim),
                |b, t| title(b, t, (112.0, 30.0), 12.0, l.ink),
            );
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(
                b,
                plate,
                VERB,
                (tx, ty, td),
                11.0,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in [("size", "Size"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(
                    b,
                    plate,
                    &a(id),
                    legend,
                    kd,
                    (xs[i], ky),
                    &l.cap,
                    l.ink,
                    l.dim,
                );
            }
        } else {
            kit::nameplate_top_right(
                b,
                plate,
                VERB,
                tier,
                (12.0, 26.0),
                108.0,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| title(b, t, (108.0, 26.0), 11.0, l.ink),
            );
            // The data wheel: the Time knob.
            let (tx, ty, td) = (dx + dw + 64.0, 70.0, 52.0);
            kit::time_knob(
                b,
                plate,
                VERB,
                (tx, ty, td),
                8.5,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            // The room and the program's knobs.
            let kx = tx + 76.0;
            ui::rule(b, plate, kx - 26.0, 44.0, 1.0, kit::H - 60.0, l.rules.0);
            ui::rule(b, plate, kx - 25.0, 44.0, 1.0, kit::H - 60.0, l.rules.1);
            for (i, (id, legend)) in [
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("damping", "HF Damp"),
                ("tone", "Tone"),
                ("level", "Level"),
            ]
            .iter()
            .enumerate()
            {
                kit::knob3d(
                    b,
                    plate,
                    &a(id),
                    legend,
                    32.0,
                    (kx + 60.0 * i as f64, 88.0),
                    &l.cap,
                    l.ink,
                    l.dim,
                );
            }
        }
        kit::face_bypass(b, plate, VERB, tier, "purple", "In");
        kit::flip_arrow(
            b,
            plate,
            VERB,
            tier,
            false,
            (l.ink, &ui::alpha(l.dim, 0.25)),
        );
        // Responsive: the scope (and its bezel) takes the width.
        let stretch = kit::with_sibling(b, plate, win, -1);
        kit::responsive(b, plate, &stretch);
    });
    let mut f = kit::reverb_face(kit::tier_file("89-verb-nonlinear", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the same faceplate, no scope — the advanced controls
/// across it, and (standing alone) the nameplate, the bypass and the arrow
/// where they are on the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let plate = faceplate(b, root, w, l);
        let knobs = [
            ("predelay", "Pre-Delay"),
            ("damping", "HF Damp"),
            ("modulation", "Mod"),
            ("low_cut", "Low Cut"),
            ("high_cut", "High Cut"),
            ("tone", "Tone"),
            ("duck", "Duck"),
            ("duck_threshold", "Duck Thr"),
            ("duck_release", "Duck Rel"),
            ("pan_a", "Pan"),
        ];
        kit::back_knobs(b, plate, tier, &knobs, l.ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &l.cap, l.ink, l.dim);
        });
        let (np, size, title_w, title_size) = if small_room {
            ((6.0, 30.0), 12.0, 112.0, 12.0)
        } else {
            ((12.0, 26.0), 10.0, 108.0, 11.0)
        };
        kit::head(b, plate, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                title_w,
                if small_room { 210.0 } else { 200.0 },
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, (title_w, np.1), title_size, l.ink),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "In");
        });
        kit::flip_arrow(b, plate, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.25)));
    });
    let mut f = kit::reverb_face(kit::tier_file("89-verb-nonlinear-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the scope the whole top row, every control on the
/// faceplate below it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let mut plate = draw::frame("Faceplate", 0.0, 0.0, kit::TALL_W, kit::TALL_H);
        plate.fills = vec![draw::linear(180.0, &l.plate)];
        plate.corner_radii = document_proto::CornerRadii::uniform(4.0);
        let plate = b.add(
            root,
            draw::with_effects(plate, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0)]),
        );
        let brush: Vec<Vec<(f64, f64)>> = (0..160)
            .map(|i| {
                vec![
                    (0.0, 1.0 + f64::from(i) * 2.05),
                    (kit::TALL_W, 1.0 + f64::from(i) * 2.05),
                ]
            })
            .collect();
        b.add(
            plate,
            ui::strokes("brush", &brush, 0.5, draw::solid(l.brush)),
        );
        let (dx, dy, dw, dh) = kit::TALL_PICTURE;
        let (dx, dy, dw, dh) = (dx + 4.0, dy + 4.0, dw - 8.0, dh - 8.0);
        let win = ui::display(b, plate, "Scope", dx, dy, dw, dh, LED, "crt");
        scope(b, win, &Scope::new(dw, dh, false));
        ui::glass(b, win);
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("damping", "HF Damp"),
                ("level", "Level"),
            ],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: if dark { "#1a191fe6" } else { "#d3d9e2e6" },
            bypass: ("purple", "In"),
            title_w: 112.0,
        };
        kit::tall_chrome(
            b,
            plate,
            &t,
            |b, p, addr, legend, d, at| {
                kit::knob3d(b, p, addr, legend, d, at, &l.cap, l.ink, l.dim);
            },
            |b, p, addr, (cx, cy, d)| {
                kit::knob3d_bare(b, p, addr, d, (cx, cy), &l.wheel);
            },
            |b, tn| title(b, tn, (112.0, 30.0), 12.0, l.ink),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file("89-verb-nonlinear", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
