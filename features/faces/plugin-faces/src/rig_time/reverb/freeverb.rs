//! FreeVerb: Jezar's Schroeder–Moorer reverb, drawn as the circuit it is —
//! a board with its network laid out in copper. The input fans out to eight
//! comb filters side by side, each a chip with its delay line meandering
//! after it (as long as its tuning, 1116 to 1617 samples) and its feedback
//! loop running back under it; their outputs sum and pass through four
//! allpasses in series to the output. The size sets where each delay line
//! is tapped (the rest of the line goes dark); the feedback loops glow as
//! hard as the decay drives them, dimmer as the damping takes their highs;
//! signal pulses run the traces. The decay is the slide pot on the board.
//!
//! Two finishes: light, a lilac board in a pale enclosure; dark, a purple
//! board in black anodised aluminium.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Look, Room, Unit};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const UNIT: Unit = Unit {
    name: "FreeVerb",
    base: "93-verb-freeverb",
    title_w: 110.0,
    legend: "On",
    lane: [
        ("size", "Room Size"),
        ("damping", "Damp"),
        ("level", "Level"),
    ],
    tall: [
        ("size", "Room Size"),
        ("damping", "Damp"),
        ("predelay", "Pre-Delay"),
        ("level", "Level"),
    ],
    look,
    ground,
    picture,
    title,
};

/// The signal's light: the section's purple.
const PULSE: &str = kit::PURPLE_LAMP;

/// The comb tunings (samples at 44.1 kHz) and the allpasses'.
const COMBS: [u32; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [u32; 4] = [556, 441, 341, 225];

struct Finish {
    look: Look,
    case: [(f64, &'static str); 3],
    mask: [(f64, &'static str); 2],
    copper: &'static str,
    silk: &'static str,
}

const LIGHT: Finish = Finish {
    look: Look {
        ink: "#26222e",
        dim: "#666072",
        accent: "#6a3fc4",
        scrim: "#dedce2e6",
        knob: [
            "color=#2a2730",
            "pointer=#ffffff",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#8f8a98",
        ],
        wheel: [
            "color=#2d2a33",
            "pointer=#c4b5fd",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#a9a4b2",
        ],
    },
    case: [(0.0, "#eceaef"), (0.6, "#dcd9e1"), (1.0, "#c9c5d0")],
    mask: [(0.0, "#8f78c4"), (1.0, "#7a62b0")],
    copper: "#e3c06a",
    silk: "#ffffff",
};

const DARK: Finish = Finish {
    look: Look {
        ink: "#e9e6ee",
        dim: "#8f899b",
        accent: kit::PURPLE_LAMP,
        scrim: "#1b1a1fe6",
        knob: [
            "color=#1c1b20",
            "pointer=#e9e6ee",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#45424c",
        ],
        wheel: [
            "color=#18171c",
            "pointer=#c4b5fd",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#55515c",
        ],
    },
    case: [(0.0, "#2a292e"), (0.6, "#1f1e22"), (1.0, "#161518")],
    mask: [(0.0, "#3d2163"), (1.0, "#2b164a")],
    copper: "#d9b45a",
    silk: "#f2eefa",
};

fn finish(dark: bool) -> &'static Finish {
    if dark { &DARK } else { &LIGHT }
}

fn look(dark: bool) -> &'static Look {
    &finish(dark).look
}

/// Where the delay lines are tapped (0..1 of their length): the size.
fn tap() -> String {
    format!("lerp(0.5, 1, norm({VERB}/size))")
}

/// The enclosure: anodised, a screw at each end.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), dark: bool) -> NodeId {
    let f = finish(dark);
    let panel = shell::panel(b, root, (w, h), &f.case, 6.0);
    for (x, y) in [
        (6.0, 12.0),
        (6.0, h - 12.0),
        (w - 6.0, 12.0),
        (w - 6.0, h - 12.0),
    ] {
        b.add(
            panel,
            draw::circle(
                "screw",
                x,
                y,
                2.4,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#d9d7de"), (1.0, "#5d5a63")]),
            ),
        );
        b.add(
            panel,
            ui::line_node(
                "slot",
                &[(x - 1.6, y - 1.0), (x + 1.6, y + 1.0)],
                0.7,
                "#2a282e",
            ),
        );
    }
    panel
}

/// A chip at (x, y), `w × h`, lettered `label`, its legs gold.
fn chip(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    label: &str,
    f: &Finish,
) {
    let legs: Vec<Vec<(f64, f64)>> = (0..((w - 4.0) / 3.0) as i32)
        .flat_map(|i| {
            let lx = x + 3.0 + f64::from(i) * 3.0;
            [
                vec![(lx, y - 1.5), (lx, y)],
                vec![(lx, y + h), (lx, y + h + 1.5)],
            ]
        })
        .collect();
    b.add(
        parent,
        ui::strokes("legs", &legs, 1.0, draw::solid(f.copper)),
    );
    b.add(
        parent,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "chip",
                    x,
                    y,
                    w,
                    h,
                    draw::linear(180.0, &[(0.0, "#2c2a30"), (1.0, "#121114")]),
                ),
                1.2,
            ),
            vec![draw::drop_shadow("#00000090", 0.0, 1.0, 1.5)],
        ),
    );
    b.add(
        parent,
        draw::circle("dot", x + 2.5, y + 2.5, 0.8, draw::solid("#5a5760")),
    );
    ui::plain(
        b,
        parent,
        label,
        x + w / 2.0,
        y + h / 2.0,
        (h * 0.42).min(5.5),
        "#d8d5de",
        800.0,
    );
}

