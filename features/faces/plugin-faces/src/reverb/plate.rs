//! The plate: a sheet of steel under tension, and the remote that sets it.
//!
//! Left, the plate itself through an inspection window: the tubular frame,
//! the sheet hung in it by its springs, the moving-coil driver near the
//! centre and the two contact pickups off it, and the porous damper plate
//! alongside — whose distance from the sheet is what the DECAY dial sets.
//! Right, the remote: the big DECAY TIME dial printed in seconds (0.8–5 s,
//! the damper's travel), and the output VU. Below the window, the plugin's
//! additions in a row, and the plate keys.

use frame::document::proto::NodeId;
use frame::surface::kit::{Place, draw};
use frame::surface::proto::Unit;
use frame::surface::widgets::Builder;

use super::ui::{self, Dial, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 360.0;
const INK: &str = "#202327";
const DIM: &str = "#4e545b";

const DECAY_S: &[f64] = &[0.8, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0];
const DECAY_MARKS: &[&str] = &["0.8", "1", "1.5", "2", "3", "4", "5"];

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

/// The plate through its window, drawn in `tank` (`w × h`).
fn plate_art(b: &mut Builder<'_>, tank: NodeId, w: f64, h: f64) {
    // Tubular frame.
    let tube = draw::stroked(
        draw::rounded(
            draw::frame("frame tube", 12.0, 10.0, w - 24.0, h - 20.0),
            8.0,
        ),
        "#7b8187",
        5.0,
    );
    b.add(
        tank,
        draw::with_effects(tube, vec![draw::drop_shadow("#00000099", 0.0, 2.0, 3.0)]),
    );
    b.add(
        tank,
        draw::stroked(
            draw::rounded(
                draw::frame("tube light", 13.5, 11.0, w - 27.0, h - 22.0),
                7.0,
            ),
            "#c9ced333",
            1.0,
        ),
    );
    let (px, py, pw, ph) = (44.0, 30.0, w - 88.0, h - 60.0);
    // Suspension: springs from the frame to the sheet's edge.
    let steel = "#aab0b6";
    let mut springs = Vec::new();
    for fx in [0.12, 0.5, 0.88] {
        let x = px + pw * fx;
        springs.push(((x, 14.5), (x, py + 2.0)));
        springs.push(((x, h - 14.5), (x, py + ph - 2.0)));
    }
    for fy in [0.3, 0.7] {
        let y = py + ph * fy;
        springs.push(((16.5, y), (px + 2.0, y)));
        springs.push(((w - 16.5, y), (px + pw - 2.0, y)));
    }
    for (a, c) in springs {
        ui::line(b, tank, "spring", &spring(a, c, 4, 2.6), 1.3, steel);
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
    b.add(
        tank,
        draw::with_effects(
            sheet,
            vec![
                draw::drop_shadow("#000000b0", 0.0, 3.0, 6.0),
                draw::inner_shadow("#ffffff80", 0.0, 1.0, 0.0),
            ],
        ),
    );
    // Rolling-mill grain along the sheet.
    let grain: Vec<Vec<(f64, f64)>> = (0..26)
        .map(|i| {
            let y =
                py + 3.25 + (ph - 6.0) * f64::from(i) / 25.0 + super::jitter(f64::from(i)) * 2.0;
            vec![(px + 2.0, y), (px + pw - 2.0, y)]
        })
        .collect();
    b.add(tank, ui::bars("grain", &grain, 0.5, "#ffffff2e"));
    // Standing waves, faintly, as a ringing plate is drawn.
    for i in 1..5 {
        let y0 = py + ph * f64::from(i) / 5.0;
        let pts: Vec<(f64, f64)> = (0..=60)
            .map(|k| {
                let u = f64::from(k) / 60.0;
                (
                    px + 8.0 + (pw * 0.55) * u,
                    y0 + (u * std::f64::consts::TAU * 2.0 + f64::from(i)).sin() * 3.0 * (1.0 - u),
                )
            })
            .collect();
        ui::line(b, tank, "mode", &pts, 0.8, "#5a7f9c55");
    }
    // Driver near the middle; pickups off to the sides.
    let (dx, dy) = (px + pw * 0.4, py + ph * 0.5);
    b.add(
        tank,
        draw::with_effects(
            draw::circle(
                "driver",
                dx,
                dy,
                15.0,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3a3b3e"), (1.0, "#0c0c0d")]),
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 2.0, 3.0)],
        ),
    );
    b.add(
        tank,
        draw::circle(
            "coil",
            dx,
            dy,
            9.0,
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
        tank,
        draw::circle("pole", dx, dy, 3.5, draw::solid("#1b1b1d")),
    );
    for (x, y) in [
        (px + pw * 0.14, py + ph * 0.28),
        (px + pw * 0.2, py + ph * 0.76),
    ] {
        b.add(
            tank,
            draw::with_effects(
                draw::rounded(
                    draw::rect(
                        "pickup",
                        x - 9.0,
                        y - 6.0,
                        18.0,
                        12.0,
                        draw::linear(180.0, &[(0.0, "#2e2f32"), (1.0, "#0e0e10")]),
                    ),
                    2.0,
                ),
                vec![draw::drop_shadow("#00000080", 0.0, 1.5, 2.0)],
            ),
        );
        b.add(
            tank,
            draw::circle("contact", x, y, 2.2, draw::solid("#c9a64c")),
        );
        ui::line(
            b,
            tank,
            "lead",
            &[(x - 9.0, y), (px - 6.0, y), (17.0, y)],
            1.0,
            "#141416",
        );
    }
    ui::line(
        b,
        tank,
        "lead",
        &[(dx, dy + 15.0), (dx, h - 13.0)],
        1.0,
        "#141416",
    );
    // The damper: a porous plate close alongside, on its crank.
    let (mx, my, mw, mh) = (px + pw * 0.7, py - 8.0, pw * 0.26, ph + 16.0);
    let damper = draw::rounded(
        draw::rect(
            "damper",
            mx,
            my,
            mw,
            mh,
            draw::linear(90.0, &[(0.0, "#3a3530c8"), (1.0, "#27231fd8")]),
        ),
        3.0,
    );
    b.add(
        tank,
        draw::with_effects(
            draw::stroked(damper, "#0c0b0a", 1.0),
            vec![draw::drop_shadow("#000000a0", -3.0, 2.0, 6.0)],
        ),
    );
    let mut holes = String::new();
    let mut yy = my + 6.0;
    let mut row = 0;
    while yy < my + mh - 4.0 {
        let mut xx = mx + 6.0 + if row % 2 == 0 { 0.0 } else { 3.5 };
        while xx < mx + mw - 4.0 {
            holes.push_str(&draw::regular(xx, yy, 1.1, 6, 0.0));
            holes.push(' ');
            xx += 7.0;
        }
        yy += 6.0;
        row += 1;
    }
    b.add(
        tank,
        ui::area("felt pores", holes, w, h, draw::solid("#8a7d6c66")),
    );
    // Crank: a rod out to the frame, with its motor.
    ui::line(
        b,
        tank,
        "crank rod",
        &[(mx + mw, my + mh * 0.5), (w - 18.0, my + mh * 0.5)],
        3.0,
        "#8d9399",
    );
    b.add(
        tank,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "motor",
                    w - 34.0,
                    my + mh * 0.5 - 12.0,
                    20.0,
                    24.0,
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
    // Engraving.
    ui::silk_left(
        b,
        tank,
        "Steel · 2 m × 1 m · 0.5 mm",
        px + 8.0,
        py + ph - 9.0,
        6.0,
        "#3b4248",
        700.0,
    );
    ui::silk(
        b,
        tank,
        "Damper",
        mx + mw / 2.0,
        my + 10.0,
        6.5,
        "#d8cbb8",
        800.0,
    );
}

pub fn face() -> Face {
    let scene = canvas("Plate Reverberator", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "api", W, H, "ears");

        // ── Badge ──────────────────────────────────────────────────────
        ui::plain(b, panel, "Plate", 80.0, 34.0, 19.0, INK, 800.0);
        ui::silk_left(
            b,
            panel,
            "Steel plate reverberation unit",
            116.0,
            30.0,
            7.5,
            INK,
            800.0,
        );
        ui::silk_left(
            b,
            panel,
            "Remote control · mono in / stereo out",
            116.0,
            42.0,
            6.0,
            DIM,
            700.0,
        );

        // ── The plate, through its window ──────────────────────────────
        let (tx, ty, tw, th) = (46.0, 58.0, 520.0, 186.0);
        let well = draw::rounded(
            draw::rect(
                "window",
                tx - 4.0,
                ty - 4.0,
                tw + 8.0,
                th + 8.0,
                draw::linear(180.0, &[(0.0, "#6d7278"), (1.0, "#e2e5e8")]),
            ),
            7.0,
        );
        b.add(
            panel,
            draw::with_effects(well, vec![draw::inner_shadow("#00000066", 0.0, 1.0, 1.0)]),
        );
        let mut tank = draw::frame("Plate", tx, ty, tw, th);
        tank.fills = vec![draw::linear(180.0, &[(0.0, "#1b1d20"), (1.0, "#2b2e32")])];
        tank.clips_content = true;
        tank.corner_radii = frame::document::proto::CornerRadii::uniform(5.0);
        tank.effects = vec![draw::inner_shadow("#000000", 0.0, 3.0, 10.0)];
        tank.shader = Some(frame::document::proto::ShaderFx {
            shader: "glass".into(),
            params: std::collections::BTreeMap::from([("strength".to_string(), 0.25)]),
            inputs: Vec::new(),
        });
        let tank = b.add(panel, tank);
        plate_art(b, tank, tw, th);
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

        // ── The remote: DECAY TIME ─────────────────────────────────────
        let (kx, ky) = (690.0, 158.0);
        ui::silk(b, panel, "Decay Time", kx, 62.0, 10.0, INK, 800.0);
        let big = Dial::new("knob.pointer", "black", 76.0, INK, DIM)
            .scale(DECAY_MARKS)
            .legend(0.0);
        ui::dial(b, panel, "plate/decay", "Decay Time", kx, ky, &big);
        ui::silk(b, panel, "Seconds", kx, ky + 76.0, 7.0, DIM, 800.0);

        // Output VU.
        let vu = Place::new("Output")
            .size(170.0)
            .variant("satin_plate")
            .legend("VU")
            .bind(frame::surface::proto::Binding::stream("plate/out"));
        ui::put(b, panel, "meter.vu_ivory", &vu, 850.0, 126.0);
        ui::silk(b, panel, "Output", 850.0, 198.0, 7.5, INK, 800.0);

        // Plate keys.
        let keys = Place::new("Plate")
            .size(44.0)
            .variant("silver")
            .bind(bind("plate/model"))
            .options(&["I", "II", "III"]);
        let k = ui::put(b, panel, "button.metal_bank", &keys, 850.0, 256.0);
        ui::strip_legend(b, k);
        ui::bank_labels(
            b,
            panel,
            k,
            &["Classic", "Digital", "Modern"],
            300.0,
            6.5,
            DIM,
        );
        ui::silk(b, panel, "Plate", 850.0, 318.0, 7.5, INK, 800.0);
        ui::rule(b, panel, 604.0, 60.0, 1.0, 270.0, "#00000033");
        ui::rule(b, panel, 605.0, 60.0, 1.0, 270.0, "#ffffff66");

        // ── The row under the window ───────────────────────────────────
        let small = Dial::new("knob.collet", "black", 38.0, INK, DIM);
        let row: &[(&str, &str)] = &[
            ("plate/predelay", "Pre-Delay"),
            ("plate/tone", "Tone"),
            ("plate/damping", "HF Damp"),
            ("plate/density", "Density"),
            ("plate/motion", "Motion"),
            ("plate/width", "Width"),
            ("plate/mix", "Mix"),
        ];
        let pitch = (tw - 40.0) / (row.len() - 1) as f64;
        for (i, (addr, legend)) in row.iter().enumerate() {
            let cx = tx + 20.0 + pitch * i as f64;
            ui::dial(b, panel, addr, legend, cx, 286.0, &small);
        }
    });

    let params = vec![
        ui::param(
            "plate/decay",
            "Decay Time",
            ui::printed(DECAY_S, Unit::Seconds),
            2.0,
        )
        .featured(),
        super::predelay("plate/predelay", "Pre-Delay", 10.0),
        super::pct("plate/tone", "Tone", 55.0),
        super::pct("plate/damping", "HF Damping", 25.0),
        super::pct("plate/density", "Density", 80.0),
        super::pct("plate/motion", "Motion", 20.0),
        super::width("plate/width"),
        super::mix("plate/mix", 25.0),
        ui::choice("plate/model", "Plate", &["Classic", "Digital", "Modern"], 0).featured(),
    ];
    Face {
        file: "03-plate",
        plugin: "reverb",
        scene,
        params,
        streams: super::levels("plate"),
    }
}
