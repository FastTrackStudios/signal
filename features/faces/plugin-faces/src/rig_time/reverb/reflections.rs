//! Reflections: early reflections and nothing after them — drawn as the
//! instrument that lives on echoes, a ranging set. On the round scope (the
//! PPI) the source sits at the centre and every reflection is a blip at its
//! bearing and its range: the sweep goes round, each blip flares as it
//! passes and its afterglow lasts as long as the decay. The blips stand
//! further out as the room grows. Beside it, the A-scope: the same echoes
//! along one range trace, spikes out of the grass, and the range gate —
//! drag it for the decay; what lies past it fades.
//!
//! Two finishes: light, a naval console's pale grey; dark, gunmetal.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Look, Room, Unit};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const UNIT: Unit = Unit {
    name: "Reflections",
    base: "91-verb-reflections",
    title_w: 128.0,
    legend: "On",
    lane: [
        ("size", "Range"),
        ("predelay", "Pre-Delay"),
        ("level", "Level"),
    ],
    tall: [
        ("size", "Range"),
        ("predelay", "Pre-Delay"),
        ("damping", "HF Damp"),
        ("level", "Level"),
    ],
    look,
    ground,
    picture,
    title,
};

/// The phosphor: the section's purple.
const PHOSPHOR: &str = kit::PURPLE_LAMP;
const HOT: &str = "#efe6ff";

