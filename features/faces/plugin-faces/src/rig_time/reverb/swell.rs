//! Swell: the reverb that fades in instead of striking — drawn as the
//! machine that has always done that, an organ's swell box. Behind the
//! box's shutters a flat of pipes, lit from within; the shutters open as the
//! wet swells and close as it dies away, again and again, and the light
//! pours out between them. Beside the box, the envelope itself: the dry
//! note's quick spike, then the swell — rising over the Rise (size), dying
//! over the decay — and a bead of light riding it in step with the
//! shutters. The tail's end is a tab to drag: the decay.
//!
//! Two finishes: light, pale oak with ivory knobs; dark, walnut.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use super::shell::{self, Look, Room, Unit};
use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const UNIT: Unit = Unit {
    name: "Swell",
    base: "90-verb-swell",
    title_w: 84.0,
    legend: "On",
    lane: [
        ("size", "Rise"),
        ("predelay", "Pre-Delay"),
        ("level", "Level"),
    ],
    tall: [
        ("size", "Rise"),
        ("predelay", "Pre-Delay"),
        ("damping", "HF Damp"),
        ("level", "Level"),
    ],
    look,
    ground,
    picture,
    title,
};

/// The light behind the shutters: the section's purple.
const LIGHT_IN: &str = kit::PURPLE_LAMP;