/// A square meander `len` long from (x, y), `amp` either side, as a
/// delay line is laid out in copper.
fn meander(x: f64, y: f64, len: f64, amp: f64) -> Vec<(f64, f64)> {
    let pitch = 3.0;
    let mut pts = vec![(x, y)];
    let mut k = 0.0;
    let mut up = true;
    while k + pitch <= len {
        let a = if up { -amp } else { amp };
        pts.push((x + k, y + a));
        pts.push((x + k + pitch, y + a));
        k += pitch;
        up = !up;
    }
    pts.push((x + len, y));
    pts
}

/// The eight combs in (x, y, w, h) of `board`, in two columns of four (or
/// one of eight where it is short), the input trunk at `trunk_x` feeding
/// them. Returns where their sum leaves (x, y).
fn combs(
    b: &mut Builder<'_>,
    board: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    trunk_x: f64,
    f: &Finish,
) -> (f64, f64) {
    let cols = 2;
    let rows = 4;
    let cw = w / f64::from(cols);
    let rh = h / f64::from(rows);
    let copper = ui::alpha(f.copper, 0.9);
    // The input trunk.
    b.add(
        board,
        ui::line_node(
            "trunk",
            &[(trunk_x, y + rh * 0.5), (trunk_x, y + h - rh * 0.5)],
            1.2,
            &copper,
        ),
    );
    let bus_y = y + h + 4.0;
    for (i, tuning) in COMBS.iter().enumerate() {
        let (col, row) = (i as u32 / rows, i as u32 % rows);
        let cx = x + cw * f64::from(col);
        let m = y + rh * (f64::from(row) + 0.5);
        // In: from the trunk (or from the previous column's gap) to the chip.
        b.add(
            board,
            ui::line_node(
                "in",
                &[
                    (if col == 0 { trunk_x } else { cx - 4.0 }, m),
                    (cx + 2.0, m),
                ],
                1.0,
                &copper,
            ),
        );
        if col > 0 && row == 0 {
            // The trunk carries on over the top to the second column.
            b.add(
                board,
                ui::line_node(
                    "trunk over",
                    &[
                        (trunk_x, y + rh * 0.5),
                        (trunk_x, y + 1.0),
                        (cx - 4.0, y + 1.0),
                        (cx - 4.0, y + h - rh * 0.5),
                    ],
                    1.0,
                    &copper,
                ),
            );
        }
        chip(
            b,
            board,
            (cx + 2.0, m - 6.0, 22.0, 12.0),
            &format!("C{}", i + 1),
            f,
        );
        // The delay line: as long as its tuning.
        let full = cw - 44.0;
        let len = full * f64::from(*tuning) / 1617.0;
        let x0 = cx + 28.0;
        b.add(
            board,
            ui::line_node("stub", &[(cx + 24.0, m), (x0, m)], 1.0, &copper),
        );
        let amp = (rh * 0.18).min(4.0);
        b.add(
            board,
            ui::strokes(
                "delay line",
                &[meander(x0, m, len, amp)],
                0.8,
                draw::solid(&copper),
            ),
        );
        // Past the tap, the line is dark.
        let dark_line = b.add(
            board,
            draw::frame("untapped", x0, m - amp - 1.0, len, 2.0 * amp + 2.0),
        );
        b.add(
            dark_line,
            draw::rect(
                "mask",
                0.0,
                0.0,
                len,
                2.0 * amp + 2.0,
                draw::solid(&ui::alpha(f.mask[1].1, 0.78)),
            ),
        );
        motion::add(
            b,
            dark_line,
            Motion::new(Target::StretchX)
                .input(format!("1 - {}", tap()))
                .range(0.0, 1.0)
                .pivot(1.0, 0.5)
                .dynamics(Dynamics::spring(200.0, 26.0)),
        );
        // The tap: a via riding the line.
        let via = b.add(board, draw::frame("tap", x0 - 3.0, m - 3.0, 6.0, 6.0));
        b.add(
            via,
            ui::glowing(
                draw::circle("via", 3.0, 3.0, 2.2, draw::solid("#f4ecff")),
                PULSE,
                1.1,
                kit::GLOW,
            ),
        );
        motion::add(
            b,
            via,
            Motion::new(Target::X)
                .input(tap())
                .range(0.0, len)
                .dynamics(Dynamics::spring(200.0, 26.0)),
        );
        // The feedback loop, under the line, back to its start: as bright
        // as the decay drives it, its highs damped.
        let ly = m + rh * 0.34;
        let lp = vec![
            (x0 + len, m),
            (x0 + len, ly),
            (x0 - 2.0, ly),
            (x0 - 2.0, m + 2.0),
        ];
        let fb = b.add(
            board,
            ui::glowing(
                ui::strokes("feedback", &[lp], 0.9, draw::solid("#e9dcff")),
                PULSE,
                1.0,
                kit::GLOW,
            ),
        );
        motion::add(
            b,
            fb,
            Motion::new(Target::Opacity)
                .input(format!(
                    "(0.15 + 0.85 * norm({VERB}/decay)) * (0.4 + 0.6 * param({VERB}/on))"
                ))
                .range(0.0, 1.0),
        );
        motion::add(
            b,
            fb,
            Motion::new(Target::Emission)
                .input(format!("1 - 0.75 * norm({VERB}/damping)"))
                .range(0.0, 1.0),
        );
        // Out: to the column's bus, down to the sum.
        let bus_x = cx + cw - 8.0;
        b.add(
            board,
            ui::line_node("out", &[(x0 + len, m), (bus_x, m)], 1.0, &copper),
        );
        if row == 0 {
            b.add(
                board,
                ui::line_node("bus", &[(bus_x, m), (bus_x, bus_y)], 1.2, &copper),
            );
        }
        ui::plain(
            b,
            board,
            &tuning.to_string(),
            x0 + len / 2.0,
            m - amp - 4.0,
            4.5,
            &ui::alpha(f.silk, 0.75),
            700.0,
        );
    }
    // The two buses join under the combs.
    let (b0, b1) = (x + cw - 8.0, x + 2.0 * cw - 8.0);
    b.add(
        board,
        ui::line_node("sum bus", &[(b0, bus_y), (b1, bus_y)], 1.2, &copper),
    );
    // Pulses down the trunk.
    for k in 0..2 {
        let p = b.add(
            board,
            draw::frame("pulse", trunk_x - 3.0, y + rh * 0.5 - 3.0, 6.0, 6.0),
        );
        b.add(
            p,
            ui::glowing(
                draw::circle("dot", 3.0, 3.0, 1.6, draw::solid("#ffffff")),
                PULSE,
                1.4,
                kit::GLOW,
            ),
        );
        motion::add(
            b,
            p,
            Motion::new(Target::Y)
                .input(format!("fract(time * 0.5 + {})", f64::from(k) * 0.5))
                .range(0.0, h - rh),
        );
        motion::add(
            b,
            p,
            Motion::new(Target::Opacity)
                .input(format!("param({VERB}/on)"))
                .range(0.0, 1.0),
        );
    }
    (b1, bus_y)
}

