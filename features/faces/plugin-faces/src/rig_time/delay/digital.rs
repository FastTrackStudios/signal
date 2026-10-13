//! Digital: a studio digital delay in the rack — black anodised faceplate
//! and ears, the delay time on red seven-segment LEDs, a phosphor scope
//! showing the echoes as they will fall (each at a multiple of the time,
//! each lower by the feedback), the data wheel for the time, and the
//! parameter knobs in a grid.

use document_proto::NodeId;
use surface_kit::draw;
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};
use surface_kit::draw::Lettering;

const INK: &str = "#d9dde2";
const DIM: &str = "#7d838b";
const LED: &str = kit::BLUE_LAMP;
const PHOSPHOR: &str = "#8cc8ff";

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// A recessed dark window (`x, y, w, h`) with smoked glass; returns it.
fn window(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    glass: (&str, &str),
) -> NodeId {
    let bezel = draw::rounded(
        draw::rect(
            "bezel",
            x - 3.0,
            y - 3.0,
            w + 6.0,
            h + 6.0,
            draw::linear(180.0, &[(0.0, "#000000"), (1.0, "#2a2c30")]),
        ),
        4.0,
    );
    b.add(parent, bezel);
    let mut win = draw::frame(name, x, y, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, glass.0), (1.0, glass.1)])];
    win.clips_content = true;
    win.corner_radii = document_proto::CornerRadii::uniform(2.0);
    win.effects = vec![draw::inner_shadow("#000000e0", 0.0, 2.0, 5.0)];
    let win = b.add(parent, win);
    kit::glass(b, win);
    win
}

/// A rack ear with its two slotted holes, `x` wide from `x0`.
fn ear(b: &mut Builder<'_>, parent: NodeId, x0: f64, w: f64) {
    for cy in [30.0, kit::H - 30.0] {
        let slot = draw::rounded(
            draw::rect(
                "rack slot",
                x0 + w / 2.0 - 6.0,
                cy - 3.5,
                12.0,
                7.0,
                draw::solid("#050505"),
            ),
            3.5,
        );
        b.add(
            parent,
            draw::with_effects(
                slot,
                vec![
                    draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                    draw::drop_shadow("#ffffff1a", 0.0, 1.0, 0.0),
                ],
            ),
        );
    }
}

