//! Wah: the pedal itself, side on, on a pedalboard.
//!
//! The treadle is the control: it rocks on its axle over a rack-and-pinion
//! housing — drag it, and it tilts heel-down (bass) to toe-down (treble)
//! through the pedal's real 16°. It is a `Knob` whose `Needle` is the
//! treadle — one frame of its parts, turning about the axle (it fills a
//! square centred on it). Beside it, the side module a modern wah grows:
//! the MODE (pedal, auto, rhythmic), the voicing — Q, sensitivity, shape —
//! the sweep RATE with its lamp for the auto modes, MIX and OUTPUT, and the
//! engage stomp.

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_proto::{Binding, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::pedal;
use crate::face::{Face, canvas};
use crate::reverb::ui::{self, Dial, bind};

const W: f64 = 940.0;
const H: f64 = 480.0;
const INK: &str = "#eceef2";
const DIM: &str = "#9ca1ab";
/// The treadle's travel, heel-down to toe-down.
const TRAVEL: f64 = 16.0;

/// The rocking treadle: a `Knob` over `wah/position` whose body turns
/// about the axle at (`ax`, `ay`). `len` is the treadle's length, the axle
/// `heel` from its heel end.
fn treadle(b: &mut Builder<'_>, parent: NodeId, ax: f64, ay: f64, len: f64, heel: f64) -> NodeId {
    let (x0, x1) = (ax - heel, ax - heel + len);
    let (top, bottom) = (ay - 60.0, ay - 20.0);
    let root = b.add(
        parent,
        draw::frame("Treadle", x0 - 8.0, top - 16.0, len + 16.0, ay - top + 36.0),
    );
    let (ox, oy) = (x0 - 8.0, top - 16.0);
    // The pivot frame: square, centred on the axle (in root coordinates).
    let r = len.max(heel) + 10.0;
    let (px, py) = (ax - ox, ay - oy);
    let pivot = b.add(root, draw::frame("pivot", px - r, py - r, 2.0 * r, 2.0 * r));
    let body = b.add(pivot, draw::frame("treadle", 0.0, 0.0, 2.0 * r, 2.0 * r));
    // Parts, in pivot coordinates (the axle is at (r, r)).
    let (lx0, lx1) = (x0 - ax + r, x1 - ax + r);
    let (ly0, ly1) = (top - ay + r, bottom - ay + r);
    let s = 2.0 * r;
    let part = |b: &mut Builder<'_>, name: &str, d: String, paint| {
        b.add(body, ui::area(name, d, s, s, paint));
    };
    // The side ears the axle passes through.
    let ears = draw::polygon(&[
        (r - 30.0, ly1 - 2.0),
        (r + 30.0, ly1 - 2.0),
        (r + 20.0, r + 14.0),
        (r - 20.0, r + 14.0),
    ]);
    part(
        b,
        "ears",
        ears,
        draw::linear(180.0, &[(0.0, "#2c2d31"), (1.0, "#101012")]),
    );
    // The casting: heel rounded, a nose at the toe.
    let cast = draw::polygon(&[
        (lx0 + 10.0, ly0),
        (lx1 - 20.0, ly0),
        (lx1 - 5.0, ly0 + 6.0),
        (lx1, ly0 + 18.0),
        (lx1 - 8.0, ly1),
        (lx0 + 16.0, ly1),
        (lx0 + 3.0, ly1 - 10.0),
        (lx0, ly0 + 12.0),
        (lx0 + 3.0, ly0 + 4.0),
    ]);
    part(
        b,
        "casting",
        cast,
        draw::linear(
            180.0,
            &[
                (0.0, "#55565c"),
                (0.12, "#34353a"),
                (0.6, "#222326"),
                (1.0, "#0f0f11"),
            ],
        ),
    );
    // Top-edge light and the web cast along the side.
    part(
        b,
        "edge light",
        draw::polygon(&[
            (lx0 + 10.0, ly0),
            (lx1 - 20.0, ly0),
            (lx1 - 18.0, ly0 + 2.0),
            (lx0 + 8.0, ly0 + 2.0),
        ]),
        draw::solid("#9a9ca3"),
    );
    let web = ly0 + (ly1 - ly0) * 0.55;
    part(
        b,
        "web",
        draw::polygon(&[
            (lx0 + 18.0, web),
            (lx1 - 16.0, web),
            (lx1 - 16.0, web + 1.5),
            (lx0 + 18.0, web + 1.5),
        ]),
        draw::solid("#00000080"),
    );
    part(
        b,
        "web light",
        draw::polygon(&[
            (lx0 + 18.0, web + 1.5),
            (lx1 - 16.0, web + 1.5),
            (lx1 - 16.0, web + 2.5),
            (lx0 + 18.0, web + 2.5),
        ]),
        draw::solid("#ffffff1a"),
    );
    // Rubber tread, ribbed.
    part(
        b,
        "tread",
        draw::polygon(&[
            (lx0 + 14.0, ly0 - 7.0),
            (lx1 - 26.0, ly0 - 7.0),
            (lx1 - 24.0, ly0 + 1.0),
            (lx0 + 12.0, ly0 + 1.0),
        ]),
        draw::linear(180.0, &[(0.0, "#3c3c40"), (1.0, "#141416")]),
    );
    let mut ribs = String::new();
    let n = 30;
    for i in 0..n {
        let u = (f64::from(i) + 0.5) / f64::from(n);
        let x = lx0 + 18.0 + (lx1 - lx0 - 48.0) * u;
        ribs.push_str(&draw::polygon(&[
            (x - 2.0, ly0 - 7.0),
            (x + 2.0, ly0 - 7.0),
            (x + 2.0, ly0 - 4.5),
            (x - 2.0, ly0 - 4.5),
        ]));
    }
    part(b, "ribs", ribs, draw::solid("#5a5a60"));
    // A painted index on the toe: where it points on the scale.
    part(
        b,
        "index",
        draw::polygon(&[
            (lx1 - 34.0, ly0 + 10.0),
            (lx1 - 10.0, ly0 + 10.0),
            (lx1 - 10.0, ly0 + 13.0),
            (lx1 - 34.0, ly0 + 13.0),
        ]),
        draw::solid("#f2b23a"),
    );
    // It rocks: the treadle swings past where the position puts it and
    // settles, a spring's lag on the knob's turn (motion).
    surface_kit::motion::add(
        b,
        body,
        surface_kit::motion::spring_lag(
            "norm(wah/position)",
            surface_kit::motion::Target::Rotate,
            TRAVEL,
        ),
    );
    let mut c = Control::new(ControlKind::Knob {
        sweep_degrees: TRAVEL,
    })
    .slot(SlotRole::Needle, body.0);
    c.binding = Some(Binding::param("wah/position"));
    b.controls.insert(root, c);
    root
}