/// The allpass chain from the sum at `from` along `y` to the output at
/// `out_x`, with pulses running it.
fn allpasses(b: &mut Builder<'_>, board: NodeId, from: (f64, f64), y: f64, out_x: f64, f: &Finish) {
    let copper = ui::alpha(f.copper, 0.9);
    let sx = from.0 + 16.0;
    b.add(
        board,
        ui::line_node("to sum", &[from, (from.0, y), (sx, y)], 1.2, &copper),
    );
    b.add(
        board,
        draw::with_effects(
            draw::circle("sum", sx, y, 6.0, draw::solid(f.copper)),
            vec![draw::drop_shadow("#00000070", 0.0, 1.0, 1.5)],
        ),
    );
    ui::plain(b, board, "Σ", sx, y, 6.5, "#2a1d08", 800.0);
    b.add(
        board,
        ui::line_node("chain", &[(sx + 6.0, y), (out_x, y)], 1.2, &copper),
    );
    let span = out_x - sx - 20.0;
    for (i, d) in ALLPASSES.iter().enumerate() {
        let ax = sx + 14.0 + span * (i as f64 + 0.5) / 4.0 - 15.0;
        chip(
            b,
            board,
            (ax, y - 7.0, 30.0, 14.0),
            &format!("AP{}", i + 1),
            f,
        );
        ui::plain(
            b,
            board,
            &d.to_string(),
            ax + 15.0,
            y + 13.0,
            4.5,
            &ui::alpha(f.silk, 0.75),
            700.0,
        );
    }
    b.add(
        board,
        draw::circle("out pad", out_x, y, 4.5, draw::solid(f.copper)),
    );
    b.add(
        board,
        draw::circle("out hole", out_x, y, 1.8, draw::solid("#1a1420")),
    );
    ui::plain(b, board, "OUT", out_x - 1.0, y - 10.0, 5.0, f.silk, 800.0);
    for k in 0..3 {
        let p = b.add(board, draw::frame("pulse", sx + 3.0, y - 3.0, 6.0, 6.0));
        b.add(
            p,
            ui::glowing(
                draw::circle("dot", 3.0, 3.0, 1.6, draw::solid("#ffffff")),
                PULSE,
                1.4,
                kit::GLOW,
            ),
        );
        motion::add(
            b,
            p,
            Motion::new(Target::X)
                .input(format!("fract(time * 0.45 + {:.3})", f64::from(k) / 3.0))
                .range(0.0, out_x - sx - 6.0),
        );
        motion::add(
            b,
            p,
            Motion::new(Target::Opacity)
                .input(format!("param({VERB}/on)"))
                .range(0.0, 1.0),
        );
    }
}