/// One finish: its inks and knobs, and its wood.
struct Finish {
    look: Look,
    wood: [(f64, &'static str); 3],
    grain: &'static str,
    case: (&'static str, &'static str),
    slat: [(f64, &'static str); 3],
    chamber: [(f64, &'static str); 2],
    graph: [(f64, &'static str); 2],
    rule: &'static str,
}

const LIGHT: Finish = Finish {
    look: Look {
        ink: "#2e2216",
        dim: "#7a6750",
        accent: "#6a3fc4",
        scrim: "#eadcc4e6",
        knob: [
            "color=#f3ead8",
            "pointer=#3b2814",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#9c7a52",
        ],
        wheel: [
            "color=#f6eedd",
            "pointer=#6a3fc4",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#7a5a36",
        ],
    },
    wood: [(0.0, "#efe1c6"), (0.6, "#e2cfa9"), (1.0, "#d2ba8e")],
    grain: "#7a552a18",
    case: ("#9a6c3c", "#5e3e1e"),
    slat: [(0.0, "#6b4524"), (0.5, "#b5834f"), (1.0, "#5a391b")],
    chamber: [(0.0, "#160f1f"), (1.0, "#0b0810")],
    graph: [(0.0, "#1a1622"), (1.0, "#100d16")],
    rule: "#8d8299",
};

const DARK: Finish = Finish {
    look: Look {
        ink: "#efe3cf",
        dim: "#a8957a",
        accent: kit::PURPLE_LAMP,
        scrim: "#2a1d14e6",
        knob: [
            "color=#2b1d14",
            "pointer=#e9d9b8",
            "index=line",
            "index_w=0.12",
            "skirt=plain",
            "skirt_color=#120b06",
        ],
        wheel: [
            "color=#efe3cf",
            "pointer=#3b1d6e",
            "index=line",
            "index_w=0.1",
            "skirt=plain",
            "skirt_color=#3a2716",
        ],
    },
    wood: [(0.0, "#3a281b"), (0.6, "#2c1e14"), (1.0, "#1e140d")],
    grain: "#ffffff08",
    case: ("#5a3b22", "#2a1a0e"),
    slat: [(0.0, "#2e1d10"), (0.5, "#7a5230"), (1.0, "#26170b")],
    chamber: [(0.0, "#120c1a"), (1.0, "#07050b")],
    graph: [(0.0, "#15121c"), (1.0, "#0c0a11")],
    rule: "#7a7088",
};

fn finish(dark: bool) -> &'static Finish {
    if dark { &DARK } else { &LIGHT }
}

fn look(dark: bool) -> &'static Look {
    &finish(dark).look
}

/// The swell's phase: one cycle every 3⅓ s.
const PHASE: &str = "fract(time * 0.3)";

/// Where the rise ends (0..1 of the cycle): later with the Rise (size).
fn rise() -> String {
    format!("lerp(0.06, 0.3, norm({VERB}/size))")
}

/// Where the tail ends: later with the decay.
fn tail() -> String {
    format!("lerp(0.4, 1.0, norm({VERB}/decay))")
}

/// The envelope at phase `p` (an expression): up over the rise, down over
/// the tail.
fn env(p: &str) -> String {
    let (r, t) = (rise(), tail());
    format!("smoothstep(0, {r}, {p}) * (1 - smoothstep({r}, {t}, {p}))")
}

/// The panel: oak (or walnut) with its grain running across.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), dark: bool) -> NodeId {
    let f = finish(dark);
    let panel = shell::panel(b, root, (w, h), &f.wood, 8.0);
    let mut grain = Vec::new();
    let mut i = 0.0;
    while i * 2.6 < h {
        let y = 1.0 + i * 2.6;
        let wave = |x: f64| y + 1.6 * ((x / w) * 6.0 + i * 0.7).sin() * crate::reverb::jitter(i);
        grain.push(
            (0..=12)
                .map(|k| f64::from(k) * w / 12.0)
                .map(|x| (x, wave(x)))
                .collect::<Vec<_>>(),
        );
        i += 1.0;
    }
    b.add(
        panel,
        ui::strokes("grain", &grain, 0.7, draw::solid(f.grain)),
    );
    panel
}

/// The swell box in (x, y, w, h) of the well: the chamber, its pipes lit
/// from behind, the shutters in front in their case.
fn swell_box(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64), f: &Finish) {
    let mut chamber = draw::frame("Swell box", x, y, w, h);
    chamber.fills = vec![draw::linear(180.0, &f.chamber)];
    chamber.clips_content = true;
    let bx = b.add(well, chamber);
    let e = format!("{} * param({VERB}/on)", env(PHASE));
    // The light inside: a field, drawn, coming up as the shutters open.
    let glow = b.add(
        bx,
        draw::rect(
            "light",
            0.0,
            0.0,
            w,
            h,
            draw::radial_at(
                0.5,
                0.42,
                0.75,
                &[
                    (0.0, &ui::alpha("#d9c4ff", 0.95)),
                    (0.5, &ui::alpha(LIGHT_IN, 0.6)),
                    (1.0, &ui::alpha(LIGHT_IN, 0.12)),
                ],
            ),
        ),
    );
    motion::add(
        b,
        glow,
        Motion::new(Target::Opacity)
            .input(format!("0.08 + 0.92 * {e}"))
            .range(0.0, 1.0),
    );
    // The lamp over the pipes, seen between the shutters.
    let lamp = b.add(
        bx,
        draw::emissive(
            draw::rounded(
                draw::rect("lamp", w * 0.1, 3.0, w * 0.8, 4.0, draw::solid("#e9dcff")),
                2.0,
            ),
            draw::hex(LIGHT_IN),
            1.4,
            kit::BLOOM,
        ),
    );
    motion::add(
        b,
        lamp,
        Motion::new(Target::Opacity)
            .input(format!("0.15 + 0.85 * {e}"))
            .range(0.0, 1.0),
    );
    // The flat of pipes: tallest at the centre, each with its mouth and
    // its foot.
    let n = 9;
    let pw = (w - 20.0) / f64::from(n);
    for i in 0..n {
        let k = (f64::from(i) - f64::from(n - 1) / 2.0).abs() / (f64::from(n - 1) / 2.0);
        let ph = h * (0.82 - 0.34 * k * k);
        let px = 10.0 + f64::from(i) * pw + pw * 0.22;
        let bw = pw * 0.56;
        let foot = h * 0.16;
        let top = h - foot - ph * 0.8;
        let metal = draw::linear(
            90.0,
            &[
                (0.0, "#3c3946"),
                (0.35, "#c9c3d6"),
                (0.55, "#f1edf7"),
                (1.0, "#4a4656"),
            ],
        );
        b.add(bx, draw::rect("pipe", px, top, bw, h - foot - top, metal));
        let cone = format!(
            "M 0 0 L {bw:.1} 0 L {:.1} {foot:.1} L {:.1} {foot:.1} Z",
            bw * 0.62,
            bw * 0.38
        );
        b.add(
            bx,
            draw::path(
                "foot",
                px,
                h - foot,
                bw,
                foot,
                cone,
                draw::linear(
                    90.0,
                    &[(0.0, "#2c2a33"), (0.5, "#9b95a8"), (1.0, "#2c2a33")],
                ),
            ),
        );
        let my = h - foot - ph * 0.8 * 0.22;
        let mouth = format!(
            "M 0 0 L {bw:.1} 0 L {:.1} 5 L {:.1} 5 Z",
            bw * 0.8,
            bw * 0.2
        );
        b.add(
            bx,
            draw::path("mouth", px, my, bw, 5.0, mouth, draw::solid("#141118")),
        );
    }
    // The shutters: upright slats that turn edge-on as the swell opens,
    // and between them the light, seen only as they part.
    let slats = ((w / 16.0).round() as i32).max(6);
    let sw = w / f64::from(slats);
    for i in 1..slats {
        let shaft = draw::rect(
            "shaft",
            f64::from(i) * sw - 1.5,
            0.0,
            3.0,
            h,
            draw::linear(
                180.0,
                &[(0.0, "#f3ecff"), (0.5, "#d9c8ff"), (1.0, "#f3ecff")],
            ),
        );
        let shaft = b.add(bx, ui::glowing(shaft, LIGHT_IN, 1.0, kit::GLOW));
        motion::add(
            b,
            shaft,
            Motion::new(Target::Opacity)
                .input(e.clone())
                .range(0.0, 1.0),
        );
    }
    for i in 0..slats {
        let s = b.add(
            bx,
            draw::frame("shutter", f64::from(i) * sw - 0.5, 0.0, sw + 1.0, h),
        );
        let slat = draw::rect("slat", 0.0, 0.0, sw + 1.0, h, draw::linear(90.0, &f.slat));
        b.add(
            s,
            draw::with_effects(slat, vec![draw::drop_shadow("#00000080", 1.0, 0.0, 2.0)]),
        );
        motion::add(
            b,
            s,
            Motion::new(Target::StretchX)
                .input(e.clone())
                .range(1.0, 0.12)
                .pivot(0.5, 0.5),
        );
    }
    // The case: a frame round the shutters, its top rail lettered.
    let case = draw::rect(
        "case",
        x - 3.0,
        y - 3.0,
        w + 6.0,
        h + 6.0,
        draw::solid("#00000000"),
    );
    let mut case = draw::stroked(draw::rounded(case, 3.0), f.case.0, 4.0);
    case.fills.clear();
    b.add(
        well,
        draw::with_effects(case, vec![draw::drop_shadow("#00000090", 0.0, 1.0, 3.0)]),
    );
    let rail = b.add(
        well,
        draw::rounded(
            draw::rect(
                "rail",
                x + w / 2.0 - 26.0,
                y - 4.0,
                52.0,
                10.0,
                draw::linear(180.0, &[(0.0, f.case.0), (1.0, f.case.1)]),
            ),
            2.0,
        ),
    );
    ui::plain(b, rail, "SWELL", 26.0, 5.0, 5.5, "#e9d6a8", 800.0);
}

/// The envelope in (x, y, w, h) of the well: its bars (the swell over one
/// cycle, shaped by the Rise and the decay), the dry spike, the bead riding
/// it, the Rise mark and the tail's tab.
fn envelope(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64), f: &Finish) {
    let g = b.add(well, draw::frame("Envelope", x, y, w, h));
    let base = h - 20.0;
    let top = 12.0;
    let gh = base - top;
    ui::silk_right(b, g, "Swell · one cycle", w, 4.0, 5.5, f.rule, 800.0);
    ui::line(
        b,
        g,
        "axis",
        &[(0.0, base), (w, base)],
        0.8,
        &ui::alpha(f.rule, 0.8),
    );
    // The bars: the envelope at each step, from the params alone.
    let n = 40;
    let bw = w / f64::from(n);
    for i in 0..n {
        let p = (f64::from(i) + 0.5) / f64::from(n);
        let bar = b.add(
            g,
            draw::frame("step", f64::from(i) * bw + bw * 0.18, top, bw * 0.64, gh),
        );
        b.add(
            bar,
            draw::rect(
                "swell",
                0.0,
                0.0,
                bw * 0.64,
                gh,
                draw::linear(
                    180.0,
                    &[
                        (0.0, &ui::alpha(LIGHT_IN, 0.95)),
                        (1.0, &ui::alpha(LIGHT_IN, 0.25)),
                    ],
                ),
            ),
        );
        motion::add(
            b,
            bar,
            Motion::new(Target::StretchY)
                .input(env(&format!("{p:.4}")))
                .range(0.02, 1.0)
                .pivot(0.5, 1.0)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
    }
    // The dry note: struck, gone.
    let spike: Vec<(f64, f64)> = (0..=10)
        .map(|k| {
            let t = f64::from(k) / 10.0;
            (1.0 + t * w * 0.05, base - gh * 0.95 * (1.0 - t).powi(3))
        })
        .collect();
    ui::line(
        b,
        g,
        "dry",
        &[(1.0, base), (1.0, base - gh * 0.95)],
        1.4,
        "#f2eef8",
    );
    ui::line(b, g, "dry tail", &spike, 1.0, &ui::alpha("#f2eef8", 0.7));
    // The Rise mark: where the swell peaks.
    let mark = b.add(g, draw::frame("rise mark", 0.0, top - 2.0, 1.0, gh + 2.0));
    ui::line(
        b,
        mark,
        "rise",
        &[(0.0, 0.0), (0.0, gh + 2.0)],
        0.8,
        &ui::alpha(f.rule, 0.9),
    );
    ui::silk(b, mark, "Rise", 0.0, gh + 9.0, 5.0, f.rule, 800.0);
    motion::add(
        b,
        mark,
        Motion::new(Target::X)
            .input(rise())
            .range(0.0, w)
            .dynamics(Dynamics::spring(200.0, 26.0)),
    );
    // The cursor and the bead: where the swell is now.
    let cur = b.add(g, draw::frame("cursor", -4.0, top, 8.0, gh));
    ui::line(
        b,
        cur,
        "now",
        &[(4.0, 0.0), (4.0, gh)],
        0.7,
        &ui::alpha("#f2eef8", 0.35),
    );
    motion::add(b, cur, Motion::new(Target::X).input(PHASE).range(0.0, w));
    let bead = b.add(
        cur,
        ui::glowing(
            draw::circle("bead", 4.0, gh, 3.2, draw::solid("#f6f1ff")),
            LIGHT_IN,
            1.4,
            kit::BLOOM,
        ),
    );
    motion::add(
        b,
        bead,
        Motion::new(Target::Y)
            .input(format!("{} * param({VERB}/on)", env(PHASE)))
            .range(0.0, -gh),
    );
    // The tail's end, a tab under the axis: drag it for the decay.
    shell::decay_handle(
        b,
        g,
        "Tail",
        (w * 0.4, w, base + 2.0),
        (26.0, 14.0),
        |b, t| {
            b.add(
                t,
                draw::path(
                    "point",
                    9.0,
                    0.0,
                    8.0,
                    4.0,
                    "M 4 0 L 8 4 L 0 4 Z".into(),
                    draw::solid(LIGHT_IN),
                ),
            );
            b.add(
                t,
                ui::glowing(
                    draw::rounded(
                        draw::rect("tab", 0.0, 4.0, 26.0, 10.0, draw::solid(LIGHT_IN)),
                        2.5,
                    ),
                    LIGHT_IN,
                    0.6,
                    kit::GLOW,
                ),
            );
            ui::plain(b, t, "Tail", 13.0, 9.0, 5.5, "#1a0f2c", 800.0);
        },
    );
}

/// The tall face's spare room under its nameplate: the console's
/// drawstops, ivory faces lettered in their stems' names — the Swell's own
/// drawn out and lit while it plays.
fn stops(b: &mut Builder<'_>, well: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let g = b.add(well, draw::frame("Stops", x, y, w, h));
    let names = [
        ("Diapason", "8'"),
        ("Gamba", "8'"),
        ("Celeste", "8'"),
        ("Flute", "4'"),
        ("Swell", "Tremulant"),
    ];
    let n = names.len() as f64;
    let r = (h / 2.0 - 4.0).min((w / n) / 2.0 - 6.0);
    let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(700.0);
    for (i, (name, pitch)) in names.iter().enumerate() {
        let cx = w * (i as f64 + 0.5) / n;
        let cy = h / 2.0;
        let on = i + 1 == names.len();
        // The stem's collar, then the face.
        b.add(
            g,
            draw::with_effects(
                draw::circle(
                    "collar",
                    cx,
                    cy,
                    r + 3.0,
                    draw::linear(180.0, &[(0.0, "#3a2414"), (1.0, "#140b05")]),
                ),
                vec![draw::drop_shadow("#000000a0", 0.0, 2.0, 4.0)],
            ),
        );
        if on {
            let ring = draw::stroked(
                draw::circle("lit ring", cx, cy, r + 2.0, draw::solid("#00000000")),
                LIGHT_IN,
                2.0,
            );
            let ring = b.add(g, ui::glowing(ring, LIGHT_IN, 1.0, kit::GLOW));
            motion::add(
                b,
                ring,
                Motion::new(Target::Opacity)
                    .input(format!("param({VERB}/on)"))
                    .range(0.15, 1.0),
            );
        }
        b.add(
            g,
            draw::with_effects(
                draw::circle(
                    "face",
                    cx,
                    cy,
                    r,
                    draw::radial_at(
                        0.4,
                        0.35,
                        0.85,
                        &[(0.0, "#fffaf0"), (0.7, "#efe4cc"), (1.0, "#cdbf9f")],
                    ),
                ),
                vec![draw::inner_shadow("#00000040", 0.0, -1.5, 2.0)],
            ),
        );
        crate::suite_studio_blues::parts::set(
            b,
            g,
            name,
            cx,
            cy - r * 0.18,
            (r * 0.3).min(8.0),
            if on { "#4c2a85" } else { "#2a1a0c" },
            &serif,
            0.0,
        );
        ui::silk(
            b,
            g,
            pitch,
            cx,
            cy + r * 0.32,
            (r * 0.2).min(5.5),
            "#6e5428",
            700.0,
        );
    }
}

/// The picture: the box, and beside it (where there is room) the
/// envelope, under glass.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    room: Room,
    dark: bool,
) -> Vec<NodeId> {
    let f = finish(dark);
    let well = shell::well(b, panel, "Picture", (x, y, w, h), &f.graph, 6.0);
    let bw = if room == Room::Phone {
        w - 20.0
    } else {
        (h * 1.55).min(w * 0.42)
    };
    swell_box(b, well, (10.0, 12.0, bw, h - 22.0), f);
    match room {
        Room::Phone => {}
        Room::Lane => envelope(b, well, (bw + 30.0, 8.0, w - bw - 44.0, h - 10.0), f),
        // The tall face's nameplate covers the top right: the envelope
        // keeps left of it, the stops sit under it.
        Room::Tall => {
            envelope(b, well, (bw + 30.0, 4.0, 400.0 - bw - 30.0, h - 6.0), f);
            stops(b, well, (414.0, 44.0, w - 422.0, h - 46.0));
        }
    }
    kit::glass(b, well);
    vec![well]
}

/// The nameplate's title: "Swell" in an organ builder's blackletter.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, dark: bool) {
    let l = draw::Lettering::family(draw::family::UNIFRAKTUR_COOK).weight(700.0);
    crate::suite_studio_blues::parts::set(
        b,
        t,
        "Swell",
        32.0,
        h / 2.0,
        size,
        finish(dark).look.ink,
        &l,
        0.04,
    );
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the box and its envelope beside the controls; on a
/// phone, the box alone.
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
