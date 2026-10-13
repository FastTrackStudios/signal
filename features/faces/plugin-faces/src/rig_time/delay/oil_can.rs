//! Oil Can: the electrostatic oil-can echo (a Tel-Ray's), the can seen
//! end on with its cap off. Inside the steel rim a disc turns through a
//! pool of amber oil; the record brush lays a charge on its edge, the disc
//! carries it round to the playback brush — a delay time later, the disc
//! turning faster the shorter the time — and the felt wiper in the oil
//! wipes it off. The oil's surface rolls with the warble (the modulation's
//! rate and depth); the playback brush's lamp flashes on each repeat.
//! Beside the can, the echoes as an oil can smears them: ripples a delay
//! time apart, each lower by the feedback and wider and wobblier than the
//! last.
//!
//! Enamel: cream, or oxblood; the can is steel.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use super::machine::{self, Look, Machine, ON, PERIOD, Room};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

const OIL: [(f64, &str); 3] = [(0.0, "#e0a24a"), (0.5, "#a8621c"), (1.0, "#5a2e0c")];
const DIM: &str = "#a0927f";
/// The brushes' places on the disc (0 = up, clockwise): a delay time apart.
const REC: f64 = -64.0;
const PLAY: f64 = 56.0;

const MACHINE: Machine = Machine {
    name: "Oil Can",
    file: "69-dly-oil-can",
    light: Look {
        ink: "#3a2618",
        dim: "#8a715a",
        panel: [(0.0, "#f2e9d6"), (0.6, "#e5d7ba"), (1.0, "#d4c29f")],
        edge: "#bba67f",
        texture: "#7a5a3016",
        well: [(0.0, "#211812"), (1.0, "#130d09")],
        knob: [
            "color=#3a2618",
            "pointer=#f2e9d6",
            "index=line",
            "index_w=0.11",
            "skirt=plain",
            "skirt_color=#bfa98a",
        ],
        wheel: [
            "color=#efe6d2",
            "pointer=#1e3a8a",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#8a715a",
        ],
        accent: kit::BLUE_DEEP,
        scrim: "#e5d7bae6",
    },
    dark: Look {
        ink: "#f0e2cc",
        dim: "#b39c80",
        panel: [(0.0, "#472a22"), (0.6, "#341d17"), (1.0, "#22120e")],
        edge: "#5c3a2e",
        texture: "#ffffff0b",
        well: [(0.0, "#160f0b"), (1.0, "#0b0705")],
        knob: [
            "color=#1e120e",
            "pointer=#f0e2cc",
            "index=line",
            "index_w=0.11",
            "skirt=plain",
            "skirt_color=#7a5644",
        ],
        wheel: [
            "color=#e9dcc4",
            "pointer=#1e3a8a",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#6b4a3a",
        ],
        accent: kit::BLUE_LIGHT,
        scrim: "#341d17e6",
    },
    title: (draw::family::PACIFICO, 400.0, 16.0, 0.0, 0.6),
    front: [
        ("feedback", "Repeats"),
        ("mod_depth", "Warble"),
        ("high_cut", "Tone"),
        ("level", "Level"),
    ],
    phone: [
        ("feedback", "Repeats"),
        ("mod_depth", "Warble"),
        ("level", "Level"),
    ],
    main: [
        ("feedback", "Repeats"),
        ("mod_depth", "Warble"),
        ("high_cut", "Tone"),
        ("level", "Level"),
    ],
    picture,
    glass: false,
};

/// The warble now, −1…1 times its depth (`phase` sets one apart from the
/// next).
fn warble(phase: f64) -> String {
    format!("sin(time * 6.2832 * param({DLY}/mod_rate) + {phase}) * param({DLY}/mod_depth)")
}

