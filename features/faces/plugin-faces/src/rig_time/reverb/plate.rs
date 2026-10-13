//! Plate: the steel plate itself, through its inspection window, with the
//! remote beside it — the plate reverb face (`reverb/plate`) laid out as a
//! lane of the Time module.
//!
//! The plate is the control. Its moving-coil driver can be picked up and
//! moved over the sheet: along the plate it sets the size (where it
//! excites the sheet), across it the tone (brighter toward the edge). The
//! porous damper slides toward the sheet and away on its crank — the
//! decay, as the real unit's damper sets it: closer, shorter. While it
//! plays, the sheet's standing waves shimmer.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::proto::{Axis, Control, ControlKind, HandleAxis};
use frame::surface::widgets::Builder;

use crate::face::Face;
use crate::reverb::ui::{self, Dial, bind};
use crate::rig_time::kit::{self, VERB};

const INK: &str = "#202327";
const DIM: &str = "#4e545b";
const STEEL: &str = "#aab0b6";

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// A coil spring between two points: a zigzag of `n` turns.
fn spring(a: (f64, f64), b: (f64, f64), n: usize, amp: f64) -> Vec<(f64, f64)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / len, dx / len);
    let mut pts = vec![a];
    let steps = n * 2;
    for i in 1..steps {
        let t = 0.15 + 0.7 * i as f64 / steps as f64;
        let s = if i % 2 == 0 { amp } else { -amp };
        pts.push((a.0 + dx * t + nx * s, a.1 + dy * t + ny * s));
    }
    pts.push(b);
    pts
}