struct Finish {
    look: Look,
    console: [(f64, &'static str); 3],
    rivet: &'static str,
    bezel: [(f64, &'static str); 3],
    scope: (&'static str, &'static str),
    print: &'static str,
}

const LIGHT: Finish = Finish {
    look: Look {
        ink: "#1f2326",
        dim: "#5d6469",
        accent: "#6a3fc4",
        scrim: "#d5d9d6e6",
        knob: [
            "color=#24272b",
            "pointer=#f1f1f1",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#9aa1a6",
        ],
        wheel: [
            "color=#2a2d31",
            "pointer=#c4b5fd",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#b8bec2",
        ],
    },
    console: [(0.0, "#e0e3e0"), (0.6, "#d0d4d1"), (1.0, "#bcc1be")],
    rivet: "#8b9295",
    bezel: [(0.0, "#f2f4f5"), (0.5, "#8d9398"), (1.0, "#e1e4e6")],
    scope: ("#100c18", "#06040a"),
    print: "#8e86a6",
};

const DARK: Finish = Finish {
    look: Look {
        ink: "#e4e7ea",
        dim: "#8c949b",
        accent: kit::PURPLE_LAMP,
        scrim: "#25292ee6",
        knob: [
            "color=#1a1c1f",
            "pointer=#e8e8e8",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#4b5157",
        ],
        wheel: [
            "color=#16181b",
            "pointer=#c4b5fd",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#5b6168",
        ],
    },
    console: [(0.0, "#363b41"), (0.6, "#2a2e33"), (1.0, "#1e2125")],
    rivet: "#121417",
    bezel: [(0.0, "#c9ced2"), (0.5, "#5b6167"), (1.0, "#aab0b5")],
    scope: ("#0e0a16", "#050308"),
    print: "#7e7696",
};

fn finish(dark: bool) -> &'static Finish {
    if dark { &DARK } else { &LIGHT }
}

fn look(dark: bool) -> &'static Look {
    &finish(dark).look
}

/// The reflections: (bearing°, range 0..1 of the scope, strength).
fn taps() -> Vec<(f64, f64, f64)> {
    (0..14)
        .map(|i| {
            let f = f64::from(i);
            let j = |k: f64| crate::reverb::jitter(f * 3.7 + k);
            // Earlier reflections are stronger.
            let range = 0.18 + 0.78 * (f + 0.5 * j(1.0)) / 14.0;
            (
                360.0 * j(2.0),
                range.min(0.96),
                (1.0 - 0.55 * range) * (0.7 + 0.3 * j(3.0)),
            )
        })
        .collect()
}

/// How far out the blips stand: the room's size.
fn spread() -> String {
    format!("lerp(0.45, 1, norm({VERB}/size))")
}

/// Where the range gate is (0..1 of the trace): the decay.
fn gate() -> String {
    format!("lerp(0.2, 1, norm({VERB}/decay))")
}

/// The sweep's turns per second.
const SWEEP: f64 = 0.4;

/// The console: grey enamel with rivets at its corners.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), dark: bool) -> NodeId {
    let f = finish(dark);
    let panel = shell::panel(b, root, (w, h), &f.console, 6.0);
    for (x, y) in [
        (5.0, 5.0),
        (w - 5.0, 5.0),
        (5.0, h - 5.0),
        (w - 5.0, h - 5.0),
    ] {
        b.add(
            panel,
            draw::with_effects(
                draw::circle(
                    "rivet",
                    x,
                    y,
                    2.0,
                    draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff60"), (1.0, f.rivet)]),
                ),
                vec![draw::drop_shadow("#00000060", 0.0, 0.8, 1.0)],
            ),
        );
    }
    panel
}

/// The PPI at (cx, cy), radius `r`, in `well`: its steel bezel, range
/// rings and bearing ticks, the source, the blips and the sweep.
fn ppi(b: &mut Builder<'_>, well: NodeId, (cx, cy, r): (f64, f64, f64), f: &Finish) {
    let bezel = b.add(
        well,
        draw::with_effects(
            draw::circle("bezel", cx, cy, r + 6.0, draw::linear(135.0, &f.bezel)),
            vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 4.0)],
        ),
    );
    kit::steel(b, bezel);
    b.add(
        well,
        draw::with_effects(
            draw::circle(
                "scope",
                cx,
                cy,
                r,
                draw::radial_at(0.5, 0.5, 0.5, &[(0.0, f.scope.0), (1.0, f.scope.1)]),
            ),
            vec![draw::inner_shadow("#000000e0", 0.0, 1.5, 5.0)],
        ),
    );
    let mut face = draw::frame("PPI", cx - r, cy - r, 2.0 * r, 2.0 * r);
    face.clips_content = true;
    face.corner_radii = document_proto::CornerRadii::uniform(r);
    let face = b.add(well, face);
    // Range rings and bearings, printed on the tube.
    let rings: Vec<Vec<(f64, f64)>> = (1..=4)
        .map(|k| {
            (0..=48)
                .map(|i| draw::polar(r, r, r * f64::from(k) / 4.0, 7.5 * f64::from(i)))
                .collect()
        })
        .collect();
    b.add(
        face,
        ui::strokes(
            "range rings",
            &rings,
            0.6,
            draw::solid(&ui::alpha(PHOSPHOR, 0.28)),
        ),
    );
    let ticks: Vec<Vec<(f64, f64)>> = (0..36)
        .map(|i| {
            let a = 10.0 * f64::from(i);
            let inner = if i % 3 == 0 { r * 0.86 } else { r * 0.93 };
            vec![draw::polar(r, r, inner, a), draw::polar(r, r, r, a)]
        })
        .collect();
    b.add(
        face,
        ui::strokes(
            "bearings",
            &ticks,
            0.6,
            draw::solid(&ui::alpha(PHOSPHOR, 0.4)),
        ),
    );
    b.add(
        face,
        ui::strokes(
            "cross",
            &[
                vec![(r, 4.0), (r, 2.0 * r - 4.0)],
                vec![(4.0, r), (2.0 * r - 4.0, r)],
            ],
            0.5,
            draw::solid(&ui::alpha(PHOSPHOR, 0.18)),
        ),
    );
    for k in 1..=3 {
        ui::silk_left(
            b,
            face,
            &format!("{}", 20 * k),
            r + 2.0,
            r - r * f64::from(k) / 4.0 + 4.0,
            4.5,
            &ui::alpha(PHOSPHOR, 0.55),
            700.0,
        );
    }
    // The sweep: a bright edge and its fading wake, round and round.
    let sw = b.add(face, draw::frame("sweep", 0.0, 0.0, 2.0 * r, 2.0 * r));
    b.add(
        sw,
        draw::circle(
            "wake",
            r,
            r,
            r,
            draw::conic(
                0.0,
                &[
                    (0.0, &ui::alpha(PHOSPHOR, 0.0)),
                    (0.84, &ui::alpha(PHOSPHOR, 0.0)),
                    (1.0, &ui::alpha(PHOSPHOR, 0.42)),
                ],
            ),
        ),
    );
    b.add(
        sw,
        ui::glowing(
            ui::line_node("edge", &[(r, r), (r, 1.0)], 1.2, HOT),
            PHOSPHOR,
            1.0,
            kit::GLOW,
        ),
    );
    motion::add(
        b,
        sw,
        Motion::new(Target::Rotate)
            .input(format!("fract(time * {SWEEP})"))
            .range(0.0, 360.0)
            .pivot(0.5, 0.5),
    );
    motion::add(
        b,
        sw,
        Motion::new(Target::Opacity)
            .input(format!("param({VERB}/on)"))
            .range(0.25, 1.0),
    );
    // The blips: out from the centre with the size; each flares as the
    // sweep passes and glows on for as long as the decay.
    for (i, (bearing, range, strength)) in taps().iter().enumerate() {
        let (dx, dy) = draw::polar(0.0, 0.0, range * (r - 4.0), *bearing);
        let blip = b.add(
            face,
            draw::frame(&format!("blip {i}"), r - 4.0, r - 4.0, 8.0, 8.0),
        );
        b.add(
            blip,
            ui::glowing(
                draw::circle("echo", 4.0, 4.0, 2.0 + 1.8 * strength, draw::solid(HOT)),
                PHOSPHOR,
                1.2,
                kit::GLOW,
            ),
        );
        let out = spread();
        motion::add(
            b,
            blip,
            Motion::new(Target::X)
                .input(out.clone())
                .range(0.0, dx)
                .dynamics(Dynamics::spring(180.0, 24.0)),
        );
        motion::add(
            b,
            blip,
            Motion::new(Target::Y)
                .input(out)
                .range(0.0, dy)
                .dynamics(Dynamics::spring(180.0, 24.0)),
        );
        let since = format!("fract(time * {SWEEP} - {:.4})", bearing / 360.0);
        let glow = format!(
            "pow(1 - {since}, lerp(6, 0.7, norm({VERB}/decay))) * {strength:.3} * param({VERB}/on)"
        );
        motion::add(
            b,
            blip,
            Motion::new(Target::Opacity).input(glow).range(0.18, 1.0),
        );
    }
    // The source, at the centre.
    b.add(
        face,
        draw::emissive(
            draw::circle("source", r, r, 2.6, draw::solid(HOT)),
            draw::hex(PHOSPHOR),
            1.2,
            kit::BLOOM,
        ),
    );
}

