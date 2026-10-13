//! Velvet: the reverb built from velvet noise — a sparse train of single
//! impulses, each one up or down at random, its density setting how smooth
//! the tail sounds — drawn as what the name says: a jeweller's velvet tray
//! with the impulses pinned into it. Each pin stands up or down from the
//! silver thread at its moment, as tall as the tail is loud there; the
//! train runs out further as the decay grows (the pearl-headed pin at its
//! end is the decay, to drag), and more pins come in as the size (the
//! density) grows. Now and then a pin head catches the light.
//!
//! Two finishes: light, a dove-grey case lined in lilac velvet; dark, black
//! velvet in a plum case.

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use super::shell::{self, Look, Room, Unit};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const UNIT: Unit = Unit {
    name: "Velvet",
    base: "92-verb-velvet",
    title_w: 92.0,
    legend: "On",
    lane: [
        ("size", "Density"),
        ("predelay", "Pre-Delay"),
        ("level", "Level"),
    ],
    tall: [
        ("size", "Density"),
        ("predelay", "Pre-Delay"),
        ("damping", "HF Damp"),
        ("level", "Level"),
    ],
    look,
    ground,
    picture,
    title,
};

/// The glints: the section's purple.
const GLINT: &str = kit::PURPLE_LAMP;
const SILVER: &str = "#e9e6f0";