/// The can at (`cx`, `cy`), its rim radius `rr`.
fn can(b: &mut Builder<'_>, well: NodeId, (cx, cy, rr): (f64, f64, f64)) {
    b.add(
        well,
        draw::circle(
            "shadow",
            cx + 1.0,
            cy + 3.0,
            rr + 5.0,
            draw::radial_at(
                0.5,
                0.5,
                0.5,
                &[(0.0, "#000000b0"), (0.85, "#00000070"), (1.0, "#00000000")],
            ),
        ),
    );
    // The rim: rolled steel, catching the light.
    let rim = b.add(
        well,
        draw::circle(
            "rim",
            cx,
            cy,
            rr,
            draw::linear(
                125.0,
                &[
                    (0.0, "#f1f3f4"),
                    (0.3, "#9ea5ab"),
                    (0.55, "#dfe2e5"),
                    (0.8, "#7d848a"),
                    (1.0, "#b8bec3"),
                ],
            ),
        ),
    );
    kit::steel(b, rim);
    let r = rr - 7.0;
    // Inside: the dark of the can, the oil, the disc.
    let mut inside = draw::frame("inside", cx - r, cy - r, 2.0 * r, 2.0 * r);
    inside.fills = vec![draw::radial_at(
        0.5,
        0.4,
        0.7,
        &[(0.0, "#2a1d14"), (1.0, "#0a0604")],
    )];
    inside.corner_radii = frame::document::proto::CornerRadii::uniform(r);
    inside.clips_content = true;
    inside.effects = vec![draw::inner_shadow("#000000e0", 0.0, 2.0, 5.0)];
    let inside = b.add(well, inside);
    // The disc, turning: a dark bronze face, its vanes, the charge on its
    // edge carried from the record brush to the playback brush.
    let dr = r * 0.82;
    let disc = b.add(
        inside,
        draw::frame("disc", r - dr, r - dr, 2.0 * dr, 2.0 * dr),
    );
    b.add(
        disc,
        draw::circle(
            "face",
            dr,
            dr,
            dr,
            draw::radial_at(
                0.4,
                0.35,
                0.9,
                &[(0.0, "#6b5440"), (0.7, "#3a2a1c"), (1.0, "#22170e")],
            ),
        ),
    );
    let vanes: Vec<Vec<(f64, f64)>> = (0..10)
        .map(|i| {
            let a = f64::from(i) * 36.0;
            vec![
                draw::polar(dr, dr, dr * 0.3, a),
                draw::polar(dr, dr, dr * 0.92, a + 8.0),
            ]
        })
        .collect();
    b.add(
        disc,
        ui::strokes("vanes", &vanes, 1.4, draw::solid("#0d080580")),
    );
    // The charge: the arc of the edge between the brushes, lit, written
    // at the record brush.
    let charge: Vec<Vec<(f64, f64)>> = (0..14)
        .map(|i| {
            let a = REC + (PLAY - REC) * f64::from(i) / 14.0;
            (0..=2)
                .map(|k| draw::polar(dr, dr, dr - 3.0, a + 3.5 * f64::from(k)))
                .collect()
        })
        .collect();
    b.add(
        disc,
        ui::glowing(
            ui::strokes(
                "charge",
                &charge,
                2.6,
                draw::solid(&ui::alpha(kit::BLUE_LIGHT, 0.85)),
            ),
            kit::BLUE,
            0.9,
            kit::GLOW,
        ),
    );
    // Round once in three delay times (the brushes are 120° apart).
    let turn = format!("fract(time * 1000 / (3 * {PERIOD}))");
    motion::add(
        b,
        disc,
        Motion::new(Target::Rotate)
            .input(turn)
            .range(0.0, 360.0)
            .pivot(0.5, 0.5),
    );
    // The oil: the lower part of the can, its surface rolling with the
    // warble.
    let pool = b.add(
        inside,
        draw::frame("oil", -6.0, r * 1.12, 2.0 * r + 12.0, r * 1.2),
    );
    let wave: Vec<(f64, f64)> = (0..=24)
        .map(|k| {
            let x = (2.0 * r + 12.0) * f64::from(k) / 24.0;
            (x, 3.0 + 2.0 * (f64::from(k) * 0.9).sin())
        })
        .collect();
    let mut d = format!("M 0 {:.1} ", wave[0].1);
    for (x, y) in &wave {
        d += &format!("L {x:.1} {y:.1} ");
    }
    d += &format!(
        "L {:.1} {:.1} L 0 {:.1} Z",
        2.0 * r + 12.0,
        r * 1.2,
        r * 1.2
    );
    b.add(
        pool,
        draw::with_opacity(
            draw::path(
                "oil body",
                0.0,
                0.0,
                2.0 * r + 12.0,
                r * 1.2,
                d,
                draw::linear(180.0, &OIL),
            ),
            0.88,
        ),
    );
    b.add(
        pool,
        ui::strokes("surface", &[wave], 1.2, draw::solid("#ffd89a")),
    );
    motion::add(
        b,
        pool,
        Motion::new(Target::Y)
            .input(format!("0.5 + 0.5 * {}", warble(0.0)))
            .range(-3.0, 3.0),
    );
    motion::add(
        b,
        pool,
        Motion::new(Target::Rotate)
            .input(format!("0.5 + 0.5 * {}", warble(1.3)))
            .range(-5.0, 5.0)
            .pivot(0.5, 0.0),
    );
    // The felt wiper, in the oil at the bottom.
    let (fx, fy) = draw::polar(r, r, dr - 1.0, 180.0);
    b.add(
        inside,
        draw::rounded(
            draw::rect(
                "felt wiper",
                fx - 9.0,
                fy - 3.0,
                18.0,
                6.0,
                draw::solid("#3d3a32"),
            ),
            2.0,
        ),
    );
    // The hub.
    b.add(
        well,
        draw::with_effects(
            draw::circle(
                "hub",
                cx,
                cy,
                5.5,
                draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#f0f2f4"), (1.0, "#4c5157")]),
            ),
            vec![draw::drop_shadow("#00000090", 0.0, 1.0, 2.0)],
        ),
    );
    // The brushes: sprung wires from the rim to the disc's edge.
    for (deg, tip, name) in [
        (REC, "#d04848", "record brush"),
        (PLAY, kit::BLUE_LIGHT, "playback brush"),
    ] {
        let (x0, y0) = draw::polar(cx, cy, rr - 2.0, deg + 10.0);
        let (x1, y1) = draw::polar(cx, cy, dr + 1.0, deg);
        b.add(
            well,
            ui::strokes(
                name,
                &[vec![
                    (x0, y0),
                    ((x0 + x1) / 2.0 + 1.5, (y0 + y1) / 2.0 - 1.5),
                    (x1, y1),
                ]],
                1.6,
                draw::solid("#d8c08a"),
            ),
        );
        b.add(well, draw::circle("tip", x1, y1, 2.2, draw::solid(tip)));
    }
    // The playback brush's lamp, on the rim, flashing on each repeat.
    let (lx, ly) = draw::polar(cx, cy, rr - 3.5, PLAY + 26.0);
    b.add(
        well,
        draw::circle(
            "lamp socket",
            lx,
            ly,
            3.6,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#e8eaec"), (1.0, "#5a6066")]),
        ),
    );
    let lamp = b.add(
        well,
        draw::emissive(
            draw::circle("lamp", lx, ly, 2.5, draw::solid(kit::BLUE_LAMP)),
            draw::hex(kit::BLUE_LAMP),
            1.2,
            kit::BLOOM,
        ),
    );
    motion::add(
        b,
        lamp,
        Motion::new(Target::Opacity)
            .input(machine::flash(1))
            .range(0.25, 1.0),
    );
}