/// The housing under the treadle, the axle bolt and the toe switch.
fn housing(b: &mut Builder<'_>, board: NodeId, x: f64, y: f64, w: f64, h: f64) {
    for fx in [x + 26.0, x + w - 26.0] {
        b.add(
            board,
            draw::rounded(
                draw::rect(
                    "foot",
                    fx - 18.0,
                    y + h - 2.0,
                    36.0,
                    9.0,
                    draw::linear(180.0, &[(0.0, "#1a1a1b"), (1.0, "#050505")]),
                ),
                3.0,
            ),
        );
    }
    let body = draw::rounded(
        draw::rect(
            "housing",
            x,
            y,
            w,
            h,
            draw::linear(
                180.0,
                &[
                    (0.0, "#3a3b40"),
                    (0.08, "#2a2b2f"),
                    (0.9, "#17181a"),
                    (1.0, "#0c0c0d"),
                ],
            ),
        ),
        10.0,
    );
    b.add(
        board,
        draw::with_effects(
            body,
            vec![
                draw::drop_shadow("#000000c0", 0.0, 8.0, 16.0),
                draw::inner_shadow("#ffffff26", 0.0, 1.5, 1.0),
            ],
        ),
    );
    // Crinkle finish: a speckle.
    let mut speck = String::new();
    for i in 0..220 {
        let fi = f64::from(i);
        let (sx, sy) = (
            x + 8.0 + crate::reverb::jitter(fi) * (w - 16.0),
            y + 6.0 + crate::reverb::jitter(fi + 50.5) * (h - 12.0),
        );
        speck.push_str(&draw::regular(sx, sy, 0.9, 4, 0.0));
        speck.push(' ');
    }
    b.add(
        board,
        ui::area("crinkle", speck, x + w, y + h, draw::solid("#ffffff10")),
    );
    // The name plate on the side.
    let plate = draw::rounded(
        draw::rect(
            "name plate",
            x + w / 2.0 - 70.0,
            y + h / 2.0 - 15.0,
            140.0,
            30.0,
            draw::linear(
                180.0,
                &[(0.0, "#e6e7e9"), (0.5, "#9b9ea3"), (1.0, "#d5d7da")],
            ),
        ),
        4.0,
    );
    b.add(
        board,
        draw::with_effects(plate, vec![draw::drop_shadow("#00000099", 0.0, 1.5, 2.0)]),
    );
    let mut t = ui::text(
        "name",
        "Wah",
        x + w / 2.0 - 70.0,
        y + h / 2.0,
        140.0,
        18.0,
        "#16171a",
        TextAlign::Center,
        800.0,
        false,
    );
    if let document_proto::NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = 1.0;
    }
    b.add(board, t);
}

