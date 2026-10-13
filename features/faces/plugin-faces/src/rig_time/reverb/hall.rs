//! Hall: the room itself, drawn as its architect did — a shoebox concert
//! hall in plan, on a blueprint: the stage and its apron at one end, the
//! stalls in rows either side of the centre aisle, the balconies along the
//! long walls, the doors at the back. The sound leaves the stage and fills
//! it: wavefronts spread from the source and ring through the hall, slower
//! to die away the longer the decay; the hall itself grows with the size.
//!
//! Beside the drawing, the house: velvet and gilt, the name in a theatre's
//! serif, and the controls.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

/// Blueprint ink and paper.
const LINE: &str = "#cdb8ff";

/// One finish's colours.
struct Look {
    house: [(f64, &'static str); 3],
    pile: &'static str,
    paper: (&'static str, &'static str),
    line: &'static str,
    sheet_shadow: &'static str,
    title: &'static str,
    ink: &'static str,
    dim: &'static str,
    small: [&'static str; 6],
}

const DARK: Look = Look {
    house: [(0.0, "#3d1b63"), (0.55, "#281043"), (1.0, "#180929")],
    pile: "#ffffff07",
    paper: ("#1d1340", "#110b28"),
    line: LINE,
    sheet_shadow: "#000000b0",
    title: GILT,
    ink: "#ecdcbc",
    dim: "#b39c77",
    small: [
        "color=#1a0a0e",
        "pointer=#d9b56a",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#a8874a",
    ],
};

const LIGHT: Look = Look {
    house: [(0.0, "#f8f3ea"), (0.55, "#efe6d6"), (1.0, "#e2d5be")],
    pile: "#5a3a8a0a",
    paper: ("#f7f3fe", "#e9e1f8"),
    line: "#6b4bc4",
    sheet_shadow: "#3b1d6e40",
    title: "#4c2a85",
    ink: "#3a2a4f",
    dim: "#7a6a8c",
    small: [
        "color=#2a1840",
        "pointer=#d9b56a",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#a8874a",
    ],
};
/// The house: velvet and gilt.
const GILT: &str = "#d9b56a";
const GILT_DIM: &str = "#a8874a";

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// Open strokes in `color`.
fn lines(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    segs: &[Vec<(f64, f64)>],
    weight: f64,
    color: &str,
) -> NodeId {
    b.add(parent, ui::strokes(name, segs, weight, draw::solid(color)))
}

/// The hall in plan, in `plan` (`w × h`): drawn at full size, squeezed
/// toward the stage by the size motion on its group.
#[allow(clippy::too_many_lines)]
fn hall(b: &mut Builder<'_>, plan: NodeId, w: f64, h: f64, line: &str) {
    // Faint drafting grid over the whole sheet.
    let mut grid: Vec<Vec<(f64, f64)>> = (1..(w / 14.0) as i32)
        .map(|i| vec![(f64::from(i) * 14.0, 0.0), (f64::from(i) * 14.0, h)])
        .collect();
    grid.extend(
        (1..(h / 14.0) as i32).map(|i| vec![(0.0, f64::from(i) * 14.0), (w, f64::from(i) * 14.0)]),
    );
    lines(b, plan, "grid", &grid, 0.5, &ui::alpha(line, 0.08));

    // The building: grows with the size, the stage end fixed.
    let (x0, y0, hh) = (18.0, 18.0, h - 52.0);
    let len = w - 36.0;
    let g = b.add(plan, draw::frame("Hall", x0, y0, len, hh));
    motion::add(
        b,
        g,
        Motion::new(Target::StretchX)
            .input(format!("norm({VERB}/size)"))
            .range(0.62, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(260.0, 28.0)),
    );
    motion::add(
        b,
        g,
        Motion::new(Target::StretchY)
            .input(format!("norm({VERB}/size)"))
            .range(0.82, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(260.0, 28.0)),
    );
    // Walls (double line: masonry), the stage end squared off.
    let wall = |b: &mut Builder<'_>, inset: f64, weight: f64, alpha: f64| {
        let r = draw::stroked(
            draw::rect(
                "wall",
                inset,
                inset,
                len - 2.0 * inset,
                hh - 2.0 * inset,
                draw::solid("#00000000"),
            ),
            &ui::alpha(line, alpha),
            weight,
        );
        let mut r = r;
        r.fills.clear();
        b.add(g, r);
    };
    wall(b, 0.0, 1.8, 0.95);
    wall(b, 4.0, 0.8, 0.55);
    // The stage and its curved apron.
    let stage_w = len * 0.16;
    let mut stage = draw::rect(
        "stage",
        4.0,
        4.0,
        stage_w,
        hh - 8.0,
        draw::solid(&ui::alpha(line, 0.1)),
    );
    stage.stroke = document_proto::Stroke {
        paints: vec![draw::solid(&ui::alpha(line, 0.7))],
        weight: 1.0,
        ..document_proto::Stroke::default()
    };
    b.add(g, stage);
    let apron: Vec<(f64, f64)> = (0..=24)
        .map(|i| {
            let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / 24.0;
            (
                4.0 + stage_w + t.cos() * hh * 0.12,
                hh / 2.0 + t.sin() * (hh / 2.0 - 12.0),
            )
        })
        .collect();
    ui::line(b, g, "apron", &apron, 1.0, &ui::alpha(line, 0.7));
    ui::silk(
        b,
        g,
        "Stage",
        4.0 + stage_w / 2.0,
        16.0,
        6.0,
        &ui::alpha(line, 0.8),
        800.0,
    );
    // The stalls: rows either side of the centre aisle, gently curved to
    // face the stage.
    let (sx0, sx1) = (stage_w + hh * 0.16 + 14.0, len - 34.0);
    let aisle = 7.0;
    let mut seats = Vec::new();
    let rows = ((sx1 - sx0) / 9.0) as i32;
    for r in 0..rows {
        let x = sx0 + f64::from(r) * 9.0;
        // Each row a shallow arc about the stage: its ends lean back.
        let bow = 4.0;
        let half = |y0: f64, y1: f64| -> Vec<(f64, f64)> {
            (0..=6)
                .map(|k| {
                    let y = y0 + (y1 - y0) * f64::from(k) / 6.0;
                    let off = ((y - hh / 2.0) / (hh / 2.0)).powi(2) * bow;
                    (x + off, y)
                })
                .collect()
        };
        seats.push(half(16.0, hh / 2.0 - aisle));
        seats.push(half(hh / 2.0 + aisle, hh - 16.0));
    }
    lines(b, g, "stalls", &seats, 1.1, &ui::alpha(line, 0.45));
    // Balconies along the long walls, and the side aisles under them.
    for y in [10.0, hh - 10.0] {
        lines(
            b,
            g,
            "balcony",
            &[vec![(sx0 - 6.0, y), (len - 10.0, y)]],
            2.2,
            &ui::alpha(line, 0.35),
        );
    }
    // Doors at the back: gaps with their swing arcs.
    for y in [hh * 0.3, hh * 0.7] {
        let arc: Vec<(f64, f64)> = (0..=8)
            .map(|i| {
                let t = std::f64::consts::FRAC_PI_2 * f64::from(i) / 8.0;
                (len - t.sin() * 9.0, y - 9.0 + t.cos() * 9.0)
            })
            .collect();
        ui::line(b, g, "door", &arc, 0.8, &ui::alpha(line, 0.6));
    }
    // Dimension line along the bottom.
    let dy = -9.0;
    lines(
        b,
        g,
        "dimension",
        &[
            vec![(0.0, dy), (len, dy)],
            vec![(0.0, dy - 3.0), (0.0, dy + 3.0)],
            vec![(len, dy - 3.0), (len, dy + 3.0)],
        ],
        0.7,
        &ui::alpha(line, 0.55),
    );

    // ── The sound: a source on the stage, and its wavefronts ──────────
    let (cx, cy) = (x0 + 4.0 + stage_w * 0.55, y0 + hh / 2.0);
    let mut room = draw::frame("Air", x0 + 5.0, y0 + 5.0, len - 10.0, hh - 10.0);
    room.clips_content = true;
    let room = b.add(plan, room);
    // The air shares the hall's size.
    motion::add(
        b,
        room,
        Motion::new(Target::StretchX)
            .input(format!("norm({VERB}/size)"))
            .range(0.62, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(260.0, 28.0)),
    );
    motion::add(
        b,
        room,
        Motion::new(Target::StretchY)
            .input(format!("norm({VERB}/size)"))
            .range(0.82, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(260.0, 28.0)),
    );
    let r_max = len * 1.05;
    let (lx, ly) = (cx - (x0 + 5.0), cy - (y0 + 5.0));
    for i in 0..5 {
        let ring = b.add(
            room,
            draw::frame(
                &format!("wavefront {i}"),
                lx - r_max,
                ly - r_max,
                2.0 * r_max,
                2.0 * r_max,
            ),
        );
        let mut c = draw::stroked(
            draw::circle("front", r_max, r_max, r_max - 3.0, draw::solid("#00000000")),
            line,
            5.0,
        );
        c.fills.clear();
        b.add(ring, ui::glowing(c, line, 0.6, kit::GLOW));
        // Each front spreads from the source and fades; they come slower
        // and ring longer as the decay grows.
        let phase = format!(
            "fract(time * lerp(0.55, 0.16, norm({VERB}/decay)) + {})",
            f64::from(i) / 5.0
        );
        motion::add(
            b,
            ring,
            Motion::new(Target::StretchX)
                .input(phase.clone())
                .range(0.02, 1.0)
                .pivot(0.5, 0.5),
        );
        motion::add(
            b,
            ring,
            Motion::new(Target::StretchY)
                .input(phase.clone())
                .range(0.02, 1.0)
                .pivot(0.5, 0.5),
        );
        let fade = format!("(1 - {phase}) * param({VERB}/on)");
        motion::add(
            b,
            ring,
            Motion::new(Target::Opacity).input(fade).range(0.0, 1.0),
        );
    }
    // The sound field: a pool of the stage's light in the air, filling
    // the hall as the decay grows (the air's clip keeps it inside the
    // walls). Drawn, not emitted: a field this size has no outline to
    // bloom from, and the vector path would halo it as a disc.
    let pool = (len * 0.42, hh * 0.7);
    let mut field = draw::circle(
        "field",
        lx,
        ly,
        pool.1,
        draw::radial_at(
            0.5,
            0.5,
            0.5,
            &[
                (0.0, &ui::alpha(line, 0.42)),
                (0.45, &ui::alpha(line, 0.15)),
                (1.0, &ui::alpha(line, 0.0)),
            ],
        ),
    );
    field.transform.tx = lx - pool.0;
    field.size.width = 2.0 * pool.0;
    let field = b.add(room, field);
    motion::add(
        b,
        field,
        Motion::new(Target::Opacity)
            .input(format!(
                "(0.25 + 0.75 * norm({VERB}/decay)) * param({VERB}/on)"
            ))
            .range(0.0, 1.0)
            .dynamics(Dynamics::spring(200.0, 26.0)),
    );
    motion::add(
        b,
        field,
        Motion::new(Target::StretchX)
            .input(format!("norm({VERB}/decay)"))
            .range(0.55, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(200.0, 26.0)),
    );
    // The source.
    let src = b.add(
        plan,
        draw::emissive(
            draw::circle("source", cx, cy, 4.5, draw::solid(GILT)),
            draw::hex(GILT),
            1.3,
            kit::BLOOM,
        ),
    );
    motion::add(
        b,
        src,
        Motion::new(Target::Opacity)
            .input(format!("0.35 + 0.65 * param({VERB}/on)"))
            .range(0.0, 1.0),
    );

    // Title block, as a drawing has — where the sheet is wide enough.
    if w < 260.0 {
        return;
    }
    let (tx, ty) = (w - 212.0, h - 28.0);
    let mut tb = draw::rect("title block", tx, ty, 194.0, 20.0, draw::solid("#00000000"));
    tb.stroke = document_proto::Stroke {
        paints: vec![draw::solid(&ui::alpha(line, 0.6))],
        weight: 0.8,
        ..document_proto::Stroke::default()
    };
    tb.fills.clear();
    b.add(plan, tb);
    ui::silk_left(
        b,
        plan,
        "Concert Hall · Plan",
        tx + 6.0,
        ty + 10.0,
        6.5,
        line,
        800.0,
    );
    ui::silk_left(
        b,
        plan,
        "1 : 200 · A-2",
        tx + 128.0,
        ty + 10.0,
        5.5,
        &ui::alpha(line, 0.7),
        700.0,
    );
}

/// The house, `w` × `h`: velvet (or ivory) in a gilt edge, its pile from
/// `pile_x` on. Returns it.
fn house(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), pile_x: f64, l: &Look) -> NodeId {
    let mut panel = draw::frame("House", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.house)];
    panel.corner_radii = document_proto::CornerRadii::uniform(6.0);
    panel.stroke = document_proto::Stroke {
        paints: vec![draw::solid(GILT_DIM)],
        weight: 2.0,
        ..document_proto::Stroke::default()
    };
    let panel = b.add(root, panel);
    let n = ((w - pile_x) / 4.4).max(0.0).floor();
    let pile: Vec<Vec<(f64, f64)>> = (0..n as u32)
        .map(|i| {
            let x = pile_x + f64::from(i) * 4.4;
            vec![(x, 4.0), (x, h - 4.0)]
        })
        .collect();
    b.add(panel, ui::strokes("pile", &pile, 1.6, draw::solid(l.pile)));
    panel
}