/// The slide pot along `y` from `x0` to `x1`: its slot, its legend, and its
/// cap — drag it for the decay (the combs' feedback).
fn slide_pot(b: &mut Builder<'_>, board: NodeId, (x0, x1, y): (f64, f64, f64), f: &Finish) {
    b.add(
        board,
        draw::with_effects(
            draw::rounded(
                draw::rect(
                    "pot body",
                    x0 - 10.0,
                    y - 6.0,
                    x1 - x0 + 20.0,
                    12.0,
                    draw::linear(180.0, &[(0.0, "#3a3840"), (1.0, "#1c1b20")]),
                ),
                2.0,
            ),
            vec![draw::drop_shadow("#00000090", 0.0, 1.0, 2.0)],
        ),
    );
    b.add(
        board,
        draw::rounded(
            draw::rect("slot", x0, y - 1.5, x1 - x0, 3.0, draw::solid("#060508")),
            1.5,
        ),
    );
    ui::silk_left(
        b,
        board,
        "Feedback · decay",
        x0 - 10.0,
        y + 11.0,
        5.0,
        f.silk,
        800.0,
    );
    shell::decay_handle(b, board, "Cap", (x0, x1, y - 9.0), (12.0, 18.0), |b, c| {
        b.add(
            c,
            draw::with_effects(
                draw::rounded(
                    draw::rect(
                        "cap",
                        0.0,
                        0.0,
                        12.0,
                        18.0,
                        draw::linear(
                            90.0,
                            &[(0.0, "#d9d6de"), (0.5, "#f6f4f8"), (1.0, "#a9a5b0")],
                        ),
                    ),
                    2.0,
                ),
                vec![draw::drop_shadow("#000000a0", 0.0, 1.5, 2.0)],
            ),
        );
        let grip: Vec<Vec<(f64, f64)>> = (0..4)
            .map(|k| {
                vec![
                    (2.5, 4.0 + 3.0 * f64::from(k)),
                    (9.5, 4.0 + 3.0 * f64::from(k)),
                ]
            })
            .collect();
        b.add(c, ui::strokes("grip", &grip, 0.7, draw::solid("#77737e")));
        b.add(
            c,
            ui::glowing(
                draw::rect("index", 5.0, 1.0, 2.0, 2.0, draw::solid(PULSE)),
                PULSE,
                1.0,
                kit::GLOW,
            ),
        );
    });
}