/// The plate through its window, drawn in `tank` (`w × h`): frame,
/// springs, sheet, pickups, and the two things you move — the driver and
/// the damper.
#[allow(clippy::too_many_lines)]
fn plate(b: &mut Builder<'_>, tank: NodeId, w: f64, h: f64) {
    // Tubular frame.
    let tube = draw::stroked(
        draw::rounded(draw::frame("frame tube", 8.0, 7.0, w - 16.0, h - 14.0), 7.0),
        "#7b8187",
        4.0,
    );
    b.add(
        tank,
        draw::with_effects(tube, vec![draw::drop_shadow("#00000099", 0.0, 2.0, 3.0)]),
    );
    b.add(
        tank,
        draw::stroked(
            draw::rounded(draw::frame("tube light", 9.5, 8.0, w - 19.0, h - 16.0), 6.0),
            "#c9ced333",
            1.0,
        ),
    );
    let (px, py, pw, ph) = (30.0, 20.0, w * 0.66, h - 40.0);
    // Suspension: springs from the frame to the sheet's edge.
    let mut springs = Vec::new();
    for fx in [0.1, 0.5, 0.9] {
        let x = px + pw * fx;
        springs.push(((x, 10.5), (x, py + 2.0)));
        springs.push(((x, h - 10.5), (x, py + ph - 2.0)));
    }
    for fy in [0.3, 0.7] {
        let y = py + ph * fy;
        springs.push(((12.0, y), (px + 2.0, y)));
    }
    for (p, q) in springs {
        ui::line(b, tank, "spring", &spring(p, q, 3, 2.2), 1.2, STEEL);
    }
    // The sheet.
    let sheet = draw::rect(
        "plate",
        px,
        py,
        pw,
        ph,
        draw::linear(
            100.0,
            &[
                (0.0, "#e4e7e9"),
                (0.3, "#b6bbc0"),
                (0.55, "#d5d9dc"),
                (0.8, "#a9afb4"),
                (1.0, "#c7ccd0"),
            ],
        ),
    );
    let sheet = b.add(
        tank,
        draw::with_effects(
            sheet,
            vec![
                draw::drop_shadow("#000000b0", 0.0, 3.0, 6.0),
                draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.0),
            ],
        ),
    );
    // (No pane over the window: an effect inside another's draws plainly,
    // and the sheet's steel is the one that matters.)
    kit::steel(b, sheet);
    let grain: Vec<Vec<(f64, f64)>> = (0..18)
        .map(|i| {
            let y = py
                + 3.0
                + (ph - 6.0) * f64::from(i) / 17.0
                + crate::reverb::jitter(f64::from(i)) * 1.6;
            vec![(px + 2.0, y), (px + pw - 2.0, y)]
        })
        .collect();
    b.add(tank, ui::bars("grain", &grain, 0.5, "#ffffff2e"));
    // Standing waves: they shimmer while it plays, faster as it gets
    // brighter, and hold still when it doesn't.
    for i in 1..4 {
        let y0 = py + ph * f64::from(i) / 4.0;
        let pts: Vec<(f64, f64)> = (0..=72)
            .map(|k| {
                let u = f64::from(k) / 72.0;
                (
                    px + 6.0 + (pw - 12.0) * u,
                    y0 + (u * std::f64::consts::TAU * 3.0 + f64::from(i)).sin()
                        * 2.6
                        * (1.0 - 0.6 * u),
                )
            })
            .collect();
        // Each mode a line of light on the steel.
        let m = b.add(
            tank,
            ui::glowing(
                ui::line_node("mode", &pts, 1.1, "#8a63e0"),
                kit::PURPLE_LAMP,
                0.9,
                kit::GLOW,
            ),
        );
        let swing =
            format!("0.5 + 0.5 * sin(time * (3 + 2 * norm({VERB}/tone)) + {i}) * param({VERB}/on)");
        motion::add(
            b,
            m,
            Motion::new(Target::Opacity)
                .input(swing)
                .range(0.15, 0.7)
                .dynamics(Dynamics::spring(200.0, 24.0)),
        );
    }
    // Pickups, fixed at the sheet's left.
    for (x, y) in [
        (px + pw * 0.08, py + ph * 0.26),
        (px + pw * 0.12, py + ph * 0.78),
    ] {
        b.add(
            tank,
            draw::with_effects(
                draw::rounded(
                    draw::rect(
                        "pickup",
                        x - 8.0,
                        y - 5.5,
                        16.0,
                        11.0,
                        draw::linear(180.0, &[(0.0, "#2e2f32"), (1.0, "#0e0e10")]),
                    ),
                    2.0,
                ),
                vec![draw::drop_shadow("#00000080", 0.0, 1.5, 2.0)],
            ),
        );
        b.add(
            tank,
            draw::circle("contact", x, y, 2.0, draw::solid("#c9a64c")),
        );
        ui::line(b, tank, "lead", &[(x - 8.0, y), (12.0, y)], 1.0, "#141416");
    }
    ui::silk_left(
        b,
        tank,
        "Steel · 2 m × 1 m · 0.5 mm",
        px + pw * 0.2,
        py + ph - 7.0,
        6.0,
        "#3b4248",
        700.0,
    );

    // ── The driver: pick it up and move it over the sheet ─────────────
    // Over the sheet, clear of where the damper comes in.
    // A handle sits by its centre: the area is where the centre goes, so
    // the driver's edge stays clear of the damper's nearest reach.
    let area = b.add(
        tank,
        draw::frame(
            "driver area",
            px + 26.0,
            py + 14.0,
            pw * 0.8 - 26.0 - 26.0,
            ph - 28.0,
        ),
    );
    let r = 15.0;
    let d = b.add(
        area,
        draw::frame("Driver", 0.0, 0.0, 2.0 * r + 8.0, 2.0 * r + 8.0),
    );
    let c = r + 4.0;
    b.add(
        d,
        draw::with_effects(
            draw::circle(
                "housing",
                c,
                c,
                r,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3a3b3e"), (1.0, "#0c0c0d")]),
            ),
            vec![draw::drop_shadow("#000000a0", 0.0, 3.0, 5.0)],
        ),
    );
    b.add(
        d,
        draw::circle(
            "coil",
            c,
            c,
            r * 0.62,
            draw::conic(
                0.0,
                &[
                    (0.0, "#e0a064"),
                    (0.25, "#8a4f22"),
                    (0.5, "#d69457"),
                    (0.75, "#7a4219"),
                    (1.0, "#e0a064"),
                ],
            ),
        ),
    );
    b.add(
        d,
        draw::circle("pole", c, c, r * 0.24, draw::solid("#1b1b1d")),
    );
    // Its lamp: lit while the plate is driven.
    let lit = b.add(
        d,
        draw::emissive(
            draw::circle(
                "drive lamp",
                c + r * 0.72,
                c - r * 0.72,
                2.6,
                draw::solid(kit::PURPLE_LAMP),
            ),
            draw::hex(kit::PURPLE_LAMP),
            1.2,
            kit::GLOW,
        ),
    );
    motion::add(
        b,
        lit,
        Motion::new(Target::Opacity)
            .input(format!("param({VERB}/on)"))
            .range(0.0, 1.0),
    );
    let kind = ControlKind::Handle {
        x: Some(bind(&a("size"))),
        y: Some(bind(&a("tone"))),
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear {
            min: -1.0,
            max: 1.0,
        }),
    };
    b.controls
        .insert(d, Control::new(kind).bound(bind(&a("size"))));

    // ── The damper: slide it to the sheet (short) or away (long) ──────
    // At its closest it laps the sheet's end; drawn back, it clears it.
    let (mx0, mw) = (px + pw * 0.8, pw * 0.2);
    let travel = b.add(
        tank,
        draw::frame(
            "damper travel",
            mx0 + mw / 2.0,
            py - 8.0,
            (w - 26.0 - mw / 2.0) - (mx0 + mw / 2.0),
            ph + 16.0,
        ),
    );
    let dm = b.add(travel, draw::frame("Damper", 0.0, 0.0, mw, ph + 16.0));
    let pad = draw::rounded(
        draw::rect(
            "felt",
            0.0,
            0.0,
            mw,
            ph + 16.0,
            draw::linear(90.0, &[(0.0, "#3a3530e0"), (1.0, "#27231fe8")]),
        ),
        3.0,
    );
    b.add(
        dm,
        draw::with_effects(
            draw::stroked(pad, "#0c0b0a", 1.0),
            vec![draw::drop_shadow("#000000a0", -3.0, 2.0, 6.0)],
        ),
    );
    let mut holes = String::new();
    let mut yy = 6.0;
    let mut row = 0;
    while yy < ph + 12.0 {
        let mut xx = 6.0 + if row % 2 == 0 { 0.0 } else { 3.5 };
        while xx < mw - 4.0 {
            holes.push_str(&draw::regular(xx, yy, 1.1, 6, 0.0));
            holes.push(' ');
            xx += 7.0;
        }
        yy += 6.0;
        row += 1;
    }
    b.add(
        dm,
        ui::area("felt pores", holes, mw, ph + 16.0, draw::solid("#8a7d6c66")),
    );
    ui::silk(b, dm, "Damper", mw / 2.0, 11.0, 6.5, "#d8cbb8", 800.0);
    // The crank rod out to the motor, riding with it — as long as the
    // tank allows (a block's is narrow, and a control is not clipped).
    let rod = (w * 0.36).min(200.0);
    ui::line(
        b,
        dm,
        "crank rod",
        &[(mw, (ph + 16.0) / 2.0), (mw + rod, (ph + 16.0) / 2.0)],
        3.0,
        "#8d9399",
    );
    let kind = ControlKind::Handle {
        x: Some(bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls
        .insert(dm, Control::new(kind).bound(bind(&a("decay"))));
    // Responsive: the handles' ranges stretch with the window (the
    // controls in them keep their shape); the rest of the drawing scales.
    for id in [area, travel] {
        if let Some(n) = b.scene.node_mut(id) {
            n.layout_child.resize_x = Some(frame::document::proto::ResizeX::Stretch);
        }
    }
    // The motor, fixed at the frame.
    b.add(
        tank,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "motor",
                    w - 28.0,
                    h / 2.0 - 11.0,
                    16.0,
                    22.0,
                    draw::linear(
                        90.0,
                        &[(0.0, "#4a4d51"), (0.5, "#8e949a"), (1.0, "#3c3f43")],
                    ),
                ),
                3.0,
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)],
        ),
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: one design — the plate, its window and the remote
/// in the same paint and lettering — laid out for the room. The smaller
/// tiers give the window less width and the remote larger knobs and
/// type, keeping what matters on stage (Decay, Pre-Delay, Level, In; the
/// plate's driver still sets size and tone); the top right stays clear
/// for the lane's preset bar.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    // Light: the painted grey remote. Dark: charcoal, lettered in white.
    let (paint, ink, dim, well, rule) = if dark {
        (
            "charcoal",
            "#e6e8eb",
            "#9aa0a7",
            [(0.0, "#0c0d0f"), (1.0, "#3a3e44")],
            ("#000000aa", "#ffffff1f"),
        )
    } else {
        (
            "api",
            INK,
            DIM,
            [(0.0, "#6d7278"), (1.0, "#e2e5e8")],
            ("#00000033", "#ffffff66"),
        )
    };
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Plate", tier, |b, root| {
        let panel = ui::panel(b, root, "panel.painted", paint, w, kit::H, "none");
        if let Some(n) = b.scene.node_mut(panel) {
            n.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
        }

        // ── The plate, through its window ──────────────────────────────
        // On a phone the controls come first: the window shrinks for them.
        let tw = if small_room {
            kit::picture_r(tier) - 12.0
        } else {
            560.0
        };
        let (tx, ty, th) = (12.0, 12.0, kit::H - 24.0);
        let wl = draw::rounded(
            draw::rect(
                "window",
                tx - 4.0,
                ty - 4.0,
                tw + 8.0,
                th + 8.0,
                draw::linear(180.0, &well),
            ),
            7.0,
        );
        b.add(
            panel,
            draw::with_effects(wl, vec![draw::inner_shadow("#00000066", 0.0, 1.0, 1.0)]),
        );
        let mut tank = draw::frame("Plate", tx, ty, tw, th);
        tank.fills = vec![draw::linear(180.0, &[(0.0, "#1b1d20"), (1.0, "#2b2e32")])];
        tank.clips_content = true;
        tank.corner_radii = frame::document::proto::CornerRadii::uniform(5.0);
        tank.effects = vec![draw::inner_shadow("#000000", 0.0, 3.0, 10.0)];
        let tank = b.add(panel, tank);
        plate(b, tank, tw, th);
        kit::scale_children(b, tank);
        let sheen = draw::linear(
            160.0,
            &[
                (0.0, "#ffffff14"),
                (0.3, "#ffffff06"),
                (0.31, "#ffffff00"),
                (1.0, "#ffffff00"),
            ],
        );
        b.add(tank, draw::rect("glass", 0.0, 0.0, tw, th, sheen));

        // ── The remote ─────────────────────────────────────────────────
        let x0 = tx + tw + 20.0;
        ui::rule(b, panel, x0 - 10.0, 12.0, 1.0, kit::H - 24.0, rule.0);
        ui::rule(b, panel, x0 - 9.0, 12.0, 1.0, kit::H - 24.0, rule.1);
        if small_room {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (8.0, 30.0),
                84.0,
                if tier == kit::Tier::Ipad {
                    150.0
                } else {
                    220.0
                },
                12.0,
                (if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }, dim),
                |b, t| {
                    ui::plain(b, t, "Plate", 32.0, 15.0, 20.0, ink, 800.0);
                },
            );
            // The phone's places: Time, then Pre-Delay and Level.
            let (tx, ty, td) = kit::time_spot(tier);
            let big = Dial {
                value: false,
                legend: 0.0,
                ..Dial::new("knob.pointer", "black", td, ink, dim)
            };
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                if tier == kit::Tier::Ipad { 10.0 } else { 11.0 },
                (ink, dim, &ui::alpha(kit::PURPLE_LAMP, 0.55)),
                |b, p, addr| {
                    ui::dial(b, p, addr, "", tx, ty, &big);
                },
            );
            let (xs, ky, kd) = kit::knob_row(tier);
            let small = Dial::new("knob.collet", "black", kd, ink, dim).legend(11.0);
            for (i, (id, legend)) in [("predelay", "Pre-Delay"), ("level", "Level")]
                .iter()
                .enumerate()
            {
                // On a pane's narrower lane, the outer places: the legends
                // clear of each other.
                let x = if tier == kit::Tier::Ipad {
                    xs[i * 2]
                } else {
                    xs[i]
                };
                ui::dial(b, panel, &a(id), legend, x, ky, &small);
            }
        } else {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (12.0, 26.0),
                76.0,
                200.0,
                10.0,
                (if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }, dim),
                |b, t| {
                    ui::plain(b, t, "Plate", 28.0, 14.0, 18.0, ink, 800.0);
                },
            );
            let big = Dial {
                value: false,
                legend: 0.0,
                ..Dial::new("knob.pointer", "black", 56.0, ink, dim)
            };
            kit::time_knob(
                b,
                panel,
                VERB,
                (x0 + 50.0, 84.0, 56.0),
                8.5,
                (ink, dim, &ui::alpha(kit::PURPLE_LAMP, 0.55)),
                |b, p, addr| {
                    ui::dial(b, p, addr, "", x0 + 50.0, 84.0, &big);
                },
            );
            let small = Dial::new("knob.collet", "black", 36.0, ink, dim).legend(7.5);
            for (i, (id, legend)) in [
                ("predelay", "Pre-Delay"),
                ("damping", "HF Damp"),
                ("level", "Level"),
            ]
            .iter()
            .enumerate()
            {
                ui::dial(
                    b,
                    panel,
                    &a(id),
                    legend,
                    x0 + 136.0 + 62.0 * i as f64,
                    96.0,
                    &small,
                );
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "In");
        kit::flip_arrow(b, panel, VERB, tier, false, (ink, &ui::alpha(dim, 0.25)));
        // Responsive: the plate's window takes the width; the remote pins right.
        let stretch = kit::with_sibling(b, panel, tank, -1);
        kit::responsive(b, panel, &stretch);
    });
    let mut f = kit::reverb_face(kit::tier_file("82-verb-plate", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the remote's paint and lettering, no plate — the
/// advanced controls across it, the nameplate, the bypass and the arrow
/// where they are on the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let (paint, ink, dim) = if dark {
        ("charcoal", "#e6e8eb", "#9aa0a7")
    } else {
        ("api", INK, DIM)
    };
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Plate", tier, |b, root| {
        let panel = ui::panel(b, root, "panel.painted", paint, w, kit::H, "none");
        let knobs = [
            ("size", "Size"),
            ("damping", "HF Damp"),
            ("modulation", "Mod"),
            ("low_cut", "Low Cut"),
            ("high_cut", "High Cut"),
            ("duck", "Duck"),
            ("tone", "Tone"),
            ("duck_threshold", "Duck Thr"),
            ("duck_release", "Duck Rel"),
            ("pan_a", "Pan"),
        ];
        kit::back_knobs(
            b,
            panel,
            tier,
            &knobs,
            ink,
            |b, p, id, legend, d, (cx, cy)| {
                let k = Dial::new("knob.collet", "black", d, ink, dim).legend(if small_room {
                    10.5
                } else {
                    7.5
                });
                ui::dial(b, p, &a(id), legend, cx, cy, &k);
            },
        );
        let (np_h, title_w, preset_w, size) = if small_room {
            (
                30.0,
                84.0,
                if tier == kit::Tier::Ipad {
                    150.0
                } else {
                    220.0
                },
                12.0,
            )
        } else {
            (26.0, 76.0, 200.0, 10.0)
        };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                (if small_room { 8.0 } else { 12.0 }, np_h),
                title_w,
                preset_w,
                size,
                (if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }, dim),
                |b, t| {
                    ui::plain(
                        b,
                        t,
                        "Plate",
                        30.0,
                        np_h / 2.0,
                        if small_room { 20.0 } else { 18.0 },
                        ink,
                        800.0,
                    );
                },
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "In");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (ink, &ui::alpha(dim, 0.25)));
    });
    let mut f = kit::reverb_face(kit::tier_file("82-verb-plate-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the plate through a window the whole top row, every
/// control on the remote's paint below it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let (paint, ink, dim, well) = if dark {
        (
            "charcoal",
            "#e6e8eb",
            "#9aa0a7",
            [(0.0, "#0c0d0f"), (1.0, "#3a3e44")],
        )
    } else {
        ("api", INK, DIM, [(0.0, "#6d7278"), (1.0, "#e2e5e8")])
    };
    let scene = kit::tall_lane("Plate", |b, root| {
        let panel = ui::panel(
            b,
            root,
            "panel.painted",
            paint,
            kit::TALL_W,
            kit::TALL_H,
            "none",
        );
        if let Some(n) = b.scene.node_mut(panel) {
            n.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
        }
        let (tx, ty, tw, th) = kit::TALL_PICTURE;
        b.add(
            panel,
            draw::with_effects(
                draw::rounded(
                    draw::rect(
                        "window",
                        tx - 4.0,
                        ty - 4.0,
                        tw + 8.0,
                        th + 8.0,
                        draw::linear(180.0, &well),
                    ),
                    7.0,
                ),
                vec![draw::inner_shadow("#00000066", 0.0, 1.0, 1.0)],
            ),
        );
        let mut tank = draw::frame("Plate", tx, ty, tw, th);
        tank.fills = vec![draw::linear(180.0, &[(0.0, "#1b1d20"), (1.0, "#2b2e32")])];
        tank.clips_content = true;
        tank.corner_radii = frame::document::proto::CornerRadii::uniform(5.0);
        tank.effects = vec![draw::inner_shadow("#000000", 0.0, 3.0, 10.0)];
        let tank = b.add(panel, tank);
        plate(b, tank, tw, th);
        b.add(
            tank,
            draw::rect(
                "glass",
                0.0,
                0.0,
                tw,
                th,
                draw::linear(
                    160.0,
                    &[
                        (0.0, "#ffffff14"),
                        (0.3, "#ffffff06"),
                        (0.31, "#ffffff00"),
                        (1.0, "#ffffff00"),
                    ],
                ),
            ),
        );
        let accent = if dark { kit::PURPLE_LAMP } else { "#6a3fc4" };
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("predelay", "Pre-Delay"),
                ("damping", "HF Damp"),
                ("size", "Size"),
                ("level", "Level"),
            ],
            inks: (ink, dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), accent),
            scrim: if dark { "#2a2d31e6" } else { "#d9dce0e6" },
            bypass: ("purple", "In"),
            title_w: 84.0,
        };
        kit::tall_chrome(
            b,
            panel,
            &t,
            |b, p, addr, legend, d, (cx, cy)| {
                ui::dial(
                    b,
                    p,
                    addr,
                    legend,
                    cx,
                    cy,
                    &Dial::new("knob.collet", "black", d, ink, dim).legend(if d > 34.0 {
                        9.0
                    } else {
                        7.5
                    }),
                );
            },
            |b, p, addr, (cx, cy, d)| {
                ui::dial(
                    b,
                    p,
                    addr,
                    "",
                    cx,
                    cy,
                    &Dial {
                        value: false,
                        legend: 0.0,
                        ..Dial::new("knob.pointer", "black", d, ink, dim)
                    },
                );
            },
            |b, tn| {
                ui::plain(b, tn, "Plate", 32.0, 15.0, 20.0, ink, 800.0);
            },
        );
    });
    let mut f = kit::reverb_face(kit::tall_file("82-verb-plate", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}

/// The routing grid's block: the steel plate through its window, its
/// standing waves shimmering.
#[must_use]
pub fn block() -> Face {
    use crate::rig_cells::S;
    let scene =
        crate::rig_cells::block("Plate", &[(0.0, "#1b1d20"), (1.0, "#2b2e32")], |b, v, _| {
            plate(b, v, S, S);
            b.add(
                v,
                draw::rect(
                    "glass",
                    0.0,
                    0.0,
                    S,
                    S,
                    draw::linear(
                        160.0,
                        &[
                            (0.0, "#ffffff14"),
                            (0.3, "#ffffff06"),
                            (0.31, "#ffffff00"),
                            (1.0, "#ffffff00"),
                        ],
                    ),
                ),
            );
        });
    kit::reverb_face("82-verb-plate-block", scene)
}