struct Finish {
    look: Look,
    case: [(f64, &'static str); 3],
    stitch: &'static str,
    velvet: [(f64, &'static str); 3],
    pile: &'static str,
    print: &'static str,
}

const LIGHT: Finish = Finish {
    look: Look {
        ink: "#352d42",
        dim: "#756c86",
        accent: "#6a3fc4",
        scrim: "#e4e0e8e6",
        knob: [
            "color=#ecebf0",
            "pointer=#5b3aa8",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#a9a4b4",
        ],
        wheel: [
            "color=#f4f2f7",
            "pointer=#5b3aa8",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#8b85a0",
        ],
    },
    case: [(0.0, "#efedf1"), (0.6, "#e1dee6"), (1.0, "#cfcad6")],
    stitch: "#a59bb8",
    velvet: [(0.0, "#4a2f6e"), (0.5, "#3a2259"), (1.0, "#2a1842")],
    pile: "#ffffff0d",
    print: "#c9bce0",
};

const DARK: Finish = Finish {
    look: Look {
        ink: "#efe7f7",
        dim: "#a092b4",
        accent: kit::PURPLE_LAMP,
        scrim: "#2a1830e6",
        knob: [
            "color=#1e1622",
            "pointer=#e2d6f5",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#0c080e",
        ],
        wheel: [
            "color=#d9d5e0",
            "pointer=#3b1d6e",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#4b3a56",
        ],
    },
    case: [(0.0, "#3a2440"), (0.6, "#2c1a31"), (1.0, "#1d1021")],
    stitch: "#6e4f78",
    velvet: [(0.0, "#1c1424"), (0.5, "#130d19"), (1.0, "#0b070f")],
    pile: "#ffffff0a",
    print: "#9a8cb0",
};

fn finish(dark: bool) -> &'static Finish {
    if dark { &DARK } else { &LIGHT }
}

fn look(dark: bool) -> &'static Look {
    &finish(dark).look
}

/// The impulses: (moment 0..1 of the window, up or down, height 0..1 of
/// the half, rank — the density at which it comes in).
fn pins(n: u32) -> Vec<(f64, f64, f64, f64)> {
    (0..n)
        .map(|i| {
            let f = f64::from(i);
            let j = |k: f64| crate::reverb::jitter(f * 2.13 + k);
            // One per grid cell, anywhere in it: velvet noise.
            let t = (f + 0.15 + 0.7 * j(1.0)) / f64::from(n);
            let sign = if j(2.0) > 0.5 { 1.0 } else { -1.0 };
            let tail = (-3.2 * t).exp() * (0.7 + 0.3 * j(3.0));
            (t, sign, tail, j(4.0))
        })
        .collect()
}

/// How far the train runs: the decay.
fn reach() -> String {
    format!("lerp(0.2, 1, norm({VERB}/decay))")
}

/// The case: lacquered, its lid's stitching round the edge.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), dark: bool) -> NodeId {
    let f = finish(dark);
    let panel = shell::panel(b, root, (w, h), &f.case, 10.0);
    let mut s = draw::rect(
        "stitching",
        4.0,
        4.0,
        w - 8.0,
        h - 8.0,
        draw::solid("#00000000"),
    );
    s.fills.clear();
    s.corner_radii = frame::document::proto::CornerRadii::uniform(7.0);
    s.stroke = frame::document::proto::Stroke {
        paints: vec![draw::solid(f.stitch)],
        weight: 0.8,
        dash_pattern: vec![3.0, 2.5],
        ..frame::document::proto::Stroke::default()
    };
    b.add(panel, s);
    panel
}

/// The tray in (x, y, w, h) on `panel`: the velvet, its pile, the silver
/// thread, the envelope and the pins. Returns the tray.
fn tray(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    room: Room,
    f: &Finish,
) -> NodeId {
    let mut t = draw::frame("Tray", x, y, w, h);
    t.fills = vec![draw::linear(165.0, &f.velvet)];
    t.clips_content = true;
    t.corner_radii = frame::document::proto::CornerRadii::uniform(6.0);
    t.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.5, 8.0)];
    let t = b.add(panel, t);
    // The pile: crushed, its sheen in soft diagonal bands.
    b.add(
        t,
        draw::rect(
            "sheen",
            0.0,
            0.0,
            w,
            h,
            draw::linear(
                110.0,
                &[
                    (0.0, "#ffffff00"),
                    (0.3, "#ffffff10"),
                    (0.42, "#ffffff00"),
                    (0.7, "#ffffff0c"),
                    (0.85, "#ffffff00"),
                ],
            ),
        ),
    );
    let pile: Vec<Vec<(f64, f64)>> = (0..(w / 3.0) as i32)
        .map(|i| {
            let px = f64::from(i) * 3.0 + crate::reverb::jitter(f64::from(i)) * 2.0;
            vec![(px, 0.0), (px + 6.0, h)]
        })
        .collect();
    b.add(t, ui::strokes("pile", &pile, 1.0, draw::solid(f.pile)));
    let (x0, mid) = (12.0, h / 2.0);
    let span = w - x0 - 16.0;
    // In the tall face the nameplate covers the top right: the train keeps
    // below it there (its pins are short by then anyway).
    let half = h / 2.0 - 12.0;
    ui::silk_left(
        b,
        t,
        "Velvet noise · ±1 impulses",
        8.0,
        8.0,
        5.5,
        f.print,
        800.0,
    );
    // The thread.
    b.add(
        t,
        ui::line_node(
            "thread",
            &[(x0, mid), (w - 6.0, mid)],
            0.8,
            &ui::alpha(SILVER, 0.55),
        ),
    );
    // The envelope, top and bottom, out to the decay: stretched, it is
    // lines alone.
    let env = b.add(t, draw::frame("Envelope", x0, mid - half, span, 2.0 * half));
    let curve = |sign: f64| -> Vec<(f64, f64)> {
        (0..=40)
            .map(|i| {
                let u = f64::from(i) / 40.0;
                (u * span, half - sign * half * (-3.2 * u).exp())
            })
            .collect()
    };
    // Under it, the tail the ear hears from the sparse train: smooth.
    let mut d = format!("M 0 {half:.1} ");
    for k in [1.0, -1.0] {
        for (px, py) in if k > 0.0 {
            curve(1.0)
        } else {
            curve(-1.0).into_iter().rev().collect()
        } {
            d += &format!("L {px:.1} {py:.1} ");
        }
    }
    d.push('Z');
    b.add(
        env,
        draw::path(
            "heard",
            0.0,
            0.0,
            span,
            2.0 * half,
            d,
            draw::linear(
                90.0,
                &[
                    (0.0, &ui::alpha(GLINT, 0.32)),
                    (1.0, &ui::alpha(GLINT, 0.0)),
                ],
            ),
        ),
    );
    let mut e = ui::strokes(
        "envelope",
        &[curve(1.0), curve(-1.0)],
        0.7,
        draw::solid(&ui::alpha(SILVER, 0.4)),
    );
    e.stroke.dash_pattern = vec![2.0, 3.0];
    b.add(env, e);
    motion::add(
        b,
        env,
        Motion::new(Target::StretchX)
            .input(reach())
            .range(0.0, 1.0)
            .pivot(0.0, 0.5)
            .dynamics(Dynamics::spring(200.0, 26.0)),
    );
    // The pins: each at its moment (the train stretched by the decay), up
    // or down, coming in with the density.
    let n = if room == Room::Phone { 26 } else { 56 };
    for (i, (moment, sign, tall, rank)) in pins(n).iter().enumerate() {
        let len = (half * tall).max(3.0);
        let pin = b.add(
            t,
            draw::frame(
                &format!("pin {i}"),
                x0 - 3.0,
                if *sign > 0.0 {
                    mid - len - 3.0
                } else {
                    mid - 3.0
                },
                6.0,
                len + 6.0,
            ),
        );
        let (stem, head) = if *sign > 0.0 {
            ((3.0, len + 3.0), (3.0, 3.0))
        } else {
            ((3.0, 3.0), (3.0, len + 3.0))
        };
        b.add(pin, ui::line_node("stem", &[stem, head], 0.9, "#c9c4d4"));
        b.add(
            pin,
            draw::circle(
                "head",
                head.0,
                head.1,
                2.0,
                draw::radial_at(
                    0.35,
                    0.3,
                    0.8,
                    &[(0.0, "#ffffff"), (0.6, SILVER), (1.0, "#8d879a")],
                ),
            ),
        );
        motion::add(
            b,
            pin,
            Motion::new(Target::X)
                .input(format!("{moment:.4} * {}", reach()))
                .range(0.0, span)
                .dynamics(Dynamics::spring(200.0, 26.0)),
        );
        motion::add(
            b,
            pin,
            Motion::new(Target::Opacity)
                .input(format!(
                    "clamp((lerp(0.25, 1.05, norm({VERB}/size)) - {rank:.3}) * 12, 0, 1)"
                ))
                .range(0.0, 1.0),
        );
        // Every fifth pin's head catches the light now and then.
        if i % 5 == 2 {
            let glint = b.add(
                pin,
                ui::glowing(
                    draw::circle("glint", head.0, head.1, 1.6, draw::solid("#ffffff")),
                    GLINT,
                    1.6,
                    kit::GLOW,
                ),
            );
            let when = format!(
                "pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 16) * param({VERB}/on)",
                0.9 + 0.8 * crate::reverb::jitter(f64::from(i as u32) * 5.1),
                f64::from(i as u32) * 1.7
            );
            motion::add(
                b,
                glint,
                Motion::new(Target::Opacity).input(when).range(0.0, 1.0),
            );
        }
    }
    // The pearl-headed pin at the train's end: drag it for the decay.
    let ph = 2.0 * half + 8.0;
    shell::decay_handle(
        b,
        t,
        "Tail pin",
        (x0 + 0.2 * span, x0 + span, mid - half - 4.0),
        (16.0, ph),
        |b, p| {
            b.add(
                p,
                ui::line_node(
                    "shaft",
                    &[(8.0, 4.0), (8.0, ph - 4.0)],
                    1.0,
                    &ui::alpha(SILVER, 0.8),
                ),
            );
            b.add(
                p,
                ui::glowing(
                    draw::circle(
                        "pearl",
                        8.0,
                        ph / 2.0,
                        5.0,
                        draw::radial_at(
                            0.35,
                            0.3,
                            0.9,
                            &[(0.0, "#ffffff"), (0.5, "#efe4ff"), (1.0, "#a58fd6")],
                        ),
                    ),
                    GLINT,
                    0.7,
                    kit::BLOOM,
                ),
            );
            ui::plain(b, p, "Tail", 8.0, ph - 2.0, 5.0, f.print, 800.0);
        },
    );
    t
}