/// The board in (x, y, w, h) on `panel`.
fn board(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    room: Room,
    f: &Finish,
) -> NodeId {
    let board = shell::well(b, panel, "Board", (x, y, w, h), &f.mask, 5.0);
    // The mounting holes.
    for (hx, hy) in [
        (6.0, 6.0),
        (w - 6.0, 6.0),
        (6.0, h - 6.0),
        (w - 6.0, h - 6.0),
    ] {
        b.add(
            board,
            draw::circle("ring", hx, hy, 3.4, draw::solid(f.copper)),
        );
        b.add(
            board,
            draw::circle("hole", hx, hy, 1.8, draw::solid("#0c0a10")),
        );
    }
    // In.
    let iy = h / 2.0;
    b.add(
        board,
        draw::circle("in pad", 16.0, iy, 4.5, draw::solid(f.copper)),
    );
    b.add(
        board,
        draw::circle("in hole", 16.0, iy, 1.8, draw::solid("#1a1420")),
    );
    ui::plain(b, board, "IN", 16.0, iy - 10.0, 5.0, f.silk, 800.0);
    let trunk = 26.0;
    b.add(
        board,
        ui::line_node(
            "in trace",
            &[(20.0, iy), (trunk, iy)],
            1.2,
            &ui::alpha(f.copper, 0.9),
        ),
    );
    match room {
        Room::Phone => {
            combs(b, board, (34.0, 14.0, w - 40.0, h - 30.0), trunk, f);
            ui::silk_right(
                b,
                board,
                "→ 4 allpasses",
                w - 10.0,
                h - 6.0,
                5.0,
                f.silk,
                800.0,
            );
        }
        Room::Lane => {
            let sum = combs(b, board, (34.0, 14.0, 300.0, h - 34.0), trunk, f);
            allpasses(b, board, sum, 52.0, w - 18.0, f);
            slide_pot(b, board, (sum.0 + 40.0, w - 40.0, 96.0), f);
            ui::silk_right(
                b,
                board,
                "FV-8/4 · rev B · Jezar",
                w - 14.0,
                h - 8.0,
                5.0,
                &ui::alpha(f.silk, 0.7),
                800.0,
            );
        }
        Room::Tall => {
            let sum = combs(b, board, (34.0, 12.0, 340.0, h - 30.0), trunk, f);
            // Under the nameplate: the allpasses and the pot.
            allpasses(b, board, sum, 66.0, w - 18.0, f);
            slide_pot(b, board, (sum.0 + 60.0, w - 40.0, 94.0), f);
        }
    }
    board
}

/// The picture: the board, under glass.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    at: (f64, f64, f64, f64),
    room: Room,
    dark: bool,
) -> Vec<NodeId> {
    let board = board(b, panel, at, room, finish(dark));
    kit::glass(b, board);
    vec![board]
}

/// The nameplate's title: "FreeVerb" in the code's monospace.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, dark: bool) {
    let l = draw::Lettering::family(draw::family::JETBRAINS_MONO).weight(700.0);
    crate::suite_studio_blues::parts::set(
        b,
        t,
        "FreeVerb",
        48.0,
        h / 2.0,
        size * 0.78,
        finish(dark).look.ink,
        &l,
        -0.12,
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the board beside the controls; on a phone, the combs
/// alone.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    shell::face_at(&UNIT, dark, tier)
}

/// Its back at `tier`.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    shell::back_at(&UNIT, dark, tier)
}

/// Its tall face.
#[must_use]
pub fn tall(dark: bool) -> Face {
    shell::tall(&UNIT, dark)
}