/// The drawing in (`px`, `py`, `pw`, `ph`) on `panel`, in its gilt frame.
/// Returns the sheet.
fn drawing(
    b: &mut Builder<'_>,
    panel: NodeId,
    (px, py, pw, ph): (f64, f64, f64, f64),
    l: &Look,
) -> NodeId {
    let mut sheet = draw::frame("Blueprint", px, py, pw, ph);
    sheet.fills = vec![draw::linear(160.0, &[(0.0, l.paper.0), (1.0, l.paper.1)])];
    sheet.clips_content = true;
    sheet.corner_radii = document_proto::CornerRadii::uniform(3.0);
    sheet.effects = vec![draw::inner_shadow(l.sheet_shadow, 0.0, 2.0, 8.0)];
    let sheet = b.add(panel, sheet);
    hall(b, sheet, pw, ph, l.line);
    kit::scale_children(b, sheet);
    kit::glass(b, sheet);
    let mut rim = draw::rect(
        "gilt",
        px - 2.0,
        py - 2.0,
        pw + 4.0,
        ph + 4.0,
        draw::solid("#00000000"),
    );
    rim.fills.clear();
    rim.stroke = document_proto::Stroke {
        paints: vec![draw::linear(
            90.0,
            &[(0.0, GILT_DIM), (0.5, GILT), (1.0, GILT_DIM)],
        )],
        weight: 2.0,
        ..document_proto::Stroke::default()
    };
    b.add(panel, rim);
    sheet
}

