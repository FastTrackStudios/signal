//! Shimmer: a reverb whose tail is fed back through a pitch shifter, so
//! every pass round the loop comes back an interval higher (or lower) —
//! the note, then its octave, then the octave above that, a staircase of
//! light climbing out of the sound. Drawn as that cascade: time across,
//! pitch up, each pass a streak of light one step from the last. The step
//! is the shift (a fifth, an octave, or down), each pass fainter by the
//! shimmer amount; the longer the decay the further apart in time they
//! fall and the longer each one rings. A glint climbs the stairs while it
//! plays.
//!
//! The head of the cascade is a crystal you can move: up and down for
//! the shift, out for the decay.
//!
//! Two finishes: light, frosted pearl with an iridescent edge; dark,
//! midnight.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::proto::{Axis, Control, ControlKind, HandleAxis};
use frame::surface::widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const NAME: &str = "Shimmer";
const FILE: &str = "86-verb-shimmer";
const BACK: &str = "86-verb-shimmer-back";
/// The tall face's rows: the picture a little under half, big knobs below.
const ROWS: kit::TallRows = kit::TallRows {
    split: 134.0,
    main: (30.0, 50.0),
    more: (126.0, 40.0),
};
/// The cascade's light: the section's purple, whitening as it climbs.
const LIGHT_0: &str = kit::PURPLE_LAMP;
const ICE: &str = "#eef0ff";
/// Passes drawn: the note and five returns.
const PASSES: usize = 6;
/// The front knobs, in order.
const FRONT: [(&str, &str); 4] = [
    ("shim_amount", "Shimmer"),
    ("shim_shift1", "Shift"),
    ("size", "Size"),
    ("level", "Level"),
];

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    panel: [(f64, &'static str); 3],
    edge: [(f64, &'static str); 4],
    well: [(f64, &'static str); 2],
    knob: [&'static str; 6],
    wheel: [&'static str; 6],
    accent: &'static str,
    scrim: &'static str,
}

const LIGHT: Look = Look {
    ink: "#1e2433",
    dim: "#667086",
    panel: [(0.0, "#f2f4f8"), (0.55, "#e4e8ef"), (1.0, "#d3d9e3")],
    edge: [
        (0.0, "#f3c4e4"),
        (0.35, "#c4d8f8"),
        (0.7, "#cfc2ff"),
        (1.0, "#bfeff0"),
    ],
    well: [(0.0, "#121a33"), (1.0, "#090e1f")],
    knob: [
        "color=#f6f7fa",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#aab3c2",
    ],
    wheel: [
        "color=#f6f7fa",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        "skirt_color=#8f99aa",
    ],
    accent: "#6a3fc4",
    scrim: "#e4e8efe6",
};

const DARK: Look = Look {
    ink: "#e6e9f2",
    dim: "#8b92a6",
    panel: [(0.0, "#1a1f2e"), (0.55, "#131725"), (1.0, "#0c0f19")],
    edge: [
        (0.0, "#f3c4e455"),
        (0.35, "#c4d8f855"),
        (0.7, "#cfc2ff55"),
        (1.0, "#bfeff055"),
    ],
    well: [(0.0, "#0b1022"), (1.0, "#05070f")],
    knob: [
        "color=#252c40",
        "pointer=#d9c8ff",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#0b0e18",
    ],
    wheel: [
        "color=#20263a",
        "pointer=#d9c8ff",
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        "skirt_color=#090c15",
    ],
    accent: kit::PURPLE_LAMP,
    scrim: "#131725e6",
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// The cascade's geometry in a picture `w` × `h` whose band starts at
/// `top`: where the note starts, the pass spacing at no decay and at full,
/// the band's middle, and how far a pass climbs at a full octave (`a` the
/// note's own offset, `b` each pass's step).
#[derive(Clone, Copy)]
struct Stairs {
    x0: f64,
    s_min: f64,
    s_max: f64,
    mid: f64,
    a: f64,
    b: f64,
}

impl Stairs {
    fn new(w: f64, h: f64, top: f64) -> Self {
        let x0 = if w < 300.0 { 18.0 } else { 34.0 };
        let s_max = (w - x0 - 14.0) / (PASSES as f64 - 1.0 + 1.7);
        let (t, bot) = (top.max(14.0), h - 14.0);
        let band = bot - t;
        Self {
            x0,
            s_min: s_max * 0.45,
            s_max,
            mid: (t + bot) / 2.0,
            a: band * 0.5,
            b: band * 0.2,
        }
    }

    /// Pass `k`'s height now (an expression, picture coordinates).
    fn y(&self, k: f64) -> String {
        format!(
            "({:.2} + param({VERB}/shim_shift1) / 12 * {:.2})",
            self.mid,
            self.a - k * self.b
        )
    }

    /// How far the passes spread with the decay (a factor on `s_min`).
    fn spread(&self) -> String {
        format!(
            "(1 + {:.3} * norm({VERB}/decay))",
            self.s_max / self.s_min - 1.0
        )
    }
}

/// The pitch step between two passes as a stroke from (0, `y0`) to
/// (`dx`, `y0 - dy`), an arrowhead at its end.
fn step_path(dx: f64, y0: f64, dy: f64) -> String {
    let mut d = String::new();
    for i in 0..=12 {
        let t = f64::from(i) / 12.0;
        // A quadratic from the start up to the end, leaving flat.
        let (cx, cy) = (dx * 0.15, y0 - dy);
        let x = (1.0 - t).powi(2) * 2.0 + 2.0 * (1.0 - t) * t * cx + t * t * dx;
        let y = (1.0 - t).powi(2) * y0 + 2.0 * (1.0 - t) * t * cy + t * t * (y0 - dy);
        d.push_str(&format!(
            "{} {x:.2} {y:.2} ",
            if i == 0 { "M" } else { "L" }
        ));
    }
    let (ex, ey) = (dx, y0 - dy);
    d.push_str(&format!(
        "M {:.2} {:.2} L {ex:.2} {ey:.2} L {:.2} {:.2}",
        ex - 4.0,
        ey - 2.5,
        ex - 4.0,
        ey + 2.5
    ));
    d
}

/// The cascade in `well` (`w` × `h`) from `top` down.
#[allow(clippy::too_many_lines)]
fn cascade(b: &mut Builder<'_>, well: NodeId, (w, h): (f64, f64), top: f64) {
    let s = Stairs::new(w, h, top);
    let compact = w < 300.0;
    // The grid: octaves across, passes down.
    let mut grid = Vec::new();
    for i in -2..=2 {
        let y = s.mid + f64::from(i) * s.b * 1.25;
        grid.push(vec![(s.x0 - 6.0, y), (w - 8.0, y)]);
    }
    for k in 0..PASSES {
        let x = s.x0 + k as f64 * s.s_max;
        grid.push(vec![(x, 8.0), (x, h - 8.0)]);
    }
    b.add(
        well,
        ui::strokes("grid", &grid, 0.5, draw::solid("#c4d0ff12")),
    );
    // The pitch axis.
    ui::line(
        b,
        well,
        "pitch axis",
        &[(s.x0 - 10.0, h - 10.0), (s.x0 - 10.0, top.max(14.0) - 2.0)],
        0.8,
        "#c4d0ff55",
    );
    ui::line(
        b,
        well,
        "arrow",
        &[
            (s.x0 - 13.0, top.max(14.0) + 2.0),
            (s.x0 - 10.0, top.max(14.0) - 2.0),
            (s.x0 - 7.0, top.max(14.0) + 2.0),
        ],
        0.8,
        "#c4d0ff55",
    );
    if !compact {
        ui::silk_left(
            b,
            well,
            "Each pass re-pitched",
            s.x0 - 4.0,
            top.max(14.0) - 4.0,
            5.5,
            "#c9d2f0",
            800.0,
        );
    }

    let spread = s.spread();
    let lh = 12.0;
    let base_len = s.s_min * 1.7;
    for k in 0..PASSES {
        let f = k as f64;
        let t = f / (PASSES as f64 - 1.0);
        let color = ui::blend(LIGHT_0, ICE, t * 0.7);
        let g = b.add(
            well,
            draw::frame(&format!("pass {k}"), s.x0 + f * s.s_min, 0.0, base_len, lh),
        );
        motion::add(
            b,
            g,
            Motion::new(Target::X)
                .input(format!(
                    "{:.2} * norm({VERB}/decay)",
                    f * (s.s_max - s.s_min)
                ))
                .range(0.0, 1.0)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
        motion::add(
            b,
            g,
            Motion::new(Target::Y)
                .input(format!("{} - {:.1}", s.y(f), lh / 2.0))
                .range(0.0, 1.0)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
        let fade = if k == 0 {
            format!("0.45 + 0.55 * param({VERB}/on)")
        } else {
            format!(
                "(0.05 + 0.95 * pow(norm({VERB}/shim_amount), {:.2}) * {:.2}) * (0.35 + 0.65 * param({VERB}/on))",
                0.4 * f,
                0.92_f64.powi(k as i32)
            )
        };
        motion::add(
            b,
            g,
            Motion::new(Target::Opacity).input(fade).range(0.0, 1.0),
        );
        // What stretches with the decay: the streak and the step to the
        // next pass.
        let st = b.add(g, draw::frame("stretch", 0.0, 0.0, base_len, lh));
        motion::add(
            b,
            st,
            Motion::new(Target::StretchX)
                .input(spread.clone())
                .range(0.0, 1.0)
                .pivot(0.0, 0.5)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
        // The pass: a streak, bright where it starts, ringing out.
        let streak = draw::rounded(
            draw::rect(
                "streak",
                0.0,
                lh / 2.0 - 2.0,
                base_len,
                4.0,
                draw::linear(
                    90.0,
                    &[
                        (0.0, "#ffffff"),
                        (0.12, &color),
                        (0.6, &ui::alpha(&color, 0.6)),
                        (1.0, &ui::alpha(&color, 0.0)),
                    ],
                ),
            ),
            2.0,
        );
        b.add(st, ui::glowing(streak, &color, 1.3, kit::GLOW));
        // Its sparkle: points of light along it, twinkling.
        let mut dots = String::new();
        for j in 0..7 {
            let u =
                0.12 + 0.11 * f64::from(j) + 0.03 * crate::reverb::jitter(f * 7.0 + f64::from(j));
            let dy = 4.5 * crate::reverb::jitter(f * 3.1 + f64::from(j) * 1.7);
            dots.push_str(&draw::regular(base_len * u, lh / 2.0 + dy, 0.9, 6, 0.0));
            dots.push(' ');
        }
        let sparkle = b.add(
            st,
            ui::glowing(
                ui::area("sparkle", dots, base_len, lh, draw::solid(ICE)),
                &color,
                1.0,
                kit::GLOW,
            ),
        );
        motion::add(
            b,
            sparkle,
            Motion::new(Target::Opacity)
                .input(format!(
                    "0.3 + 0.7 * pow(0.5 + 0.5 * sin(time * {:.1} + {f}), 3)",
                    2.3 + 0.7 * f
                ))
                .range(0.0, 1.0),
        );
        // Its partials, faint above it.
        let ghost = draw::rect(
            "partial",
            4.0,
            lh / 2.0 - 5.0,
            base_len * 0.7,
            1.0,
            draw::linear(
                90.0,
                &[
                    (0.0, &ui::alpha(&color, 0.6)),
                    (1.0, &ui::alpha(&color, 0.0)),
                ],
            ),
        );
        b.add(st, ghost);
        if k + 1 < PASSES {
            // The step up to the next pass, shaped by the shift: drawn at
            // −12, 0 and +12 and blended between.
            let span = s.b + 3.0;
            let mut n = ui::strokes(
                "step",
                &[vec![(0.0, 0.0), (s.s_min, 2.0 * span)]],
                0.9,
                draw::solid(&ui::alpha(ICE, 0.7)),
            );
            n.transform = frame::document::proto::Transform::translate(0.0, lh / 2.0 - span);
            n.size = frame::document::proto::Size {
                width: s.s_min,
                height: 2.0 * span,
            };
            let shapes: Vec<String> = [-12.0, 0.0, 12.0]
                .iter()
                .map(|sh: &f64| step_path(s.s_min, span, sh / 12.0 * s.b))
                .collect();
            n.fill_geometry = vec![shapes[2].clone()];
            let n = b.add(st, ui::glowing(n, LIGHT_0, 0.5, kit::GLOW));
            motion::add(
                b,
                n,
                Motion::new(Target::Morph { shapes })
                    .input(format!("(param({VERB}/shim_shift1) + 12) / 12"))
                    .range(0.0, 1.0),
            );
        }
    }
    // The glints: up the stairs, again and again while it plays.
    for (i, lag) in [0.0, 0.5].iter().enumerate() {
        let c = format!("fract(time * 0.32 + {lag})");
        let span = (PASSES as f64 - 1.0) * s.s_min;
        let glint = draw::path(
            "glint",
            -5.0,
            -5.0,
            10.0,
            10.0,
            "M 5 0 L 6 4 L 10 5 L 6 6 L 5 10 L 4 6 L 0 5 L 4 4 Z".into(),
            draw::solid(ICE),
        );
        let glint = b.add(well, ui::glowing(glint, LIGHT_0, 1.4, kit::GLOW));
        let x = format!("{:.2} + {c} * {span:.2} * {spread}", s.x0);
        let y = format!(
            "{:.2} + param({VERB}/shim_shift1) / 12 * ({:.2} - {c} * {:.2})",
            s.mid,
            s.a,
            (PASSES as f64 - 1.0) * s.b
        );
        motion::add(b, glint, Motion::new(Target::X).input(x).range(0.0, 1.0));
        motion::add(b, glint, Motion::new(Target::Y).input(y).range(0.0, 1.0));
        let fade = format!(
            "param({VERB}/on) * sin({c} * 3.1416) * (0.3 + 0.7 * norm({VERB}/shim_amount))"
        );
        motion::add(
            b,
            glint,
            Motion::new(Target::Opacity).input(fade).range(0.0, 1.0),
        );
        motion::add(
            b,
            glint,
            Motion::new(Target::Rotate)
                .input(c.clone())
                .range(0.0, 90.0 + 30.0 * i as f64)
                .pivot(0.5, 0.5),
        );
    }

    // The crystal at the head of the cascade: a handle on the shift (up,
    // down) and the decay (out).
    let last = PASSES as f64 - 1.0;
    let (x0, x1) = (s.x0 + last * s.s_min, s.x0 + last * s.s_max);
    let (y0, y1) = (s.mid - s.a + last * s.b, s.mid + s.a - last * s.b);
    let (ya, yb) = (y0.min(y1), y0.max(y1));
    let area = b.add(
        well,
        draw::frame("crystal travel", x0, ya, x1 - x0, yb - ya),
    );
    let hs = 22.0;
    let g = b.add(
        area,
        draw::frame(
            "Crystal",
            0.4 * (x1 - x0) - hs / 2.0,
            0.0 - hs / 2.0,
            hs,
            hs,
        ),
    );
    let c = hs / 2.0;
    let mut ring = draw::circle("ring", c, c, 8.5, draw::solid("#00000000"));
    ring.fills.clear();
    b.add(g, draw::stroked(ring, &ui::alpha(ICE, 0.55), 0.9));
    let gem = draw::path(
        "gem",
        c - 5.0,
        c - 7.0,
        10.0,
        14.0,
        "M 5 0 L 10 6 L 5 14 L 0 6 Z".into(),
        draw::linear(160.0, &[(0.0, "#ffffff"), (0.5, "#d9ccff"), (1.0, LIGHT_0)]),
    );
    b.add(g, ui::glowing(gem, LIGHT_0, 1.2, kit::BLOOM));
    ui::line(
        b,
        g,
        "facet",
        &[(c - 5.0, c - 1.0), (c + 5.0, c - 1.0)],
        0.6,
        "#ffffffaa",
    );
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: Some(ui::bind(&a("shim_shift1"))),
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear {
            min: -12.0,
            max: 12.0,
        }),
    };
    b.controls
        .insert(g, Control::new(kind).bound(ui::bind(&a("decay"))));
}

/// The picture in (`x`, `y`, `w`, `h`) on `panel`, the cascade from `top`
/// down (below a nameplate over the picture). Returns its well.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    top: f64,
    l: &Look,
) -> NodeId {
    let mut well = draw::frame("Cascade", x, y, w, h);
    well.fills = vec![draw::linear(180.0, &l.well)];
    well.clips_content = true;
    well.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
    well.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0)];
    let well = b.add(panel, well);
    // A haze of the climb's light, high where the cascade ends.
    b.add(
        well,
        draw::rect(
            "haze",
            0.0,
            0.0,
            w,
            h,
            draw::radial_at(
                0.85,
                0.15,
                0.9,
                &[
                    (0.0, &ui::alpha(LIGHT_0, 0.12)),
                    (1.0, &ui::alpha(LIGHT_0, 0.0)),
                ],
            ),
        ),
    );
    cascade(b, well, (w, h), top);
    kit::glass(b, well);
    well
}

/// The panel, `w` × `h`: frosted, an iridescent edge along its top.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.panel)];
    panel.corner_radii = frame::document::proto::CornerRadii::uniform(10.0);
    let panel = b.add(
        root,
        draw::with_effects(
            panel,
            vec![
                draw::inner_shadow("#ffffff40", 0.0, 1.0, 0.0),
                draw::inner_shadow("#00000040", 0.0, -2.0, 3.0),
            ],
        ),
    );
    b.add(
        panel,
        draw::rounded(
            draw::rect(
                "iridescence",
                10.0,
                2.0,
                w - 20.0,
                2.0,
                draw::linear(90.0, &l.edge),
            ),
            1.0,
        ),
    );
    b.add(
        panel,
        draw::rounded(
            draw::rect(
                "iridescence",
                10.0,
                h - 4.0,
                w - 20.0,
                2.0,
                draw::linear(90.0, &l.edge),
            ),
            1.0,
        ),
    );
    panel
}

/// The nameplate's title: wide and light.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let wide = draw::Lettering::family(draw::family::MICHROMA).weight(400.0);
    crate::suite_studio_blues::parts::set(b, t, NAME, 50.0, h / 2.0, size, ink, &wide, 0.0);
}

/// The nameplate's title width.
const TITLE_W: f64 = 112.0;

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the cascade beside the controls. On a phone the
/// knobs are Shimmer, Shift and Level.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l);
        let sw = if small_room {
            kit::PHONE_PICTURE_R - 10.0
        } else {
            540.0
        };
        let well = picture(b, panel, (10.0, 10.0, sw, kit::H - 20.0), 0.0, l);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (6.0, 30.0),
                TITLE_W,
                200.0,
                12.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 30.0, 13.0, l.ink),
            );
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                11.0,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in [
                ("shim_amount", "Shimmer"),
                ("shim_shift1", "Shift"),
                ("level", "Level"),
            ]
            .iter()
            .enumerate()
            {
                kit::knob3d(
                    b,
                    panel,
                    &a(id),
                    legend,
                    kd,
                    (xs[i], ky),
                    &l.knob,
                    l.ink,
                    l.dim,
                );
            }
        } else {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (10.0, 28.0),
                TITLE_W,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 28.0, 12.0, l.ink),
            );
            let x0 = 10.0 + sw + 18.0;
            let (tx, ty, td) = (x0 + 38.0, 78.0, 52.0);
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                8.5,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            for (i, (id, legend)) in FRONT.iter().enumerate() {
                kit::knob3d(
                    b,
                    panel,
                    &a(id),
                    legend,
                    32.0,
                    (x0 + 108.0 + 52.0 * i as f64, 90.0),
                    &l.knob,
                    l.ink,
                    l.dim,
                );
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "On");
        kit::flip_arrow(b, panel, VERB, tier, false, (l.ink, &ui::alpha(l.dim, 0.3)));
        kit::responsive(b, panel, &[well]);
    });
    let mut f = kit::reverb_face(kit::tier_file(FILE, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the panel, no cascade — the advanced controls
/// across it, and (standing alone) the nameplate, the bypass and the
/// arrow.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l);
        kit::back_knobs(
            b,
            panel,
            tier,
            &kit::VERB_MORE,
            l.ink,
            |b, p, id, legend, d, at| {
                kit::knob3d(b, p, &a(id), legend, d, at, &l.knob, l.ink, l.dim);
            },
        );
        let (np, size, ts) = if small_room {
            ((6.0, 30.0), 12.0, 13.0)
        } else {
            ((10.0, 28.0), 10.0, 12.0)
        };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                TITLE_W,
                200.0,
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, np.1, ts, l.ink),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "On");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file(BACK, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the cascade across the top, every control under it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let panel = ground(b, root, (kit::TALL_W, kit::TALL_H), l);
        picture(
            b,
            panel,
            (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0),
            44.0,
            l,
        );
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("shim_amount", "Shimmer"),
                ("shim_shift1", "Shift"),
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("level", "Level"),
            ],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", "On"),
            title_w: TITLE_W,
        };
        kit::tall_chrome_rows(
            b,
            panel,
            &t,
            ROWS,
            |b, p, addr, legend, d, at| {
                kit::knob3d(b, p, addr, legend, d, at, &l.knob, l.ink, l.dim);
            },
            |b, p, addr, (cx, cy, d)| {
                kit::knob3d_bare(b, p, addr, d, (cx, cy), &l.wheel);
            },
            |b, tn| title(b, tn, 30.0, 13.0, l.ink),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file(FILE, dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
