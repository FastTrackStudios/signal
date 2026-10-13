//! Analog: the bucket-brigade delay, kept simple — a matte midnight panel
//! with one clean line across it. The note is a bright spike at the left;
//! each repeat a soft swell further along (one delay time apart), lower by
//! the feedback and wider than the last, the way an analog repeat smears
//! and darkens. Beside it, the name and five knobs.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const INK: &str = "#e6eeff";
const DIM: &str = "#7f93bd";

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// A soft swell — a bell `w` wide and `h` tall standing on its baseline
/// at (`cx`, `base`) — filled with a glow and edged in `color`.
fn swell(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (cx, base): (f64, f64),
    w: f64,
    h: f64,
    color: &str,
) -> NodeId {
    let g = b.add(parent, draw::frame(name, cx - w / 2.0, base - h, w, h));
    let pts: Vec<(f64, f64)> = (0..=40)
        .map(|k| {
            let u = f64::from(k) / 40.0;
            let z = (u - 0.5) * 5.0;
            (w * u, h - h * (-z * z / 2.0).exp())
        })
        .collect();
    let mut d = format!("M 0 {h:.1} ");
    for (x, y) in &pts {
        d += &format!("L {x:.1} {y:.1} ");
    }
    d += &format!("L {w:.1} {h:.1} Z");
    // The swell gives off light, a soft halo round its body.
    b.add(
        g,
        ui::glowing(
            draw::path(
                "glow",
                0.0,
                0.0,
                w,
                h,
                d,
                draw::linear(
                    180.0,
                    &[
                        (0.0, &ui::alpha(color, 0.45)),
                        (1.0, &ui::alpha(color, 0.0)),
                    ],
                ),
            ),
            color,
            0.6,
            kit::BLOOM,
        ),
    );
    b.add(
        g,
        ui::glowing(
            ui::strokes("edge", &[pts], 1.6, draw::solid(color)),
            color,
            1.4,
            kit::GLOW,
        ),
    );
    g
}