/// The side module: a small box in the pedal's crinkle.
fn module(b: &mut Builder<'_>, board: NodeId, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let mut m = draw::frame("Module", x, y, w, h);
    m.fills = vec![draw::linear(
        175.0,
        &[(0.0, "#2e2f34"), (0.5, "#232428"), (1.0, "#18191c")],
    )];
    m.corner_radii = document_proto::CornerRadii::uniform(14.0);
    m.effects = vec![
        draw::drop_shadow("#000000c0", 0.0, 8.0, 16.0),
        draw::inner_shadow("#ffffff2a", 0.0, 1.5, 1.0),
        draw::inner_shadow("#00000080", 0.0, -3.0, 6.0),
    ];
    let m = b.add(board, m);
    for (sx, sy) in [
        (12.0, 12.0),
        (w - 12.0, 12.0),
        (12.0, h - 12.0),
        (w - 12.0, h - 12.0),
    ] {
        b.add(
            m,
            draw::circle(
                "screw",
                sx,
                sy,
                4.0,
                draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#e8e8e4"), (1.0, "#4c4d4a")]),
            ),
        );
    }
    m
}

pub fn face() -> Face {
    let scene = canvas("Wah", |b, root| {
        ui::fit_root(b, root, W, H);
        // The board: dark tolex.
        let mut board = draw::frame("Pedalboard", 0.0, 0.0, W, H);
        board.fills = vec![
            draw::linear(180.0, &[(0.0, "#1d1d20"), (1.0, "#101012")]),
            draw::radial_at(0.35, 0.5, 0.7, &[(0.0, "#ffffff0c"), (1.0, "#ffffff00")]),
        ];
        board.corner_radii = document_proto::CornerRadii::uniform(10.0);
        let board = b.add(root, board);
        let mut weave = Vec::new();
        let mut yy = 3.0;
        while yy < H {
            weave.push(vec![(0.0, yy + 0.3), (W, yy + 0.3)]);
            yy += 3.0;
        }
        b.add(board, ui::bars("weave", &weave, 0.6, "#ffffff06"));

        // ── The pedal, side on ─────────────────────────────────────────
        let (hx, hy, hw, hh) = (60.0, 262.0, 560.0, 104.0);
        let (ax, ay) = (hx + hw * 0.45, hy + 12.0);
        // Scale printed on the board behind the toe: heel (bass) … toe (treble).
        ui::plain(b, board, "Wah", 86.0, 44.0, 20.0, INK, 800.0);
        ui::silk_left(
            b,
            board,
            "Inductor wah · rocker pedal",
            122.0,
            40.0,
            7.5,
            INK,
            800.0,
        );
        ui::silk_left(
            b,
            board,
            "Drag the treadle: heel down for bass, toe down for treble",
            122.0,
            53.0,
            6.5,
            DIM,
            700.0,
        );
        ui::silk_left(b, board, "Heel · bass", hx - 4.0, 150.0, 7.5, DIM, 800.0);
        ui::silk_right(
            b,
            board,
            "Toe · treble",
            hx + hw + 4.0,
            150.0,
            7.5,
            DIM,
            800.0,
        );
        let toe = hx + hw - 14.0;
        for i in 0..=8 {
            let a = -TRAVEL / 2.0 + TRAVEL * f64::from(i) / 8.0;
            let rad = a.to_radians();
            let (dx, dy) = (rad.cos(), rad.sin());
            let reach = toe - ax;
            let (x1, y1) = (ax + dx * (reach + 16.0), ay - 58.0 + dy * (reach + 16.0));
            let len = if i % 4 == 0 { 12.0 } else { 6.0 };
            ui::line(
                b,
                board,
                "scale",
                &[(x1, y1), (x1 + dx * len, y1 + dy * len)],
                if i % 4 == 0 { 1.6 } else { 1.0 },
                &ui::alpha(INK, 0.55),
            );
        }
        // The treadle rides between the housing's walls: drawn first, its
        // toe sinks behind the housing's lip when pressed down.
        treadle(b, board, ax, ay, hw - 10.0, hw * 0.45 - 5.0);
        housing(b, board, hx, hy, hw, hh);
        // The boss round the axle on the housing's side.
        b.add(
            board,
            draw::with_effects(
                draw::circle(
                    "boss",
                    ax,
                    ay,
                    17.0,
                    draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3c3d42"), (1.0, "#121214")]),
                ),
                vec![draw::drop_shadow("#00000099", 0.0, 2.0, 3.0)],
            ),
        );
        // Axle bolt over everything.
        b.add(
            board,
            draw::with_effects(
                draw::circle(
                    "axle",
                    ax,
                    ay,
                    9.0,
                    draw::conic(
                        0.0,
                        &[
                            (0.0, "#f0f0ec"),
                            (0.25, "#80817e"),
                            (0.5, "#e4e4e0"),
                            (0.75, "#6e6f6c"),
                            (1.0, "#f0f0ec"),
                        ],
                    ),
                ),
                vec![draw::drop_shadow("#00000099", 0.0, 1.5, 2.0)],
            ),
        );
        b.add(
            board,
            draw::path(
                "axle nut",
                0.0,
                0.0,
                ax + 10.0,
                ay + 10.0,
                draw::regular(ax, ay, 4.5, 6, 0.0),
                draw::solid("#4b4c4a"),
            ),
        );
        // Where the treadle sits, read out.
        ui::readout(
            b,
            board,
            "Position",
            bind("wah/position"),
            hx + hw / 2.0,
            424.0,
            96.0,
            22.0,
            "#f2b23a",
        );
        ui::silk(b, board, "Position", hx + hw / 2.0, 402.0, 7.0, DIM, 800.0);

        // ── The side module ────────────────────────────────────────────
        let (mx, my, mw, mh) = (668.0, 28.0, 240.0, 424.0);
        let m = module(b, board, mx, my, mw, mh);
        let mode = Dial::new("switch.rotary", "cream", 40.0, INK, DIM)
            .options(&["Pedal", "Auto", "Rhythm"])
            .sweep(120.0)
            .legend(8.0)
            .legend_at(96.0);
        ui::dial(b, m, "wah/mode", "Mode", mw / 2.0, 56.0, &mode);
        let knob = Dial::new("knob.daka", "black", 36.0, INK, DIM).legend(7.5);
        let grid: &[(&str, &str, f64, f64)] = &[
            ("wah/resonance", "Q", 66.0, 136.0),
            ("wah/sensitivity", "Sens", mw - 66.0, 136.0),
            ("wah/rate", "Rate", 66.0, 214.0),
            ("wah/shape", "Shape", mw - 66.0, 214.0),
            ("wah/mix", "Mix", 66.0, 292.0),
            ("wah/output", "Output", mw - 66.0, 292.0),
        ];
        for (addr, legend, cx, cy) in grid {
            ui::dial(b, m, addr, legend, *cx, *cy, &knob.legend_at(cy + 44.0));
        }
        ui::stream_lamp(
            b,
            m,
            "Sweep lamp",
            "wah/lfo",
            "Sweep",
            mw / 2.0,
            214.0,
            26.0,
            "#ffae2e",
            INK,
        );
        ui::stomp(
            b,
            m,
            "Engage",
            "wah/on",
            mw / 2.0,
            mh - 36.0,
            40.0,
            "#ff3b2f",
        );
    });

    let params = vec![
        ui::percent("wah/position", "Position", 50.0).featured(),
        ui::choice("wah/mode", "Mode", &["Pedal", "Auto", "Rhythmic"], 0).featured(),
        ui::percent("wah/resonance", "Resonance", 60.0),
        ui::percent("wah/sensitivity", "Sensitivity", 50.0),
        pedal::rate("wah", 1.0),
        ui::percent("wah/shape", "Shape", 50.0),
        ui::percent("wah/mix", "Mix", 100.0),
        pedal::output("wah"),
        pedal::engaged("wah"),
    ];
    Face {
        file: "05-wah",
        plugin: "modulation",
        scene,
        params,
        streams: pedal::lfo("wah"),
    }
}