/// The A-scope in (x, y, w, h) of `well`: the range trace — the grass, the
/// direct sound, each reflection's spike at its range (further out with the
/// size), the gate and what lies past it dimmed.
fn a_scope(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64), f: &Finish) {
    let mut g = draw::frame("A-scope", x, y, w, h);
    g.clips_content = true;
    let g = b.add(well, g);
    let base = h - 16.0;
    let top = 12.0;
    let tall = base - top;
    ui::silk_left(b, g, "A-scope · range", 0.0, 4.0, 5.5, f.print, 800.0);
    // The graticule.
    let grid: Vec<Vec<(f64, f64)>> = (0..=8)
        .map(|k| {
            let gx = w * f64::from(k) / 8.0;
            vec![(gx, top), (gx, base)]
        })
        .chain((0..=3).map(|k| {
            let gy = top + tall * f64::from(k) / 3.0;
            vec![(0.0, gy), (w, gy)]
        }))
        .collect();
    b.add(
        g,
        ui::strokes(
            "graticule",
            &grid,
            0.5,
            draw::solid(&ui::alpha(PHOSPHOR, 0.14)),
        ),
    );
    ui::silk_left(b, g, "0", 0.0, base + 8.0, 5.0, f.print, 700.0);
    ui::silk_right(b, g, "80 ms", w, base + 8.0, 5.0, f.print, 700.0);
    // The grass: the noise floor along the trace.
    let grass: Vec<(f64, f64)> = (0..=160)
        .map(|i| {
            let t = f64::from(i) / 160.0;
            (
                t * w,
                base - 1.5 - 3.0 * crate::reverb::jitter(f64::from(i) * 1.31),
            )
        })
        .collect();
    b.add(
        g,
        ui::glowing(
            ui::strokes(
                "grass",
                &[grass],
                0.8,
                draw::solid(&ui::alpha(PHOSPHOR, 0.8)),
            ),
            PHOSPHOR,
            0.5,
            kit::GLOW,
        ),
    );
    // The direct sound.
    let direct = format!("M 0 {tall:.1} L 2 0 L 4 {tall:.1} Z");
    b.add(
        g,
        ui::glowing(
            draw::path("direct", 0.0, top, 4.0, tall, direct, draw::solid(HOT)),
            PHOSPHOR,
            1.0,
            kit::GLOW,
        ),
    );
    let (out, gate) = (spread(), gate());
    for (i, (_, range, strength)) in taps().iter().enumerate() {
        let sh = tall * 0.92 * strength;
        let spike = b.add(
            g,
            draw::frame(&format!("spike {i}"), -2.0, base - sh, 4.0, sh),
        );
        let d = format!("M 0 {sh:.1} L 2 0 L 4 {sh:.1} Z");
        b.add(
            spike,
            ui::glowing(
                draw::path(
                    "echo",
                    0.0,
                    0.0,
                    4.0,
                    sh,
                    d,
                    draw::linear(180.0, &[(0.0, HOT), (1.0, PHOSPHOR)]),
                ),
                PHOSPHOR,
                0.9,
                kit::GLOW,
            ),
        );
        let at = format!("{range:.4} * {out}");
        motion::add(
            b,
            spike,
            Motion::new(Target::X)
                .input(at.clone())
                .range(0.0, w)
                .dynamics(Dynamics::spring(180.0, 24.0)),
        );
        // Past the gate, it fades.
        let kept = format!(
            "(0.12 + 0.88 * (1 - smoothstep({gate} - 0.03, {gate} + 0.03, {at}))) * (0.35 + 0.65 * param({VERB}/on))"
        );
        motion::add(
            b,
            spike,
            Motion::new(Target::Opacity).input(kept).range(0.0, 1.0),
        );
    }
    // What lies past the gate, dimmed.
    // (Pinned at the right, stretched back to the gate: it never leaves
    // the trace.)
    let past = b.add(g, draw::frame("past the gate", 0.0, top, w, tall));
    b.add(
        past,
        draw::rect(
            "shade",
            0.0,
            0.0,
            w,
            tall,
            draw::linear(
                90.0,
                &[
                    (0.0, &ui::alpha(f.scope.1, 0.55)),
                    (1.0, &ui::alpha(f.scope.1, 0.8)),
                ],
            ),
        ),
    );
    motion::add(
        b,
        past,
        Motion::new(Target::StretchX)
            .input(format!("1 - {gate}"))
            .range(0.0, 1.0)
            .pivot(1.0, 0.5),
    );
    // The gate: its line, its tab on the baseline — drag it.
    let hh = h - top;
    shell::decay_handle(b, g, "Gate", (w * 0.2, w, top), (26.0, hh), |b, t| {
        b.add(
            t,
            ui::strokes(
                "gate line",
                &[vec![(13.0, 0.0), (13.0, hh - 14.0)]],
                0.9,
                draw::solid(&ui::alpha(HOT, 0.75)),
            ),
        );
        b.add(
            t,
            ui::glowing(
                draw::rounded(
                    draw::rect("tab", 0.0, hh - 13.0, 26.0, 10.0, draw::solid(PHOSPHOR)),
                    2.5,
                ),
                PHOSPHOR,
                0.6,
                kit::GLOW,
            ),
        );
        ui::plain(b, t, "Gate", 13.0, hh - 8.0, 5.5, "#1a0f2c", 800.0);
    });
}