/// The line in `view` (`w × h`): the note, then its repeats.
fn repeats(b: &mut Builder<'_>, view: NodeId, w: f64, h: f64, ink: &str, dim: &str, grid: &str) {
    let base = h - 18.0;
    // The baseline, and a faint time grid under it (a tick a quarter-second).
    b.add(
        view,
        ui::strokes(
            "baseline",
            &[vec![(0.0, base), (w, base)]],
            1.0,
            draw::solid(&ui::alpha(grid, 0.35)),
        ),
    );
    let window = 2500.0;
    let lead = 20.0;
    let span = w - lead - 16.0;
    let ticks: Vec<Vec<(f64, f64)>> = (0..=10)
        .map(|i| {
            let x = lead + span * f64::from(i) * 250.0 / window;
            vec![(x, base + 3.0), (x, base + 7.0)]
        })
        .collect();
    b.add(
        view,
        ui::strokes("ticks", &ticks, 1.0, draw::solid(&ui::alpha(grid, 0.3))),
    );
    ui::silk_left(b, view, "0", lead - 2.0, base + 12.0, 5.5, dim, 700.0);
    ui::silk(
        b,
        view,
        "2.5 s",
        lead + span - 6.0,
        base + 12.0,
        5.5,
        dim,
        700.0,
    );
    // The note.
    let note = draw::rect("note", lead - 1.0, 10.0, 2.0, base - 10.0, draw::solid(ink));
    b.add(view, ui::glowing(note, kit::BLUE_LIGHT, 1.3, kit::GLOW));
    // The repeats: each drawn at the left, slid to its time, shrunk by the
    // feedback, wider each time round.
    let spring = Dynamics::spring(260.0, 28.0);
    for i in 1..=8_u32 {
        let fi = f64::from(i);
        let wd = 16.0 + 7.0 * fi;
        let s = swell(
            b,
            view,
            &format!("repeat {i}"),
            (lead, base),
            wd,
            base - 12.0,
            kit::BLUE,
        );
        let at = format!("clamp({i} * param({DLY}/time) / {window}, 0, 1.2)");
        motion::add(
            b,
            s,
            Motion::new(Target::X)
                .input(at)
                .range(lead - wd / 2.0, lead - wd / 2.0 + span)
                .dynamics(spring.clone()),
        );
        let fall = format!(
            "pow(param({DLY}/feedback), {}) * (0.3 + 0.7 * param({DLY}/on))",
            i - 1
        );
        motion::add(
            b,
            s,
            Motion::new(Target::StretchY)
                .input(fall)
                .range(0.01, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(spring.clone()),
        );
        // A slow wobble: the analog clock's modulation.
        let wob =
            format!("sin(time * 6.2832 * param({DLY}/mod_rate) + {fi}) * norm({DLY}/mod_depth)");
        motion::add(
            b,
            s,
            Motion::new(Target::Y)
                .input(format!("0.5 + 0.5 * {wob}"))
                .range(-2.0 * fi.sqrt(), 2.0 * fi.sqrt()),
        );
    }
    kit::glass(b, view);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: one design — the matte panel and its one line —
/// laid out for the room. The smaller tiers give the line less width and
/// the knobs more (Time, Repeats, Tone, Level). The nameplate is at the
/// top right, the bypass at the Time faces' spot.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    // Dark: midnight. Light: a pale blue-white panel, a bright window.
    let (ink, dim, fill, edge, window, grid, knob) = if dark {
        (
            INK,
            DIM,
            [(0.0, "#16223d"), (1.0, "#0c1426")],
            "#2a3d66",
            [(0.0, "#0a1122"), (1.0, "#070c19")],
            kit::BLUE_LIGHT,
            [
                "color=#1b2a4a",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#0b1224",
            ],
        )
    } else {
        (
            "#12264a",
            "#62779f",
            [(0.0, "#f3f7fd"), (1.0, "#dde7f5")],
            "#b9cbe6",
            [(0.0, "#ffffff"), (1.0, "#eaf1fb")],
            kit::BLUE_DEEP,
            [
                "color=#f8fafd",
                "pointer=#1e3a8a",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#b9cbe6",
            ],
        )
    };
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Analog", tier, |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, w, kit::H);
        panel.fills = vec![draw::linear(180.0, &fill)];
        panel.corner_radii = frame::document::proto::CornerRadii::uniform(10.0);
        let panel = b.add(
            root,
            draw::with_effects(
                draw::stroked(panel, edge, 1.0),
                vec![draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.0)],
            ),
        );

        // ── The line ───────────────────────────────────────────────────
        // On a phone the controls come first: the line gives way.
        let vw = if small_room {
            kit::picture_r(tier) - 14.0
        } else {
            540.0
        };
        let (vx, vy, vh) = (14.0, 14.0, kit::H - 28.0);
        let mut view = draw::frame("Repeats", vx, vy, vw, vh);
        view.fills = vec![draw::linear(180.0, &window)];
        view.clips_content = true;
        view.corner_radii = frame::document::proto::CornerRadii::uniform(7.0);
        view.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 6.0)];
        let view = b.add(panel, view);
        repeats(b, view, vw, vh, ink, dim, grid);

        // ── The knobs ──────────────────────────────────────────────────
        let x0 = vx + vw + 24.0;
        let lit = ui::alpha(kit::BLUE, 0.5);
        if small_room {
            let (tx, ty, td) = kit::time_spot(tier);
            kit::time_knob(
                b,
                panel,
                DLY,
                (tx, ty, td),
                if tier == kit::Tier::Ipad { 10.0 } else { 11.0 },
                (ink, dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &knob);
                },
            );
            let (xs, ky, kd) = kit::knob_row(tier);
            for (i, (id, legend)) in [
                ("feedback", "Repeats"),
                ("high_cut", "Tone"),
                ("level", "Level"),
            ]
            .iter()
            .enumerate()
            {
                kit::knob3d(b, panel, &a(id), legend, kd, (xs[i], ky), &knob, ink, dim);
            }
        } else {
            kit::time_knob(
                b,
                panel,
                DLY,
                (x0 + 28.0, 78.0, 42.0),
                8.5,
                (ink, dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, 42.0, (x0 + 28.0, 78.0), &knob);
                },
            );
            for (i, (id, legend)) in [
                ("feedback", "Repeats"),
                ("high_cut", "Tone"),
                ("mod_depth", "Mod"),
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
                    (x0 + 96.0 + 58.0 * i as f64, 84.0),
                    &knob,
                    ink,
                    dim,
                );
            }
        }
        // The nameplate, top right: the preset, then the machine.
        let (np_h, title_w, preset_w, size) = if tier == kit::Tier::Ipad {
            (26.0, 76.0, 150.0, 11.0)
        } else if small_room {
            (30.0, 92.0, 200.0, 12.0)
        } else {
            (28.0, 86.0, 190.0, 10.0)
        };
        let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
        kit::nameplate_top_right(
            b,
            panel,
            DLY,
            tier,
            (8.0, np_h),
            title_w,
            preset_w,
            size,
            (
                if dark {
                    kit::BLUE_LIGHT
                } else {
                    kit::BLUE_DEEP
                },
                dim,
            ),
            |b, t| {
                crate::suite_studio_blues::parts::set(
                    b,
                    t,
                    "Analog",
                    title_w / 2.0 - 8.0,
                    np_h / 2.0,
                    if tier == kit::Tier::Ipad {
                        18.0
                    } else if small_room {
                        22.0
                    } else {
                        20.0
                    },
                    ink,
                    &sans,
                    0.02,
                );
            },
        );
        kit::face_bypass(b, panel, DLY, tier, "blue", "On");
        kit::flip_arrow(b, panel, DLY, tier, false, (ink, &ui::alpha(dim, 0.3)));
        // Responsive: the line takes the width.
        kit::responsive(b, panel, &[view]);
    });
    let mut f = kit::delay_face(kit::tier_file("62-dly-analog", tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::time_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its back at `tier`: the matte panel, no line — the advanced controls
/// across it, the nameplate, the bypass and the arrow where they are on
/// the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let (ink, dim, fill, edge, knob) = if dark {
        (
            INK,
            DIM,
            [(0.0, "#16223d"), (1.0, "#0c1426")],
            "#2a3d66",
            [
                "color=#1b2a4a",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#0b1224",
            ],
        )
    } else {
        (
            "#12264a",
            "#62779f",
            [(0.0, "#f3f7fd"), (1.0, "#dde7f5")],
            "#b9cbe6",
            [
                "color=#f8fafd",
                "pointer=#1e3a8a",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#b9cbe6",
            ],
        )
    };
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Analog", tier, |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, w, kit::H);
        panel.fills = vec![draw::linear(180.0, &fill)];
        panel.corner_radii = frame::document::proto::CornerRadii::uniform(10.0);
        let panel = b.add(
            root,
            draw::with_effects(
                draw::stroked(panel, edge, 1.0),
                vec![draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.0)],
            ),
        );
        let knobs = [
            ("mod_depth", "Mod"),
            ("mod_rate", "Mod Rate"),
            ("high_pass", "Low Cut"),
            ("duck_sens", "Duck"),
            ("duck_release", "Duck Rel"),
            ("pan", "Pan"),
            ("repeat_dyn", "Dynamics"),
            ("dry", "Dry"),
        ];
        kit::back_knobs(b, panel, tier, &knobs, ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &knob, ink, dim);
        });
        let (np_h, title_w, preset_w, size) = if tier == kit::Tier::Ipad {
            (26.0, 76.0, 150.0, 11.0)
        } else if small_room {
            (30.0, 92.0, 200.0, 12.0)
        } else {
            (28.0, 86.0, 190.0, 10.0)
        };
        let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
        kit::head(b, panel, DLY, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                DLY,
                tier,
                (8.0, np_h),
                title_w,
                preset_w,
                size,
                (
                    if dark {
                        kit::BLUE_LIGHT
                    } else {
                        kit::BLUE_DEEP
                    },
                    dim,
                ),
                |b, t| {
                    crate::suite_studio_blues::parts::set(
                        b,
                        t,
                        "Analog",
                        title_w / 2.0 - 8.0,
                        np_h / 2.0,
                        if tier == kit::Tier::Ipad {
                            18.0
                        } else if small_room {
                            22.0
                        } else {
                            20.0
                        },
                        ink,
                        &sans,
                        0.02,
                    );
                },
            );
            kit::face_bypass(b, head, DLY, tier, "blue", "On");
        });
        kit::flip_arrow(b, panel, DLY, tier, true, (ink, &ui::alpha(dim, 0.3)));
    });
    let mut f = kit::delay_face(kit::tier_file("62-dly-analog-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its tall face: the line the whole top row, every control on the panel
/// below it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let (ink, dim, fill, edge, window, grid, knob) = if dark {
        (
            INK,
            DIM,
            [(0.0, "#16223d"), (1.0, "#0c1426")],
            "#2a3d66",
            [(0.0, "#0a1122"), (1.0, "#070c19")],
            kit::BLUE_LIGHT,
            [
                "color=#1b2a4a",
                "pointer=#bcd6ff",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#0b1224",
            ],
        )
    } else {
        (
            "#12264a",
            "#62779f",
            [(0.0, "#f3f7fd"), (1.0, "#dde7f5")],
            "#b9cbe6",
            [(0.0, "#ffffff"), (1.0, "#eaf1fb")],
            kit::BLUE_DEEP,
            [
                "color=#f8fafd",
                "pointer=#1e3a8a",
                "index=line",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#b9cbe6",
            ],
        )
    };
    let scene = kit::tall_lane("Analog", |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, kit::TALL_W, kit::TALL_H);
        panel.fills = vec![draw::linear(180.0, &fill)];
        panel.corner_radii = frame::document::proto::CornerRadii::uniform(10.0);
        let panel = b.add(
            root,
            draw::with_effects(
                draw::stroked(panel, edge, 1.0),
                vec![draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.0)],
            ),
        );
        let (vx, vy, vw, vh) = kit::TALL_PICTURE;
        let mut view = draw::frame("Repeats", vx, vy, vw, vh);
        view.fills = vec![draw::linear(180.0, &window)];
        view.clips_content = true;
        view.corner_radii = frame::document::proto::CornerRadii::uniform(7.0);
        view.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 6.0)];
        let view = b.add(panel, view);
        repeats(b, view, vw, vh, ink, dim, grid);
        let sans = draw::Lettering::family(draw::family::INTER).weight(300.0);
        let t = kit::Tall {
            ns: DLY,
            main: &[
                ("feedback", "Repeats"),
                ("high_cut", "Tone"),
                ("mod_depth", "Mod"),
                ("level", "Level"),
            ],
            inks: (
                ink,
                dim,
                &ui::alpha(kit::BLUE, 0.5),
                if dark {
                    kit::BLUE_LIGHT
                } else {
                    kit::BLUE_DEEP
                },
            ),
            scrim: if dark { "#16223de6" } else { "#f3f7fde6" },
            bypass: ("blue", "On"),
            title_w: 92.0,
        };
        kit::tall_chrome(
            b,
            panel,
            &t,
            |b, p, addr, legend, d, at| {
                kit::knob3d(b, p, addr, legend, d, at, &knob, ink, dim);
            },
            |b, p, addr, (cx, cy, d)| {
                kit::knob3d_bare(b, p, addr, d, (cx, cy), &knob);
            },
            |b, tn| {
                crate::suite_studio_blues::parts::set(
                    b,
                    tn,
                    "Analog",
                    92.0 / 2.0 - 8.0,
                    15.0,
                    22.0,
                    ink,
                    &sans,
                    0.02,
                );
            },
        );
    });
    let mut f = kit::delay_face(kit::tall_file("62-dly-analog", dark), scene);
    f.params.extend(kit::tall_params(DLY));
    f
}

/// The routing grid's block: the line, the note's spike and its repeats
/// swelling and smearing along it.
#[must_use]
pub fn block() -> Face {
    use crate::rig_cells::S;
    let scene = crate::rig_cells::block(
        "Analog",
        &[(0.0, "#0a1122"), (1.0, "#070c19")],
        |b, v, _| {
            repeats(b, v, S, S, INK, DIM, kit::BLUE_LIGHT);
        },
    );
    kit::delay_face("62-dly-analog-block", scene)
}