#[must_use]
pub fn face(dark: bool) -> Face {
    // Dark: black anodising. Light: brushed silver. The displays stay dark
    // glass either way, as they are on the real thing.
    let (ink, dim, fill, brush_ink, rules) = if dark {
        (
            INK,
            DIM,
            [(0.0, "#1f2124"), (0.5, "#16181a"), (1.0, "#0f1012")],
            "#ffffff08",
            ("#00000099", "#ffffff14"),
        )
    } else {
        (
            "#1c1f23",
            "#5d636b",
            [(0.0, "#eceef0"), (0.5, "#d9dbde"), (1.0, "#c3c6ca")],
            "#0000000c",
            ("#00000040", "#ffffff90"),
        )
    };
    let scene = kit::lane("Digital", |b, root| {
        let mut plate = draw::frame("Faceplate", 0.0, 0.0, kit::W, kit::H);
        plate.fills = vec![draw::linear(180.0, &fill)];
        plate.corner_radii = document_proto::CornerRadii::uniform(4.0);
        let plate = b.add(
            root,
            draw::with_effects(
                plate,
                vec![
                    draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
                    draw::inner_shadow("#00000080", 0.0, -2.0, 3.0),
                ],
            ),
        );
        // Brushed anodising.
        let brush: Vec<Vec<(f64, f64)>> = (0..80)
            .map(|i| {
                let y = 1.0 + f64::from(i) * 2.05;
                vec![(0.0, y), (kit::W, y)]
            })
            .collect();
        b.add(
            plate,
            ui::strokes("brush", &brush, 0.5, draw::solid(brush_ink)),
        );
        let ear_w = 26.0;
        ear(b, plate, 0.0, ear_w);
        ear(b, plate, kit::W - ear_w, ear_w);
        for x in [ear_w, kit::W - ear_w] {
            ui::rule(b, plate, x, 6.0, 1.0, kit::H - 12.0, rules.0);
            ui::rule(b, plate, x + 1.0, 6.0, 1.0, kit::H - 12.0, rules.1);
        }

        // ── The display: the time in milliseconds ──────────────────────
        let x0 = ear_w + 18.0;
        ui::silk_left(b, plate, "Digital Delay", x0, 16.0, 8.0, ink, 800.0);
        ui::silk_left(
            b,
            plate,
            "24-bit · 96 kHz",
            x0 + 94.0,
            16.0,
            6.0,
            dim,
            700.0,
        );
        let disp = window(
            b,
            plate,
            "Display",
            (x0, 28.0, 250.0, 78.0),
            ("#030a1c", "#01050e"),
        );
        kit::seg_readout(b, disp, &a("time"), (8.0, 42.0, 190.0), 40.0, 4, 0, LED);
        ui::silk_left(b, disp, "ms", 204.0, 58.0, 9.0, LED, 800.0);
        ui::silk_left(
            b,
            disp,
            "Delay",
            8.0,
            10.0,
            6.0,
            &ui::alpha(LED, 0.6),
            800.0,
        );
        // Feedback and level as smaller LED readouts under it.
        let sub = window(
            b,
            plate,
            "Sub display",
            (x0, 118.0, 250.0, 30.0),
            ("#030a1c", "#01050e"),
        );
        ui::silk_left(b, sub, "Fdbk", 8.0, 15.0, 6.0, &ui::alpha(LED, 0.6), 800.0);
        kit::seg_readout(b, sub, &a("feedback"), (34.0, 15.0, 70.0), 15.0, 3, 2, LED);
        ui::silk_left(
            b,
            sub,
            "Lvl dB",
            126.0,
            15.0,
            6.0,
            &ui::alpha(LED, 0.6),
            800.0,
        );
        kit::seg_readout(b, sub, &a("level"), (160.0, 15.0, 80.0), 15.0, 3, 0, LED);

        // ── The scope: the echoes as they fall ─────────────────────────
        let (sx, sy, sw, sh) = (x0 + 268.0, 28.0, 250.0, 120.0);
        let scope = window(b, plate, "Scope", (sx, sy, sw, sh), ("#03101f", "#010710"));
        let grid: Vec<Vec<(f64, f64)>> = (1..10)
            .map(|i| {
                vec![
                    (sw * f64::from(i) / 10.0, 0.0),
                    (sw * f64::from(i) / 10.0, sh),
                ]
            })
            .chain((1..4).map(|i| {
                vec![
                    (0.0, sh * f64::from(i) / 4.0),
                    (sw, sh * f64::from(i) / 4.0),
                ]
            }))
            .collect();
        b.add(
            scope,
            ui::strokes(
                "graticule",
                &grid,
                0.5,
                draw::solid(&ui::alpha(PHOSPHOR, 0.12)),
            ),
        );
        kit::echo_train(
            b,
            scope,
            (8.0, 14.0, sw - 16.0, sh - 26.0),
            10,
            2500.0,
            PHOSPHOR,
        );
        ui::silk_left(
            b,
            scope,
            "0",
            6.0,
            sh - 6.0,
            5.5,
            &ui::alpha(PHOSPHOR, 0.5),
            700.0,
        );
        ui::silk(
            b,
            scope,
            "2.5 s",
            sw - 16.0,
            sh - 6.0,
            5.5,
            &ui::alpha(PHOSPHOR, 0.5),
            700.0,
        );
        b.add(
            scope,
            draw::rect(
                "glass",
                0.0,
                0.0,
                sw,
                sh,
                draw::linear(160.0, &[(0.0, "#ffffff10"), (0.4, "#ffffff00")]),
            ),
        );

        // ── The wheel and the knobs ────────────────────────────────────
        let wx = sx + sw + 58.0;
        let wheel = [
            "color=#2a2d31",
            "pointer=#e9ecef",
            "index=dot",
            "skirt=plain",
            "skirt_color=#3a3e44",
        ];
        kit::knob3d(
            b,
            plate,
            &a("time"),
            "Delay Time",
            62.0,
            (wx, 66.0),
            &wheel,
            ink,
            dim,
        );
        let cap = [
            "color=#c4c8cd",
            "pointer=#101113",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#1a1c1f",
        ];
        let grid = [
            ("feedback", "Feedback"),
            ("high_cut", "Hi Cut"),
            ("mod_rate", "Mod Rate"),
            ("level", "Level"),
            ("high_pass", "Lo Cut"),
            ("mod_depth", "Mod Depth"),
        ];
        for (i, (id, legend)) in grid.iter().enumerate() {
            let (col, row) = (i % 3, i / 3);
            kit::knob3d(
                b,
                plate,
                &a(id),
                legend,
                26.0,
                (wx + 74.0 + 52.0 * col as f64, 42.0 + 64.0 * row as f64),
                &cap,
                ink,
                dim,
            );
        }
        kit::bypass(
            b,
            plate,
            DLY,
            kit::W - ear_w - 24.0,
            118.0,
            24.0,
            "blue",
            "On",
        );
        kit::scale_children(b, scope);
        // Responsive: the scope (and its bezel) takes the width.
        let stretch = kit::with_sibling(b, plate, scope, -1);
        kit::responsive(b, plate, &stretch);
    });
    kit::delay_face(kit::file("61-dly-digital", dark), scene)
}