/// Brass for the Time knob.
const BRASS: [&str; 6] = [
    "color=#c9a45c",
    "pointer=#2a1a0c",
    "index=line",
    "index_w=0.1",
    "skirt=plain",
    "skirt_color=#6e5428",
];

/// The nameplate's title: "Hall" in the theatre's serif.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(700.0);
    crate::suite_studio_blues::parts::set(b, t, "Hall", 28.0, h / 2.0, size, ink, &serif, 0.06);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the drawing beside the house's controls — the
/// brass Time knob, the room's knobs, the nameplate, bypass and More. On a
/// phone the drawing narrows and the knobs are Size and Level.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    // Dark: purple velvet, the drawing a violet print. Light: an ivory
    // house, the drawing purple ink on lavender paper.
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane("Hall", tier, |b, root| {
        let pw = if small_room {
            kit::PHONE_PICTURE_R - 12.0
        } else {
            552.0
        };
        let panel = house(b, root, (w, kit::H), pw + 20.0, l);
        let sheet = drawing(b, panel, (10.0, 10.0, pw, kit::H - 20.0), l);
        let (ink, dim) = (l.ink, l.dim);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        let accent = if dark { GILT } else { "#4c2a85" };
        if small_room {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (6.0, 30.0),
                80.0,
                220.0,
                12.0,
                (accent, dim),
                |b, t| title(b, t, 30.0, 22.0, l.title),
            );
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                11.0,
                (ink, dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &BRASS);
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
                    &l.small,
                    ink,
                    dim,
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
                (accent, dim),
                |b, t| title(b, t, 28.0, 22.0, l.title),
            );
            let x0 = pw + 30.0;
            let (tx, ty, td) = (x0 + 40.0, 78.0, 50.0);
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                8.5,
                (ink, dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &BRASS);
                },
            );
            for (i, (id, legend)) in [
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("damping", "Damp"),
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
                    32.0,
                    (x0 + 110.0 + 50.0 * i as f64, 90.0),
                    &l.small,
                    ink,
                    dim,
                );
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "In");
        kit::flip_arrow(b, panel, VERB, tier, false, (ink, &ui::alpha(dim, 0.3)));
        // Responsive: the drawing (and its gilt rim) takes the width.
        let stretch = kit::with_sibling(b, panel, sheet, 1);
        kit::responsive(b, panel, &stretch);
    });
    let mut f = kit::reverb_face(kit::tier_file("81-verb-hall", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the house, no drawing — the advanced controls
/// across it, and (standing alone) the nameplate, the bypass and the arrow
/// where they are on the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane("Hall", tier, |b, root| {
        let panel = house(b, root, (w, kit::H), 0.0, l);
        let knobs = [
            ("predelay", "Pre-Delay"),
            ("damping", "Damp"),
            ("modulation", "Mod"),
            ("low_cut", "Low Cut"),
            ("high_cut", "High Cut"),
            ("tone", "Tone"),
            ("duck", "Duck"),
            ("duck_threshold", "Duck Thr"),
            ("duck_release", "Duck Rel"),
            ("pan_a", "Pan"),
        ];
        kit::back_knobs(b, panel, tier, &knobs, l.ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &l.small, l.ink, l.dim);
        });
        let accent = if dark { GILT } else { "#4c2a85" };
        let (np, preset_w, size) = if small_room {
            ((6.0, 30.0), 220.0, 12.0)
        } else {
            ((10.0, 28.0), 200.0, 10.0)
        };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                80.0,
                preset_w,
                size,
                (accent, l.dim),
                |b, t| title(b, t, np.1, 22.0, l.title),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "In");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file("81-verb-hall-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the drawing the whole top row, the house's controls
/// below it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane("Hall", |b, root| {
        let panel = house(b, root, (kit::TALL_W, kit::TALL_H), 0.0, l);
        drawing(b, panel, kit::TALL_PICTURE, l);
        let accent = if dark { GILT } else { "#4c2a85" };
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("damping", "Damp"),
                ("level", "Level"),
            ],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), accent),
            scrim: if dark { "#281043e6" } else { "#efe6d6e6" },
            bypass: ("purple", "In"),
            title_w: 80.0,
        };
        kit::tall_chrome(
            b,
            panel,
            &t,
            |b, p, addr, legend, d, at| {
                kit::knob3d(b, p, addr, legend, d, at, &l.small, l.ink, l.dim);
            },
            |b, p, addr, (cx, cy, d)| {
                kit::knob3d_bare(b, p, addr, d, (cx, cy), &BRASS);
            },
            |b, tn| title(b, tn, 30.0, 22.0, l.title),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file("81-verb-hall", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}

/// The routing grid's block: the hall's plan, its wavefronts going out.
#[must_use]
pub fn block() -> Face {
    use crate::rig_cells::S;
    let l = &DARK;
    let scene =
        crate::rig_cells::block("Hall", &[(0.0, l.paper.0), (1.0, l.paper.1)], |b, v, _| {
            hall(b, v, S, S, l.line);
        });
    kit::reverb_face("81-verb-hall-block", scene)
}
