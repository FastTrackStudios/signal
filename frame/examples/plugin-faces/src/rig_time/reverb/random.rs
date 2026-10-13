//! Random: a digital reverberator in the rack, the way the first ones
//! looked — a 1U-tall faceplate with its ear and handle, a smoked display
//! lit by LEDs, the program number in seven segments and the program name
//! on a dot-matrix line, a bank of long-throw faders for the parameters
//! and a row of program keys, each with its lamp.
//!
//! The display's matrix is the algorithm itself: one row of LEDs per delay
//! line, its lit dot the line's tap as it random-walks, snapping from LED
//! to LED the way a stepped readout does. How far the taps wander follows
//! the modulation; they stop when the reverb is out.
//!
//! The program keys pick the algorithm (Random lit; the others swap the
//! face). Two finishes: light, the oatmeal enamel of a late-seventies
//! studio unit; dark, black-anodised graphite.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{NodeId, TextAlign};
use surface_kit::{Ink, Place, draw, motion};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB, VERB_ALGOS};

/// The algorithm this face draws, as `VERB_ALGOS` names it.
const NAME: &str = "Random";
/// The display's LEDs: the section's purple.
const LED: &str = kit::PURPLE_LAMP;
/// The program keys: the algorithm each picks, its legend.
const KEYS: [(&str, &str); 8] = [
    ("Room", "Room"),
    ("Hall", "Hall"),
    ("Plate", "Plate"),
    ("Spring", "Spring"),
    ("Chorale", "Chorale"),
    ("NonLinear", "NonLin"),
    ("Velvet", "Velvet"),
    (NAME, "Random"),
];

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    plate: [(f64, &'static str); 3],
    ear: [(f64, &'static str); 2],
    brush: &'static str,
    /// The rule's shadow and its light edge.
    rules: (&'static str, &'static str),
    key: [(f64, &'static str); 2],
    /// The faders' caps (a `lever.fader` variant).
    fader: &'static str,
    /// The data wheel (the Time knob) and the small knobs.
    wheel: [&'static str; 5],
    cap: [&'static str; 6],
    accent: &'static str,
}

const LIGHT: Look = Look {
    ink: "#2a2620",
    dim: "#6f675a",
    plate: [(0.0, "#eee8da"), (0.55, "#ded6c4"), (1.0, "#cbc2ae")],
    ear: [(0.0, "#d6cebb"), (1.0, "#b9af99")],
    brush: "#0000000a",
    rules: ("#00000038", "#ffffff8c"),
    key: [(0.0, "#f8f4ea"), (1.0, "#d8cfbb")],
    fader: "black",
    wheel: [
        "color=#26232c",
        "pointer=#efe9ff",
        "index=dot",
        "skirt=plain",
        "skirt_color=#3b3742",
    ],
    cap: [
        "color=#efe9dc",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#2a2620",
    ],
    accent: "#6a3fc4",
};

const DARK: Look = Look {
    ink: "#e6e3ec",
    dim: "#8e8a9b",
    plate: [(0.0, "#2b2c31"), (0.55, "#1f2024"), (1.0, "#151619")],
    ear: [(0.0, "#27282d"), (1.0, "#131417")],
    brush: "#ffffff07",
    rules: ("#000000a0", "#ffffff14"),
    key: [(0.0, "#3c3d44"), (1.0, "#222329")],
    fader: "grey",
    wheel: [
        "color=#1b1a20",
        "pointer=#d9c8ff",
        "index=dot",
        "skirt=plain",
        "skirt_color=#0e0e11",
    ],
    cap: [
        "color=#2e2d35",
        "pointer=#c4b5fd",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#0e0e11",
    ],
    accent: kit::PURPLE_LAMP,
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// The algorithm's program number on the unit: its place in the block's
/// `algorithm` order, from 1.
fn program() -> usize {
    VERB_ALGOS
        .iter()
        .position(|n| *n == NAME)
        .map_or(0, |i| i + 1)
}

/// The faceplate, `w` wide: enamel or anodising with a faint grain, three
/// pinstripes along the top in the section's purples, and — on the front —
/// the rack ear and its handle at the left. Returns the plate.
fn faceplate(b: &mut Builder<'_>, root: NodeId, w: f64, l: &Look, ear_w: f64) -> NodeId {
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
    let grain: Vec<Vec<(f64, f64)>> = (0..55)
        .map(|i| {
            let y = 1.5 + f64::from(i) * 3.0;
            vec![(0.0, y), (w, y)]
        })
        .collect();
    b.add(
        plate,
        ui::strokes("grain", &grain, 0.6, draw::solid(l.brush)),
    );
    for (i, c) in [kit::PURPLE_DEEP, kit::PURPLE, kit::PURPLE_LIGHT]
        .iter()
        .enumerate()
    {
        b.add(
            plate,
            draw::rect(
                "pinstripe",
                ear_w,
                3.0 + 2.6 * i as f64,
                w - ear_w,
                1.5,
                draw::solid(c),
            ),
        );
    }
    if ear_w > 0.0 {
        ear(b, plate, ear_w, l);
    }
    plate
}

/// The rack ear, `w` wide at the left: two slotted holes, the chrome
/// handle between them, and the fold where it meets the face.
fn ear(b: &mut Builder<'_>, plate: NodeId, w: f64, l: &Look) {
    b.add(
        plate,
        draw::rect("ear", 0.0, 0.0, w, kit::H, draw::linear(90.0, &l.ear)),
    );
    for cy in [18.0, kit::H - 18.0] {
        let slot = draw::rounded(
            draw::rect(
                "rack slot",
                w / 2.0 - 6.0,
                cy - 3.5,
                12.0,
                7.0,
                draw::solid("#060607"),
            ),
            3.5,
        );
        b.add(
            plate,
            draw::with_effects(
                slot,
                vec![
                    draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                    draw::drop_shadow("#ffffff22", 0.0, 1.0, 0.0),
                ],
            ),
        );
    }
    let (hx, hy, hh) = (w / 2.0 - 4.0, 34.0, kit::H - 68.0);
    for y in [hy - 3.0, hy + hh - 5.0] {
        b.add(
            plate,
            draw::rounded(
                draw::rect(
                    "handle foot",
                    hx - 2.0,
                    y,
                    12.0,
                    8.0,
                    draw::solid("#1a1a1d"),
                ),
                2.0,
            ),
        );
    }
    let bar = draw::rounded(
        draw::rect(
            "handle",
            hx,
            hy,
            8.0,
            hh,
            draw::linear(
                90.0,
                &[
                    (0.0, "#6e7177"),
                    (0.35, "#f1f2f4"),
                    (0.6, "#a9adb3"),
                    (1.0, "#55585e"),
                ],
            ),
        ),
        4.0,
    );
    b.add(
        plate,
        draw::with_effects(bar, vec![draw::drop_shadow("#00000090", 2.0, 3.0, 4.0)]),
    );
    ui::rule(b, plate, w, 0.0, 1.0, kit::H, l.rules.0);
    ui::rule(b, plate, w + 1.0, 0.0, 1.0, kit::H, l.rules.1);
}

/// Seven-segment digits that never change (the program number), lit over
/// their unlit "88", right-aligned in the box (`x`, `cy`, `w`).
fn segments(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy, w): (f64, f64, f64), size: f64) {
    let digits = |label: &str, text: &str, fill: &str| {
        let mut n = draw::text(
            label,
            text,
            x,
            cy - size * 0.65,
            w,
            size,
            fill,
            TextAlign::Right,
        );
        if let document_proto::NodeKind::Text(t) = &mut n.kind {
            t.font_family = Some(draw::family::DSEG7.into());
        }
        n
    };
    b.add(
        parent,
        digits("unlit", &"8".repeat(s.len()), &ui::alpha(LED, 0.12)),
    );
    b.add(
        parent,
        draw::emissive(digits("program", s, LED), draw::hex(LED), 1.0, kit::GLOW),
    );
}

/// A line of the dot-matrix display, lit, its left edge at `x`.
fn dots(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy, w): (f64, f64, f64), size: f64) {
    let t = draw::styled_text(
        "matrix",
        s,
        x,
        cy - size * 0.68,
        w,
        size,
        LED,
        TextAlign::Left,
        &draw::Lettering::family(draw::family::DOTO).weight(700.0),
    );
    b.add(parent, ui::glowing(t, LED, 0.8, kit::GLOW));
}

/// The delay lines, `rows` of `cols` LEDs in (`x`, `y`, `w`, `h`): each
/// row's lit dot is its tap, random-walking about its own place and
/// snapping LED to LED, a fading trail behind it. The walk widens with the
/// modulation and the whole matrix goes dark when the reverb is out.
fn lines(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    rows: u32,
    cols: u32,
) {
    let g = b.add(parent, draw::frame("Lines", x, y, w, h));
    let (px, py) = (w / f64::from(cols), h / f64::from(rows));
    let (dw, dh) = (px * 0.64, py * 0.56);
    let mut unlit = String::new();
    for r in 0..rows {
        for c in 0..cols {
            let (x0, y0) = (
                px * (f64::from(c) + 0.5) - dw / 2.0,
                py * (f64::from(r) + 0.5) - dh / 2.0,
            );
            unlit.push_str(&format!(
                "M {x0:.2} {y0:.2} h {dw:.2} v {dh:.2} h {:.2} Z ",
                -dw
            ));
        }
    }
    b.add(
        g,
        ui::area("unlit", unlit, w, h, draw::solid(&ui::alpha(LED, 0.1))),
    );
    let last = f64::from(cols - 1);
    let (lo, hi) = (px * 0.5 - dw / 2.0, px * (last + 0.5) - dw / 2.0);
    for r in 0..rows {
        let fr = f64::from(r);
        let j = |k: f64| crate::reverb::jitter(fr * 3.7 + k);
        let mean = 0.2 + 0.6 * j(0.0);
        let (w1, w2, p1, p2) = (
            0.3 + 0.4 * j(1.0),
            0.9 + 0.8 * j(2.0),
            6.0 * j(3.0),
            6.0 * j(4.0),
        );
        for (lag, glow) in [(0.0, 1.0), (0.22, 0.5), (0.44, 0.22)] {
            let dot = draw::rect(
                "tap",
                0.0,
                py * (fr + 0.5) - dh / 2.0,
                dw,
                dh,
                draw::solid(LED),
            );
            let dot = b.add(g, ui::glowing(dot, LED, 1.2 * glow, kit::GLOW));
            let t = format!("(time - {lag})");
            let walk = format!(
                "{mean:.3} + (0.1 + 0.3 * norm({VERB}/modulation)) * (0.6 * sin({t} * {w1:.2} + {p1:.2}) + 0.4 * sin({t} * {w2:.2} + {p2:.2}))"
            );
            let snapped = format!("floor(clamp({walk}, 0, 1) * {last} + 0.5) / {last}");
            motion::add(b, dot, Motion::new(Target::X).input(snapped).range(lo, hi));
            motion::add(
                b,
                dot,
                Motion::new(Target::Opacity)
                    .input(format!("param({VERB}/on)"))
                    .range(0.0, glow)
                    .dynamics(Dynamics::spring(300.0, 30.0)),
            );
        }
    }
}

/// The display's contents (`w` × `h`): the program — its number in seven
/// segments, its name on the matrix line — then, room permitting, the
/// pre-delay and size readouts, and under them the delay lines.
fn display(b: &mut Builder<'_>, win: NodeId, w: f64, h: f64, compact: bool) {
    let label = ui::alpha(LED, 0.6);
    ui::lit_text(
        b,
        win,
        "Program",
        8.0,
        10.0,
        60.0,
        5.5,
        &label,
        TextAlign::Left,
        700.0,
    );
    let n = format!("{:02}", program());
    segments(b, win, &n, (6.0, 30.0, 46.0), 22.0);
    if compact {
        dots(b, win, "RANDOM", (62.0, 29.0, w - 66.0), 13.0);
    } else {
        dots(b, win, "RANDOM SPACE", (66.0, 21.0, w - 70.0), 13.0);
        ui::lit_text(
            b,
            win,
            "Pre",
            66.0,
            40.0,
            24.0,
            5.5,
            &label,
            TextAlign::Left,
            700.0,
        );
        kit::seg_readout(b, win, &a("predelay"), (82.0, 40.0, 38.0), 11.0, 3, 0, LED);
        ui::lit_text(
            b,
            win,
            "ms",
            123.0,
            40.0,
            16.0,
            5.5,
            &label,
            TextAlign::Left,
            700.0,
        );
        ui::lit_text(
            b,
            win,
            "Size",
            154.0,
            40.0,
            24.0,
            5.5,
            &label,
            TextAlign::Left,
            700.0,
        );
        kit::seg_readout(b, win, &a("size"), (174.0, 40.0, 40.0), 11.0, 3, 2, LED);
    }
    ui::rule(b, win, 6.0, 52.0, w - 12.0, 0.6, &ui::alpha(LED, 0.25));
    let cols = if compact { 18 } else { 36 };
    lines(b, win, (6.0, 57.0, w - 12.0, h - 63.0), 8, cols);
}

/// A long-throw fader on `id`, its slot centred on `cx` from `top`, its
/// legend printed under it at `legend_y`.
fn fader(
    b: &mut Builder<'_>,
    plate: NodeId,
    (id, legend): (&str, &str),
    (cx, top, travel): (f64, f64, f64),
    legend_y: f64,
    l: &Look,
) {
    let p = Place::new(legend)
        .size(travel)
        .variant(l.fader)
        .legend(" ")
        .ink(Ink::Color(l.dim.into()))
        .bind(ui::bind(&a(id)));
    let f = ui::put(b, plate, "lever.fader", &p, cx, 0.0);
    ui::strip_legend(b, f);
    let fw = b.scene.node(f).map_or(0.0, |n| n.size.width);
    ui::move_to(b, f, cx - fw / 2.0, top);
    ui::silk(b, plate, legend, cx, legend_y, 6.5, l.ink, 800.0);
}

/// The program keys, four to a row, `kw` × `kh` each, `gap` apart from
/// (`x`, `y`): each a
/// `Select` over the block's algorithm, its lamp above it lit while it is
/// the one playing.
fn program_keys(
    b: &mut Builder<'_>,
    plate: NodeId,
    (x, y): (f64, f64),
    (kw, kh, gap): (f64, f64, f64),
    l: &Look,
) {
    let count = u32::try_from(VERB_ALGOS.len()).unwrap_or(u32::MAX);
    for (i, (algo, legend)) in KEYS.iter().enumerate() {
        let Some(index) = VERB_ALGOS
            .iter()
            .position(|n| n == algo)
            .and_then(|p| u32::try_from(p).ok())
        else {
            continue;
        };
        let root = b.add(
            plate,
            draw::frame(
                &format!("Program {legend}"),
                x + (kw + gap) * (i % 4) as f64,
                y + (kh + 16.0) * (i / 4) as f64,
                kw,
                kh + 12.0,
            ),
        );
        b.add(
            root,
            draw::circle("lamp well", kw / 2.0, 4.0, 3.4, draw::solid("#0b0a0d")),
        );
        let lamp = b.add(
            root,
            draw::emissive(
                draw::circle("lamp", kw / 2.0, 4.0, 2.4, draw::solid(LED)),
                draw::hex(LED),
                1.1,
                kit::GLOW,
            ),
        );
        let well = draw::rounded(
            draw::rect(
                "key well",
                -1.5,
                10.5,
                kw + 3.0,
                kh + 3.0,
                draw::solid("#00000066"),
            ),
            4.0,
        );
        b.add(root, well);
        let cap = draw::rounded(
            draw::rect("key", 0.0, 12.0, kw, kh, draw::linear(180.0, &l.key)),
            3.0,
        );
        b.add(
            root,
            draw::with_effects(
                cap,
                vec![
                    draw::drop_shadow("#00000070", 0.0, 1.5, 2.0),
                    draw::inner_shadow("#ffffff40", 0.0, 1.0, 0.0),
                ],
            ),
        );
        ui::silk(
            b,
            root,
            legend,
            kw / 2.0,
            12.0 + kh / 2.0,
            6.0,
            l.ink,
            800.0,
        );
        let c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select { index, count },
        })
        .slot(SlotRole::Led, lamp.0)
        .bound(ui::bind(&a("algorithm")));
        b.controls.insert(root, c);
    }
}

/// The nameplate's title: "Random" in a wide techno face, as the unit's
/// badge has it.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let l = draw::Lettering::family(draw::family::MICHROMA).weight(400.0);
    b.add(
        t,
        draw::styled_text(
            "title",
            NAME,
            2.0,
            h / 2.0 - size * 0.68,
            80.0,
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

/// The face at `tier`: one design — the faceplate, its display and its
/// lettering — laid out for the room. On a phone the display narrows to
/// the program and the lines, and the faders give way to the standard
/// Time knob and two knobs (Size, Level); the top right stays clear for
/// the nameplate.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let ear_w = if small_room { 22.0 } else { 30.0 };
        let plate = faceplate(b, root, w, l, ear_w);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);

        // ── The display ────────────────────────────────────────────────
        let (dx, dy) = (ear_w + 16.0, 32.0);
        let dw = if small_room {
            kit::PHONE_PICTURE_R - dx - 4.0
        } else {
            282.0
        };
        let dh = kit::H - dy - 14.0;
        let mark = draw::Lettering::family(draw::family::MICHROMA).weight(400.0);
        b.add(
            plate,
            draw::styled_text(
                "maker",
                if small_room {
                    "Digital Reverb"
                } else {
                    "Digital Reverberator"
                },
                dx,
                13.0,
                200.0,
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
                "Model 16 · 16-bit · random-walk lines",
                dx + 150.0,
                18.0,
                5.5,
                l.dim,
                700.0,
            );
        }
        let win = ui::display(b, plate, "Display", dx, dy, dw, dh, LED, "glass");
        display(b, win, dw, dh, small_room);
        kit::scale_children(b, win);
        ui::glass(b, win);

        if small_room {
            kit::nameplate_top_right(
                b,
                plate,
                VERB,
                tier,
                (6.0, 30.0),
                92.0,
                220.0,
                12.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 30.0, 13.0, l.ink),
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
                88.0,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 26.0, 11.0, l.ink),
            );
            // The data wheel: the Time knob.
            let (tx, ty, td) = (dx + dw + 60.0, 68.0, 52.0);
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
            // The fader bank.
            let fx = tx + 74.0;
            ui::rule(b, plate, fx - 20.0, 30.0, 1.0, kit::H - 44.0, l.rules.0);
            ui::rule(b, plate, fx - 19.0, 30.0, 1.0, kit::H - 44.0, l.rules.1);
            let bank = [
                ("predelay", "Pre-Dly"),
                ("size", "Size"),
                ("damping", "HF Damp"),
                ("modulation", "Drift"),
                ("level", "Level"),
            ];
            for (i, f) in bank.iter().enumerate() {
                fader(
                    b,
                    plate,
                    *f,
                    (fx + 38.0 * i as f64, 30.0, 92.0),
                    kit::H - 12.0,
                    l,
                );
            }
            // The program keys, under the nameplate.
            let kx = fx + 38.0 * 4.0 + 34.0;
            ui::rule(b, plate, kx - 16.0, 46.0, 1.0, kit::H - 60.0, l.rules.0);
            ui::rule(b, plate, kx - 15.0, 46.0, 1.0, kit::H - 60.0, l.rules.1);
            program_keys(b, plate, (kx, 46.0), (36.0, 20.0, 8.0), l);
            ui::silk(
                b,
                plate,
                "Program",
                kx + 2.0 * 36.0 + 1.5 * 8.0,
                128.0,
                6.5,
                l.ink,
                800.0,
            );
            ui::silk(
                b,
                plate,
                "Algorithm select",
                kx + 2.0 * 36.0 + 1.5 * 8.0,
                139.0,
                5.5,
                l.dim,
                700.0,
            );
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
        // Responsive: the display (and its bezel) takes the width.
        let stretch = kit::with_sibling(b, plate, win, -1);
        kit::responsive(b, plate, &stretch);
    });
    let mut f = kit::reverb_face(kit::tier_file("95-verb-random", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the same faceplate without the ear — the advanced
/// controls across it on small knobs, the nameplate, the bypass and the
/// arrow where they are on the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let plate = faceplate(b, root, w, l, 0.0);
        let knobs = [
            ("predelay", "Pre-Delay"),
            ("modulation", "Drift"),
            ("damping", "HF Damp"),
            ("low_cut", "Low Cut"),
            ("high_cut", "High Cut"),
            ("duck", "Duck"),
            ("tone", "Tone"),
            ("duck_threshold", "Duck Thr"),
            ("duck_release", "Duck Rel"),
            ("pan_a", "Pan"),
        ];
        kit::back_knobs(b, plate, tier, &knobs, l.ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &l.cap, l.ink, l.dim);
        });
        let (np, size, title_size) = if small_room {
            ((6.0, 30.0), 12.0, 13.0)
        } else {
            ((12.0, 26.0), 10.0, 11.0)
        };
        kit::head(b, plate, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                if small_room { 92.0 } else { 88.0 },
                if small_room { 220.0 } else { 200.0 },
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, np.1, title_size, l.ink),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "In");
        });
        kit::flip_arrow(b, plate, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.25)));
    });
    let mut f = kit::reverb_face(kit::tier_file("95-verb-random-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the display the whole top row, every control on the
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
        let grain: Vec<Vec<(f64, f64)>> = (0..110)
            .map(|i| {
                vec![
                    (0.0, 1.5 + f64::from(i) * 3.0),
                    (kit::TALL_W, 1.5 + f64::from(i) * 3.0),
                ]
            })
            .collect();
        b.add(
            plate,
            ui::strokes("grain", &grain, 0.6, draw::solid(l.brush)),
        );
        for (i, c) in [kit::PURPLE_DEEP, kit::PURPLE, kit::PURPLE_LIGHT]
            .iter()
            .enumerate()
        {
            b.add(
                plate,
                draw::rect(
                    "pinstripe",
                    0.0,
                    kit::H + 3.0 + 2.6 * i as f64,
                    kit::TALL_W,
                    1.5,
                    draw::solid(c),
                ),
            );
        }
        let (dx, dy, dw, dh) = kit::TALL_PICTURE;
        let (dx, dy, dw, dh) = (dx + 4.0, dy + 4.0, dw - 8.0, dh - 8.0);
        let win = ui::display(b, plate, "Display", dx, dy, dw, dh, LED, "glass");
        display(b, win, dw, dh, false);
        ui::glass(b, win);
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("predelay", "Pre-Delay"),
                ("size", "Size"),
                ("damping", "HF Damp"),
                ("modulation", "Drift"),
                ("level", "Level"),
            ],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: if dark { "#1f2024e6" } else { "#ded6c4e6" },
            bypass: ("purple", "In"),
            title_w: 92.0,
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
            |b, tn| title(b, tn, 30.0, 13.0, l.ink),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file("95-verb-random", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