/// The echoes as the oil smears them, in (`x, y, w, h`): ripples a delay
/// time apart, each lower by the feedback, wider and wobblier than the last.
fn ripples(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let mut f = draw::frame("Ripples", x, y, w, h);
    f.clips_content = true;
    let f = b.add(well, f);
    let mid = h / 2.0;
    b.add(
        f,
        ui::strokes(
            "axis",
            &[vec![(0.0, mid), (w, mid)]],
            0.8,
            draw::solid(&ui::alpha(DIM, 0.35)),
        ),
    );
    ui::silk_left(b, f, "0", 0.0, h - 4.0, 5.0, DIM, 700.0);
    ui::silk_right(b, f, "2.5 s", w, h - 4.0, 5.0, DIM, 700.0);
    b.add(
        f,
        ui::glowing(
            draw::rect(
                "note",
                1.0,
                6.0,
                2.0,
                h - 16.0,
                draw::solid(kit::BLUE_LIGHT),
            ),
            kit::BLUE_LIGHT,
            1.2,
            kit::GLOW,
        ),
    );
    let window = 2500.0;
    for i in 1..=8_u32 {
        let fi = f64::from(i);
        // Each a vertical squiggle: wider, more turns, the further down
        // the line.
        let amp = 1.5 + 1.1 * fi;
        let cw = 2.0 * amp + 4.0;
        let col = b.add(
            f,
            draw::frame(&format!("repeat {i}"), 0.0, 6.0, cw, h - 16.0),
        );
        let ch = h - 16.0;
        let turns = 2.0 + 0.6 * fi;
        let pts: Vec<(f64, f64)> = (0..=48)
            .map(|k| {
                let t = f64::from(k) / 48.0;
                let env = (std::f64::consts::PI * t).sin();
                (
                    cw / 2.0 + amp * env * (t * turns * std::f64::consts::TAU).sin(),
                    t * ch,
                )
            })
            .collect();
        let wob = b.add(col, draw::frame("wobble", 0.0, 0.0, cw, ch));
        let color = if i == 1 { kit::BLUE_LIGHT } else { kit::BLUE };
        let ripple = ui::strokes("ripple", &[pts], 1.6, draw::solid(color));
        b.add(
            wob,
            if i <= 3 {
                ui::glowing(ripple, color, 1.0, kit::GLOW)
            } else {
                ripple
            },
        );
        motion::add(
            b,
            wob,
            Motion::new(Target::X)
                .input(format!("0.5 + 0.5 * {}", warble(fi * 0.8)))
                .range(-0.6 * fi, 0.6 * fi),
        );
        let at = format!("clamp({fi} * param({DLY}/time) / {window}, 0, 1.1)");
        motion::add(
            b,
            col,
            Motion::new(Target::X)
                .input(at)
                .range(2.0 - cw / 2.0, w - cw / 2.0)
                .dynamics(machine::spring()),
        );
        let fall = format!(
            "pow(param({DLY}/feedback), {}) * (0.25 + 0.75 * {ON})",
            i - 1
        );
        motion::add(
            b,
            col,
            Motion::new(Target::StretchY)
                .input(fall)
                .range(0.08, 1.0)
                .pivot(0.5, 0.5)
                .dynamics(machine::spring()),
        );
    }
}

fn picture(
    b: &mut Builder<'_>,
    well: NodeId,
    (w, h): (f64, f64),
    _l: &Look,
    room: Room,
    _dark: bool,
) {
    let (cx, cy, rr) = match room {
        Room::Phone => (w / 2.0, h / 2.0 + 4.0, 52.0),
        Room::Lane => (78.0, h / 2.0 + 2.0, 54.0),
        Room::Tall => (68.0, h / 2.0 + 2.0, 46.0),
    };
    can(b, well, (cx, cy, rr));
    let x = cx + rr + 30.0;
    match room {
        Room::Phone => {}
        Room::Lane => ripples(b, well, (x, 10.0, w - x - 14.0, h - 14.0)),
        Room::Tall => ripples(b, well, (x, 40.0, w - x - 16.0, h - 44.0)),
    }
    ui::silk_left(
        b,
        well,
        "Oil can · electrostatic",
        8.0,
        9.0,
        5.5,
        DIM,
        800.0,
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    MACHINE.face_at(dark, tier)
}

#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    MACHINE.back_at(dark, tier)
}

#[must_use]
pub fn tall(dark: bool) -> Face {
    MACHINE.tall(dark)
}