/// The tall face's spare room under its nameplate: the set's readouts —
/// the range, the pre-delay and the persistence — in segments.
fn readouts(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64), f: &Finish) {
    let g = b.add(well, draw::frame("Readouts", x, y, w, h));
    let cells: [(&str, &str, usize, u8); 3] = [
        ("size", "Range", 3, 2),
        ("predelay", "Pre ms", 3, 0),
        ("decay", "Persist", 3, 2),
    ];
    let cw = w / cells.len() as f64;
    for (i, (id, label, digits, dec)) in cells.iter().enumerate() {
        let cx = cw * i as f64;
        b.add(
            g,
            draw::with_effects(
                draw::rounded(
                    draw::rect(
                        "window",
                        cx + 6.0,
                        6.0,
                        cw - 12.0,
                        h - 22.0,
                        draw::solid("#07050c"),
                    ),
                    3.0,
                ),
                vec![draw::inner_shadow("#000000e0", 0.0, 1.0, 3.0)],
            ),
        );
        kit::seg_readout(
            b,
            g,
            &format!("{VERB}/{id}"),
            (cx + 12.0, 6.0 + (h - 22.0) / 2.0, cw - 24.0),
            (h - 22.0) * 0.5,
            *digits,
            *dec,
            PHOSPHOR,
        );
        ui::silk(b, g, label, cx + cw / 2.0, h - 7.0, 5.5, f.print, 800.0);
    }
}