/// The picture: the tray, under glass, in a silver rim.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    room: Room,
    dark: bool,
) -> Vec<NodeId> {
    let f = finish(dark);
    let t = tray(b, panel, (x, y, w, h), room, f);
    kit::glass(b, t);
    let mut rim = draw::rect(
        "rim",
        x - 2.0,
        y - 2.0,
        w + 4.0,
        h + 4.0,
        draw::solid("#00000000"),
    );
    rim.fills.clear();
    rim.corner_radii = frame::document::proto::CornerRadii::uniform(7.0);
    rim.stroke = frame::document::proto::Stroke {
        paints: vec![draw::linear(
            100.0,
            &[
                (0.0, "#f3f2f6"),
                (0.45, "#8f8a9a"),
                (0.6, "#e6e3ec"),
                (1.0, "#7d7888"),
            ],
        )],
        weight: 2.5,
        ..frame::document::proto::Stroke::default()
    };
    let rim = b.add(panel, rim);
    vec![t, rim]
}

/// The nameplate's title: "Velvet" in a script.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, dark: bool) {
    let l = draw::Lettering::family(draw::family::DANCING_SCRIPT).weight(700.0);
    crate::suite_studio_blues::parts::set(
        b,
        t,
        "Velvet",
        38.0,
        h / 2.0,
        size * 1.1,
        finish(dark).look.ink,
        &l,
        0.02,
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the tray beside the controls.
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
