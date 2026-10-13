//! Room: a studio's live room, drawn in plan on the acoustician's sheet —
//! its four walls, the absorbers on the side walls and the diffuser on the
//! back, a source (the drum) and a listener (the mic). The early
//! reflections are traced the way an acoustician traces them, by image
//! sources: each first-order path off one wall bright, the second-order
//! paths off two walls fainter, coming up with the decay. A wavefront
//! leaves the source again and again and rings round the walls, slower to
//! die the longer the decay. The plan keeps its size — the room's size is
//! in time, not on the sheet: beside the plan, where there is room, the
//! same reflections over time, the tap diagram, its taps spreading as the
//! room grows.
//!
//! Two finishes: light, birch ply round a slate sheet; dark, charcoal
//! felt.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const NAME: &str = "Room";
/// The tall face's rows: a shorter sheet, bigger knobs under it.
const ROWS: kit::TallRows = kit::TallRows {
    split: 134.0,
    main: (30.0, 50.0),
    more: (126.0, 40.0),
};
/// The reflections' light: the section's purple.
const RAY: &str = kit::PURPLE_LAMP;

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    panel: [(f64, &'static str); 3],
    grain: &'static str,
    sheet: [(f64, &'static str); 2],
    wall: &'static str,
    line: &'static str,
    knob: [&'static str; 6],
    wheel: [&'static str; 5],
    accent: &'static str,
    scrim: &'static str,
}

const LIGHT: Look = Look {
    ink: "#2b2118",
    dim: "#76675a",
    panel: [(0.0, "#eee3cf"), (0.6, "#e0d0b2"), (1.0, "#d2bf9c")],
    grain: "#7a5a3014",
    sheet: [(0.0, "#1b1e24"), (1.0, "#12141a")],
    wall: "#d9d2c6",
    line: "#8f8a99",
    knob: [
        "color=#f4efe6",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#8a6d4a",
    ],
    wheel: [
        "color=#2a2420",
        "pointer=#efe9ff",
        "index=dot",
        "skirt=plain",
        "skirt_color=#4a3b2e",
    ],
    accent: "#6a3fc4",
    scrim: "#e7d9bfe6",
};

const DARK: Look = Look {
    ink: "#ece6dc",
    dim: "#9a9087",
    panel: [(0.0, "#2c2926"), (0.6, "#221f1d"), (1.0, "#181614")],
    grain: "#ffffff08",
    sheet: [(0.0, "#16181d"), (1.0, "#0d0e12")],
    wall: "#bdb6aa",
    line: "#77728a",
    knob: [
        "color=#2f2b28",
        "pointer=#c4b5fd",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#0e0d0c",
    ],
    wheel: [
        "color=#1b1917",
        "pointer=#d9c8ff",
        "index=dot",
        "skirt=plain",
        "skirt_color=#0b0a09",
    ],
    accent: kit::PURPLE_LAMP,
    scrim: "#221f1de6",
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

type P = (f64, f64);

/// The room's inside, in its plan's coordinates: (left, top, right,
/// bottom), and where the source and the listener stand.
#[derive(Clone, Copy)]
struct Plan {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
    src: P,
    ear: P,
}

/// The four walls: the coordinate each lies on (left, top, right,
/// bottom), and whether it is upright.
const WALLS: [(usize, bool); 4] = [(0, true), (1, false), (2, true), (3, false)];

impl Plan {
    fn new(w: f64, h: f64) -> Self {
        let (l, t, r, b) = (w * 0.08, h * 0.14, w * 0.92, h * 0.88);
        Self {
            l,
            t,
            r,
            b,
            src: (l + (r - l) * 0.28, t + (b - t) * 0.62),
            ear: (l + (r - l) * 0.74, t + (b - t) * 0.38),
        }
    }

    const fn wall(&self, i: usize) -> f64 {
        match i {
            0 => self.l,
            1 => self.t,
            2 => self.r,
            _ => self.b,
        }
    }

    /// `p` mirrored in wall `i`: its image source.
    fn mirror(&self, p: P, (i, upright): (usize, bool)) -> P {
        let c = self.wall(i);
        if upright {
            (2.0 * c - p.0, p.1)
        } else {
            (p.0, 2.0 * c - p.1)
        }
    }

    /// Where the line from `from` to `to` crosses wall `i`.
    fn hit(&self, from: P, to: P, (i, upright): (usize, bool)) -> P {
        let c = self.wall(i);
        if upright {
            let k = (c - from.0) / (to.0 - from.0);
            (c, from.1 + (to.1 - from.1) * k)
        } else {
            let k = (c - from.1) / (to.1 - from.1);
            (from.0 + (to.0 - from.0) * k, c)
        }
    }

    /// Whether `q` lies on the room's walls (not off their ends).
    fn on_wall(&self, q: P) -> bool {
        let e = 0.5;
        q.0 >= self.l - e && q.0 <= self.r + e && q.1 >= self.t - e && q.1 <= self.b + e
    }

    /// The first-order paths: source, the bounce, the listener.
    fn first(&self) -> Vec<Vec<P>> {
        WALLS
            .iter()
            .map(|w| {
                vec![
                    self.src,
                    self.hit(self.mirror(self.src, *w), self.ear, *w),
                    self.ear,
                ]
            })
            .collect()
    }

    /// The second-order paths, off two walls that face each other or meet.
    fn second(&self) -> Vec<Vec<P>> {
        let pairs = [
            (0, 2),
            (2, 0),
            (1, 3),
            (3, 1),
            (0, 1),
            (2, 3),
            (1, 2),
            (3, 0),
        ];
        pairs
            .iter()
            .map(|(i, j)| {
                let (wa, wb) = (WALLS[*i], WALLS[*j]);
                let sa = self.mirror(self.src, wa);
                let sab = self.mirror(sa, wb);
                let p2 = self.hit(sab, self.ear, wb);
                let p1 = self.hit(sa, p2, wa);
                vec![self.src, p1, p2, self.ear]
            })
            // Only the paths that really bounce off the walls' lengths.
            .filter(|path| path[1..3].iter().all(|q| self.on_wall(*q)))
            .collect()
    }
}

/// A path's length.
fn length(p: &[P]) -> f64 {
    p.windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum()
}

/// The wavefront, clipped to the room's inside: it leaves the source and
/// rings round the walls, dying slower the longer the decay.
fn wavefront(b: &mut Builder<'_>, room: NodeId, p: &Plan) {
    let mut inside = draw::frame("Inside", p.l, p.t, p.r - p.l, p.b - p.t);
    inside.clips_content = true;
    let inside = b.add(room, inside);
    b.add(
        inside,
        draw::rect(
            "floor",
            0.0,
            0.0,
            p.r - p.l,
            p.b - p.t,
            draw::linear(180.0, &[(0.0, "#ffffff07"), (1.0, "#ffffff02")]),
        ),
    );
    let reach = (p.r - p.l).max(p.b - p.t) * 1.2;
    for (lag, strength) in [(0.0, 1.0), (0.5, 0.6)] {
        let ring = draw::stroked(
            draw::circle(
                "wavefront",
                p.src.0 - p.l,
                p.src.1 - p.t,
                reach,
                draw::solid("#00000000"),
            ),
            RAY,
            // Scaled with the ring: a hairline near the source that
            // broadens as it spreads.
            4.0,
        );
        // A ring, not a disc: its light is its stroke's alone.
        let mut ring = ring;
        ring.fills.clear();
        let ring = b.add(inside, ui::glowing(ring, RAY, 1.2, kit::BLOOM));
        let phase = format!("fract(time * 0.7 + {lag})");
        motion::add(
            b,
            ring,
            Motion::new(Target::Scale)
                .input(phase.clone())
                .range(0.02, 1.0)
                .pivot(0.5, 0.5),
        );
        let fade = format!(
            "pow(1 - {phase}, 1 + 4 * (1 - norm({VERB}/decay))) * param({VERB}/on) * {strength}"
        );
        motion::add(
            b,
            ring,
            Motion::new(Target::Opacity).input(fade).range(0.0, 0.9),
        );
    }
}

/// The walls, the treatment on them, and the door.
fn walls(b: &mut Builder<'_>, room: NodeId, p: &Plan, l: &Look) {
    let t = 5.0;
    for (x, y, ww, hh) in [
        (p.l - t, p.t - t, p.r - p.l + 2.0 * t, t),
        (p.l - t, p.b, p.r - p.l + 2.0 * t, t),
        (p.l - t, p.t, t, p.b - p.t),
        (p.r, p.t, t, p.b - p.t),
    ] {
        b.add(room, draw::rect("wall", x, y, ww, hh, draw::solid(l.wall)));
    }
    let (rw, rh) = (p.r - p.l, p.b - p.t);
    let panel = ui::alpha(l.line, 0.85);
    for k in 0..3 {
        let y = p.t + rh * (0.12 + 0.3 * f64::from(k));
        b.add(
            room,
            draw::rect("absorber", p.l, y, 3.0, rh * 0.16, draw::solid(&panel)),
        );
        b.add(
            room,
            draw::rect(
                "absorber",
                p.r - 3.0,
                y + rh * 0.08,
                3.0,
                rh * 0.16,
                draw::solid(&panel),
            ),
        );
    }
    let wells: Vec<Vec<P>> = (0..9)
        .map(|k| {
            let x = p.l + rw * (0.3 + 0.05 * f64::from(k));
            let d = 2.0 + 3.0 * crate::reverb::jitter(f64::from(k) * 2.3).abs();
            vec![(x, p.t), (x, p.t + d)]
        })
        .collect();
    b.add(
        room,
        ui::strokes("diffuser", &wells, 2.4, draw::solid(&panel)),
    );
    b.add(
        room,
        draw::rect(
            "door",
            p.l + rw * 0.06,
            p.b,
            rw * 0.12,
            t,
            draw::solid("#0d0e12"),
        ),
    );
    let swing: Vec<P> = (0..=12)
        .map(|k| draw::polar(p.l + rw * 0.06, p.b, rw * 0.12, 90.0 * f64::from(k) / 12.0))
        .collect();
    ui::line(b, room, "door swing", &swing, 0.6, &ui::alpha(l.wall, 0.4));
}

/// The room in plan in `sheet` (`w × h`), filling it.
fn plan(b: &mut Builder<'_>, sheet: NodeId, (w, h): (f64, f64), l: &Look) {
    // The sheet's grid.
    let mut grid = Vec::new();
    let mut x = 6.0;
    while x < w {
        grid.push(vec![(x, 0.0), (x, h)]);
        x += 14.0;
    }
    let mut y = 6.0;
    while y < h {
        grid.push(vec![(0.0, y), (w, y)]);
        y += 14.0;
    }
    b.add(
        sheet,
        ui::strokes("grid", &grid, 0.5, draw::solid("#ffffff08")),
    );
    let p = Plan::new(w, h);
    let room = b.add(sheet, draw::frame("Room", 0.0, 0.0, w, h));
    wavefront(b, room, &p);
    // The paths: second order under, fainter, up with the decay.
    let second = b.add(
        room,
        ui::glowing(
            ui::strokes(
                "second order",
                &p.second(),
                0.9,
                draw::solid(&ui::alpha(RAY, 0.55)),
            ),
            RAY,
            0.45,
            kit::GLOW,
        ),
    );
    motion::add(
        b,
        second,
        Motion::new(Target::Opacity)
            .input(format!(
                "(0.1 + 0.8 * norm({VERB}/decay)) * (0.3 + 0.7 * param({VERB}/on))"
            ))
            .range(0.0, 1.0)
            .dynamics(Dynamics::spring(200.0, 26.0)),
    );
    let first = b.add(
        room,
        ui::glowing(
            ui::strokes("first order", &p.first(), 1.3, draw::solid(RAY)),
            RAY,
            1.1,
            kit::GLOW,
        ),
    );
    // Its light comes up with the decay: a live room rings brighter.
    motion::add(
        b,
        first,
        Motion::new(Target::Opacity)
            .input(format!(
                "(0.35 + 0.65 * param({VERB}/on)) * (0.7 + 0.3 * norm({VERB}/decay))"
            ))
            .range(0.0, 1.0),
    );
    ui::line(
        b,
        room,
        "direct",
        &[p.src, p.ear],
        0.9,
        &ui::alpha(l.wall, 0.45),
    );
    for path in p.first() {
        let (x, y) = path[1];
        b.add(
            room,
            ui::glowing(
                draw::circle("bounce", x, y, 2.2, draw::solid(RAY)),
                RAY,
                1.3,
                kit::GLOW,
            ),
        );
    }
    walls(b, room, &p, l);
    // The source and the listener.
    // The drum sounds: it is the room's light source.
    let drum = b.add(
        room,
        ui::glowing(
            draw::circle(
                "drum",
                p.src.0,
                p.src.1,
                6.0,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#f3ecff"), (1.0, "#8b6fd1")]),
            ),
            RAY,
            0.9,
            kit::BLOOM,
        ),
    );
    motion::add(
        b,
        drum,
        Motion::new(Target::Opacity)
            .input(format!("0.55 + 0.45 * param({VERB}/on)"))
            .range(0.0, 1.0),
    );
    b.add(
        room,
        draw::stroked(
            draw::circle("drum rim", p.src.0, p.src.1, 6.0, draw::solid("#00000000")),
            "#1a1424",
            1.2,
        ),
    );
    ui::line(
        b,
        room,
        "mic stand",
        &[p.ear, (p.ear.0 + 9.0, p.ear.1 + 9.0)],
        1.2,
        "#f3ecff",
    );
    b.add(
        room,
        ui::glowing(
            draw::circle("mic", p.ear.0, p.ear.1, 3.2, draw::solid("#f3ecff")),
            RAY,
            1.0,
            kit::GLOW,
        ),
    );
}

/// The tap diagram in `at` (x, y, w, h) on `sheet` for a plan `plan_wh`:
/// the direct sound and each reflection at its path's time, the
/// first-order taps bright, the second-order ones up with the decay; the
/// taps spread as the room grows.
fn taps(
    b: &mut Builder<'_>,
    sheet: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    plan_wh: (f64, f64),
    l: &Look,
) {
    let g = b.add(sheet, draw::frame("Taps", x, y, w, h));
    let base = h - 14.0;
    ui::line(
        b,
        g,
        "axis",
        &[(0.0, base), (w, base)],
        0.8,
        &ui::alpha(l.line, 0.8),
    );
    for k in 0..=4 {
        let tx = w * f64::from(k) / 4.0;
        ui::line(
            b,
            g,
            "tick",
            &[(tx, base), (tx, base + 3.0)],
            0.8,
            &ui::alpha(l.line, 0.8),
        );
    }
    ui::silk_left(b, g, "Early reflections", 0.0, 8.0, 5.5, l.wall, 800.0);
    ui::silk_left(b, g, "0", 0.0, base + 9.0, 5.0, l.line, 700.0);
    ui::silk_right(b, g, "80 ms", w, base + 9.0, 5.0, l.line, 700.0);
    let p = Plan::new(plan_wh.0, plan_wh.1);
    let direct = length(&[p.src, p.ear]);
    let longest = p.second().iter().map(|q| length(q)).fold(direct, f64::max) * 1.05;
    let mut all: Vec<(f64, f64, bool)> = vec![(direct, 1.0, true)];
    all.extend(p.first().iter().map(|q| (length(q), 0.78, true)));
    all.extend(p.second().iter().map(|q| (length(q), 0.5, false)));
    for (i, (len, hgt, bright)) in all.iter().enumerate() {
        let tall = (base - 16.0) * hgt * (0.9 + 0.1 * crate::reverb::jitter(i as f64));
        let fill = if *bright {
            RAY.to_string()
        } else {
            ui::alpha(RAY, 0.7)
        };
        let bar = b.add(g, draw::frame("tap", -1.0, base - tall, 2.4, tall));
        let line = draw::rect("line", 0.0, 0.0, 2.4, tall, draw::solid(&fill));
        b.add(
            bar,
            if *bright {
                ui::glowing(line, RAY, 1.0, kit::GLOW)
            } else {
                line
            },
        );
        // The room's size stretches the times (the bars keep their width).
        let xpos = format!(
            "{:.2} * (0.66 + 0.34 * norm({VERB}/size))",
            len / longest * w
        );
        motion::add(
            b,
            bar,
            Motion::new(Target::X)
                .input(xpos)
                .range(0.0, 1.0)
                .dynamics(Dynamics::spring(160.0, 24.0)),
        );
        if !*bright {
            motion::add(
                b,
                bar,
                Motion::new(Target::StretchY)
                    .input(format!("0.1 + 0.9 * norm({VERB}/decay)"))
                    .range(0.0, 1.0)
                    .pivot(0.5, 1.0)
                    .dynamics(Dynamics::spring(200.0, 26.0)),
            );
        }
    }
}

/// The picture in `at` (x, y, w, h) on `panel`: the sheet, the plan and,
/// when it is wide, the taps beside it, from `taps_top` down (below a
/// nameplate over the sheet's top right). Returns the sheet.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    taps_top: f64,
    l: &Look,
) -> NodeId {
    let mut sheet = draw::frame("Sheet", x, y, w, h);
    sheet.fills = vec![draw::linear(180.0, &l.sheet)];
    sheet.clips_content = true;
    sheet.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
    sheet.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0)];
    let sheet = b.add(panel, sheet);
    let wide = w > 360.0;
    let pw = if wide { (h * 1.9).min(w * 0.55) } else { w };
    let room = b.add(sheet, draw::frame("Plan", 0.0, 0.0, pw, h));
    plan(b, room, (pw, h), l);
    if wide {
        taps(
            b,
            sheet,
            (pw + 16.0, taps_top, w - pw - 30.0, h - taps_top - 6.0),
            (pw, h),
            l,
        );
    }
    ui::silk_left(b, sheet, "Live room · plan", 8.0, 9.0, 5.5, l.wall, 800.0);
    kit::glass(b, sheet);
    sheet
}

/// The panel, `w` × `h`: ply (or felt) with its grain.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.panel)];
    panel.corner_radii = frame::document::proto::CornerRadii::uniform(8.0);
    let panel = b.add(
        root,
        draw::with_effects(
            panel,
            vec![
                draw::inner_shadow("#ffffff30", 0.0, 1.0, 0.0),
                draw::inner_shadow("#00000050", 0.0, -2.0, 3.0),
            ],
        ),
    );
    let rows = (h / 3.0).floor();
    let mut grain = Vec::new();
    let mut i = 0.0;
    while i < rows {
        let y = 1.0 + i * 3.0 + crate::reverb::jitter(i) * 0.8;
        grain.push(vec![
            (0.0, y),
            (w, y + crate::reverb::jitter(i * 1.7) * 2.0),
        ]);
        i += 1.0;
    }
    b.add(
        panel,
        ui::strokes("grain", &grain, 0.8, draw::solid(l.grain)),
    );
    panel
}

/// The nameplate's title.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
    crate::suite_studio_blues::parts::set(b, t, NAME, 30.0, h / 2.0, size, ink, &sans, 0.02);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the sheet beside the controls. On a phone the sheet
/// is the plan alone and the knobs are Size and Level.
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
            560.0
        };
        let sheet = picture(b, panel, (10.0, 10.0, sw, kit::H - 20.0), 12.0, l);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (6.0, 30.0),
                84.0,
                220.0,
                12.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 30.0, 22.0, l.ink),
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
            for (i, (id, legend)) in [("size", "Size"), ("level", "Level")].iter().enumerate() {
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
                80.0,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 28.0, 20.0, l.ink),
            );
            let x0 = 10.0 + sw + 20.0;
            let (tx, ty, td) = (x0 + 40.0, 78.0, 52.0);
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
            for (i, (id, legend)) in [
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
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
                    34.0,
                    (x0 + 116.0 + 60.0 * i as f64, 90.0),
                    &l.knob,
                    l.ink,
                    l.dim,
                );
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "On");
        kit::flip_arrow(b, panel, VERB, tier, false, (l.ink, &ui::alpha(l.dim, 0.3)));
        kit::responsive(b, panel, &[sheet]);
    });
    let mut f = kit::reverb_face(kit::tier_file("80-verb-room", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the panel, no sheet — the advanced controls across
/// it, and (standing alone) the nameplate, the bypass and the arrow.
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
        let (np, title_w, preset_w, size, ts) = if small_room {
            ((6.0, 30.0), 84.0, 220.0, 12.0, 22.0)
        } else {
            ((10.0, 28.0), 80.0, 200.0, 10.0, 20.0)
        };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                title_w,
                preset_w,
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, np.1, ts, l.ink),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "On");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file("80-verb-room-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the sheet — the plan and the taps under the nameplate —
/// across the top, shorter than half so the controls below it get big
/// knobs.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let panel = ground(b, root, (kit::TALL_W, kit::TALL_H), l);
        picture(
            b,
            panel,
            (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0),
            50.0,
            l,
        );
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("damping", "HF Damp"),
                ("level", "Level"),
            ],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", "On"),
            title_w: 84.0,
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
            |b, tn| title(b, tn, 30.0, 22.0, l.ink),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file("80-verb-room", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