/// The picture: the PPI, and beside it (where there is room) the A-scope,
/// on the set's dark face under glass.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    room: Room,
    dark: bool,
) -> Vec<NodeId> {
    let f = finish(dark);
    let well = shell::well(
        b,
        panel,
        "Picture",
        (x, y, w, h),
        &[(0.0, "#16141c"), (1.0, "#0c0b10")],
        6.0,
    );
    let r = h / 2.0 - 10.0;
    match room {
        Room::Phone => ppi(b, well, (w / 2.0, h / 2.0, r), f),
        Room::Lane => {
            ppi(b, well, (r + 14.0, h / 2.0, r), f);
            a_scope(
                b,
                well,
                (2.0 * r + 34.0, 8.0, w - 2.0 * r - 48.0, h - 12.0),
                f,
            );
        }
        Room::Tall => {
            ppi(b, well, (r + 14.0, h / 2.0, r), f);
            a_scope(
                b,
                well,
                (2.0 * r + 34.0, 6.0, 400.0 - 2.0 * r - 34.0, h - 10.0),
                f,
            );
            readouts(b, well, (416.0, 46.0, w - 424.0, h - 48.0), f);
        }
    }
    kit::glass(b, well);
    vec![well]
}

/// The nameplate's title: "Reflections" in an instrument maker's wide
/// face.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, dark: bool) {
    let l = draw::Lettering::family(draw::family::MICHROMA).weight(400.0);
    crate::suite_studio_blues::parts::set(
        b,
        t,
        "Reflections",
        56.0,
        h / 2.0,
        size * 0.62,
        finish(dark).look.ink,
        &l,
        0.02,
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the scopes beside the controls; on a phone, the PPI
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