/// The face at `tier`: the rack at full size on the desktop ([`face`]); in
/// a small room (a phone's lane, a desktop pane's) the same anodised plate
/// with the controls first — the LED time and the scope as its picture, the
/// Time knob, Feedback, Hi Cut and Level in the knob row, the nameplate at
/// the top right and the bypass at the Time faces' spot.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    if !tier.small() {
        return face(dark);
    }
    let (ink, dim, fill, brush_ink, knob) = if dark {
        (
            INK,
            DIM,
            [(0.0, "#1f2124"), (0.5, "#16181a"), (1.0, "#0f1012")],
            "#ffffff08",
            [
                "color=#2a2d31",
                "pointer=#e9ecef",
                "index=dot",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#3a3e44",
            ],
        )
    } else {
        (
            "#1c1f23",
            "#5d636b",
            [(0.0, "#eceef0"), (0.5, "#d9dbde"), (1.0, "#c3c6ca")],
            "#0000000c",
            [
                "color=#c4c8cd",
                "pointer=#101113",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#1a1c1f",
            ],
        )
    };
    let w = tier.lane_w();
    let pane = tier == kit::Tier::Ipad;
    let scene = kit::tier_lane("Digital", tier, |b, root| {
        let mut plate = draw::frame("Faceplate", 0.0, 0.0, w, kit::H);
        plate.fills = vec![draw::linear(180.0, &fill)];
        plate.corner_radii = document_proto::CornerRadii::uniform(4.0);
        let plate = b.add(
            root,
            draw::with_effects(
                plate,
                vec![
                    draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
                    draw::inner_shadow("#00000080", 0.0, -2.0, 3.0),
                ],
            ),
        );
        let brush: Vec<Vec<(f64, f64)>> = (0..80)
            .map(|i| {
                let y = 1.0 + f64::from(i) * 2.05;
                vec![(0.0, y), (w, y)]
            })
            .collect();
        b.add(
            plate,
            ui::strokes("brush", &brush, 0.5, draw::solid(brush_ink)),
        );

        // ── The picture: the time on the LEDs, the echoes under it ──────
        let px = 12.0;
        let pw = kit::picture_r(tier) - px - 10.0;
        let (dh, digits) = if pane { (54.0, 26.0) } else { (58.0, 32.0) };
        let disp = window(
            b,
            plate,
            "Display",
            (px, 14.0, pw, dh),
            ("#030a1c", "#01050e"),
        );
        kit::seg_readout(
            b,
            disp,
            &a("time"),
            (4.0, dh / 2.0 + 4.0, pw - 22.0),
            digits,
            4,
            0,
            LED,
        );
        ui::silk_left(b, disp, "ms", pw - 16.0, dh - 10.0, 7.0, LED, 800.0);
        ui::silk_left(b, disp, "Delay", 5.0, 8.0, 5.5, &ui::alpha(LED, 0.6), 800.0);
        let (sy, sh) = (14.0 + dh + 10.0, kit::H - 14.0 - (14.0 + dh + 10.0));
        let scope = window(b, plate, "Scope", (px, sy, pw, sh), ("#03101f", "#010710"));
        let grid: Vec<Vec<(f64, f64)>> = (1..6)
            .map(|i| {
                vec![
                    (pw * f64::from(i) / 6.0, 0.0),
                    (pw * f64::from(i) / 6.0, sh),
                ]
            })
            .collect();
        b.add(
            scope,
            ui::strokes(
                "graticule",
                &grid,
                0.5,
                draw::solid(&ui::alpha(PHOSPHOR, 0.12)),
            ),
        );
        kit::echo_train(
            b,
            scope,
            (4.0, 6.0, pw - 8.0, sh - 10.0),
            8,
            2500.0,
            PHOSPHOR,
        );
        b.add(
            scope,
            draw::rect(
                "glass",
                0.0,
                0.0,
                pw,
                sh,
                draw::linear(160.0, &[(0.0, "#ffffff10"), (0.4, "#ffffff00")]),
            ),
        );
        kit::scale_children(b, scope);

        // ── The controls ────────────────────────────────────────────────
        let lit = ui::alpha(kit::BLUE, 0.5);
        let (tx, ty, td) = kit::time_spot(tier);
        kit::time_knob(
            b,
            plate,
            DLY,
            (tx, ty, td),
            if pane { 10.0 } else { 11.0 },
            (ink, dim, &lit),
            |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &knob);
            },
        );
        let (xs, ky, kd) = kit::knob_row(tier);
        for (i, (id, legend)) in [
            ("feedback", "Feedback"),
            ("high_cut", "Hi Cut"),
            ("level", "Level"),
        ]
        .iter()
        .enumerate()
        {
            kit::knob3d(b, plate, &a(id), legend, kd, (xs[i], ky), &knob, ink, dim);
        }
        // The nameplate, top right: the preset, then the machine.
        let (np_h, title_w, preset_w, size) = if pane {
            (26.0, 76.0, 150.0, 11.0)
        } else {
            (30.0, 92.0, 200.0, 12.0)
        };
        let sans = Lettering::family(draw::family::INTER).weight(500.0);
        kit::nameplate_top_right(
            b,
            plate,
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
                    "Digital",
                    title_w / 2.0 - 8.0,
                    np_h / 2.0,
                    if pane { 16.0 } else { 19.0 },
                    ink,
                    &sans,
                    0.02,
                );
            },
        );
        kit::face_bypass(b, plate, DLY, tier, "blue", "On");
        kit::flip_arrow(b, plate, DLY, tier, false, (ink, &ui::alpha(dim, 0.3)));
        // Responsive: the scope (and its bezel) takes the width.
        let stretch = kit::with_sibling(b, plate, scope, -1);
        kit::responsive(b, plate, &stretch);
    });
    let mut f = kit::delay_face(kit::tier_file("61-dly-digital", tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::time_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its back at `tier`: the plate, no displays — the advanced controls
/// across it, the nameplate, the bypass and the arrow where they are on
/// the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let (ink, dim, fill, knob) = if dark {
        (
            INK,
            DIM,
            [(0.0, "#1f2124"), (0.5, "#16181a"), (1.0, "#0f1012")],
            [
                "color=#2a2d31",
                "pointer=#e9ecef",
                "index=dot",
                "index_w=0.1",
                "skirt=plain",
                "skirt_color=#3a3e44",
            ],
        )
    } else {
        (
            "#1c1f23",
            "#5d636b",
            [(0.0, "#eceef0"), (0.5, "#d9dbde"), (1.0, "#c3c6ca")],
            [
                "color=#c4c8cd",
                "pointer=#101113",
                "index=line",
                "index_w=0.12",
                "skirt=plain",
                "skirt_color=#1a1c1f",
            ],
        )
    };
    let w = tier.lane_w();
    let pane = tier == kit::Tier::Ipad;
    let scene = kit::tier_lane("Digital", tier, |b, root| {
        let mut plate = draw::frame("Faceplate", 0.0, 0.0, w, kit::H);
        plate.fills = vec![draw::linear(180.0, &fill)];
        plate.corner_radii = document_proto::CornerRadii::uniform(4.0);
        let plate = b.add(
            root,
            draw::with_effects(
                plate,
                vec![
                    draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
                    draw::inner_shadow("#00000080", 0.0, -2.0, 3.0),
                ],
            ),
        );
        let knobs = [
            ("mod_rate", "Mod Rate"),
            ("mod_depth", "Mod Depth"),
            ("high_pass", "Lo Cut"),
            ("duck_sens", "Duck"),
            ("duck_release", "Duck Rel"),
            ("pan", "Pan"),
            ("dry", "Dry"),
        ];
        kit::back_knobs(b, plate, tier, &knobs, ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &knob, ink, dim);
        });
        let (np_h, title_w, preset_w, size) = if pane {
            (26.0, 76.0, 150.0, 11.0)
        } else if tier.small() {
            (30.0, 92.0, 200.0, 12.0)
        } else {
            (28.0, 86.0, 190.0, 10.0)
        };
        let sans = Lettering::family(draw::family::INTER).weight(500.0);
        kit::nameplate_top_right(
            b,
            plate,
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
                    "Digital",
                    title_w / 2.0 - 8.0,
                    np_h / 2.0,
                    if pane { 16.0 } else { 19.0 },
                    ink,
                    &sans,
                    0.02,
                );
            },
        );
        kit::face_bypass(b, plate, DLY, tier, "blue", "On");
        kit::flip_arrow(b, plate, DLY, tier, true, (ink, &ui::alpha(dim, 0.3)));
    });
    let mut f = kit::delay_face(kit::tier_file("61-dly-digital-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// The routing grid's block: the scope, the echoes falling across it.
#[must_use]
pub fn block() -> Face {
    use crate::rig_cells::S;
    let scene = crate::rig_cells::block(
        "Digital",
        &[(0.0, "#03101f"), (1.0, "#010710")],
        |b, v, _| {
            let grid: Vec<Vec<(f64, f64)>> = (1..6)
                .map(|i| vec![(S * f64::from(i) / 6.0, 0.0), (S * f64::from(i) / 6.0, S)])
                .chain(
                    (1..6)
                        .map(|i| vec![(0.0, S * f64::from(i) / 6.0), (S, S * f64::from(i) / 6.0)]),
                )
                .collect();
            b.add(
                v,
                ui::strokes(
                    "graticule",
                    &grid,
                    0.6,
                    draw::solid(&ui::alpha(PHOSPHOR, 0.12)),
                ),
            );
            kit::echo_train(b, v, (12.0, 28.0, S - 24.0, S - 76.0), 8, 2500.0, PHOSPHOR);
        },
    );
    kit::delay_face("61-dly-digital-block", scene)
}
