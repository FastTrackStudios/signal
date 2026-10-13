//! The tape echo, laid out from front-on photographs of the Roland RE-201
//! Space Echo (1974): a multi-head machine — one record head and three
//! playback heads on a free-running tape loop — which is what this face's
//! head switches are. (The Echoplex, the other candidate, has a single
//! sliding playback head.)
//!
//! Read from the photo, as fractions of the front: a black textured tolex
//! case round a brushed aluminium name strip across the top and a
//! near-black control plate; on the plate, left to right, the input section
//! (a red peak lamp, the small dark VU, three knobs under it), the green
//! centre field where the unit has its head selector, then two fields of
//! three knobs each — the upper one dark, the lower one green — and the
//! power lamp and toggle at the far right; a silver jack strip along the
//! bottom. The knobs are chrome-skirted with aluminium caps and a black
//! line, eleven ticks round them and no numbers, legends over them.
//!
//! What goes where: the lower green field is the unit's own REPEAT RATE /
//! INTENSITY / ECHO VOLUME (time, feedback, mix — "straight" to "echo"
//! printed at its ends); the input knobs are the record level and the
//! tape's wow and flutter; the upper field holds tone and tape age, and the
//! time window (ms, and the note it's locked to) in the space the unit
//! leaves blank; the centre field shows the tape loop past the record head
//! and the three playback heads, each with a lamp that is its switch; the
//! power toggle's place takes the Sync / Free toggle. The peak lamp glows
//! with the record level. No maker or model is printed.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::{Ink as KitInk, Place, TOUCH_MIN, draw};
use frame::surface::proto::{Control, ControlKind, SlotRole};
use frame::surface::widgets::Builder;

use super::art;
use super::parts::{self, Ink, bind};
use crate::face::{Face, canvas};

const W: f64 = 960.0;
const H: f64 = 430.0;
/// The control plate inside the case, and its name strip.
const PLATE: (f64, f64, f64, f64) = (32.0, 55.0, 890.0, 332.0);
const STRIP_H: f64 = 58.0;
/// Silkscreen on the plate: a pale blue-white, bold and condensed.
const INK: &str = "#dce6ec";
const GREEN: &str = "#3d5a39";
const RULE: &str = "#5d8a55";
const LAMP: &str = "#ff4a2e";
const TIME_INK: &str = "#ffb040";
const T_MIN: f64 = 20.0;
const T_MAX: f64 = 2000.0;
/// Knob skirt diameter.
const KNOB: f64 = 60.0;

fn a(s: &str) -> String {
    format!("tape_echo/{s}")
}

fn label() -> Ink<'static> {
    Ink::new(8.5, INK).weight(800.0).track(0.08)
}

/// A chrome-skirted knob with its shaft at (`x`, `y`), legend over it.
fn knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    addr: &str,
    legend: &str,
    (x, y): (f64, f64),
) -> NodeId {
    let p = Place::new(legend)
        .size(KNOB)
        .legend(legend)
        .bind(bind(&a(addr)))
        .ink(KitInk::Color(INK.into()))
        .touch(TOUCH_MIN);
    let k = parts::put(b, panel, "knob.chrome_skirt", &p, x, y);
    parts::caption_style(b, k, label());
    k
}

/// The case: black tolex, pebbled.
fn case(b: &mut Builder<'_>, panel: NodeId) {
    let mut rng: u64 = 0x0201_ec40;
    let mut next = || {
        rng = rng
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (rng >> 33) as f64 / f64::from(u32::MAX >> 1)
    };
    let (mut lit, mut dark) = (String::new(), String::new());
    for _ in 0..1800 {
        let (x, y, r) = (next() * W, next() * H, 0.5 + next() * 1.1);
        let d = art::circle_d(x, y, r, false);
        if next() > 0.5 {
            lit.push_str(&d)
        } else {
            dark.push_str(&d)
        }
    }
    b.add(
        panel,
        draw::path("pebble", 0.0, 0.0, W, H, lit, draw::solid("#ffffff0d")),
    );
    b.add(
        panel,
        draw::path(
            "pebble shade",
            0.0,
            0.0,
            W,
            H,
            dark,
            draw::solid("#0000004d"),
        ),
    );
    // The case's rounded edge catching the light along the top.
    b.add(
        panel,
        draw::rect(
            "edge light",
            0.0,
            0.0,
            W,
            6.0,
            draw::linear(180.0, &[(0.0, "#ffffff21"), (1.0, "#ffffff00")]),
        ),
    );
}

/// A section field on the plate: a flat colour inside a thin green rule.
fn field(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    fill: &str,
) -> NodeId {
    let n = draw::stroked(
        draw::rounded(draw::rect(name, x, y, w, h, draw::solid(fill)), 4.0),
        RULE,
        1.2,
    );
    b.add(parent, n)
}

/// The centre field: the loop past the heads, a lamp switch per playback head.
fn heads(b: &mut Builder<'_>, panel: NodeId) {
    let (x0, y0, w, h) = (336.0, 126.0, 146.0, 188.0);
    field(b, panel, "Head field", (x0, y0, w, h), GREEN);
    parts::silk(b, panel, "Playback Heads", x0 + w / 2.0, y0 + 15.0, label());
    // The loop, printed: round the capstan at the right, back past the tank.
    let (lx0, lx1, ty, by) = (x0 + 14.0, x0 + w - 14.0, y0 + 74.0, y0 + 40.0);
    let loop_pts = vec![
        (lx0 + 10.0, ty),
        (lx1 - 10.0, ty),
        (lx1, ty - 8.0),
        (lx1, by + 8.0),
        (lx1 - 10.0, by),
        (lx0 + 10.0, by),
        (lx0, by + 8.0),
        (lx0, ty - 8.0),
        (lx0 + 10.0, ty),
    ];
    parts::line(b, panel, "tape loop", &loop_pts, 2.4, "#1c1a16");
    // The oxide edge, dashed where the splices and wear show, runs round
    // the loop at the motor's speed (the shorter the REPEAT RATE, the faster
    // the tape) and wanders with WOW — motion.
    let edge = parts::line(b, panel, "tape loop edge", &loop_pts, 0.8, "#8a6a44");
    if let Some(n) = b.scene.node_mut(edge) {
        n.stroke.dash_pattern = vec![7.0, 3.0];
        n.motion = vec![
            frame::surface::kit::motion::spin("(1.2 + 6*(1 - norm(tape_echo/time)))")
                .on(frame::surface::kit::motion::Target::DashOffset)
                .range(0.0, -10.0),
            frame::surface::kit::motion::Motion::new(frame::surface::kit::motion::Target::Y)
                .dynamics(frame::surface::kit::motion::Dynamics::Noise {
                    rate: frame::surface::kit::motion::Num::Value(2.5),
                    amount: frame::surface::kit::motion::Num::Expr(
                        "0.8*norm(tape_echo/wow)".into(),
                    ),
                }),
        ];
    }
    let head_x = [x0 + 30.0, x0 + 60.0, x0 + 88.0, x0 + 116.0];
    for x in head_x {
        art::head(b, panel, (x, ty + 8.0), 14.0, 16.0);
    }
    parts::silk(
        b,
        panel,
        "Rec",
        head_x[0],
        ty + 28.0,
        Ink::new(7.0, INK).weight(700.0).track(0.1),
    );
    art::post(b, panel, (lx1, (ty + by) / 2.0), 4.0);
    let engraved = Ink::new(8.0, INK).weight(800.0).track(0.0);
    for (i, x) in head_x.iter().skip(1).enumerate() {
        let lamp = parts::lamp_toggle(
            b,
            panel,
            &format!("Head {}", i + 1),
            &a(&format!("head_{}", i + 1)),
            (*x, ty + 56.0),
            11.0,
            28.0,
            TIME_INK,
        );
        parts::label_at(b, lamp, &format!("{}", i + 1), (*x, ty + 28.0), engraved);
    }
    parts::silk(
        b,
        panel,
        "Repeat",
        x0 + w / 2.0,
        y0 + h - 16.0,
        Ink::new(7.0, INK).weight(700.0).track(0.2),
    );
}

/// The silver jack strip along the bottom: sockets in outlined boxes.
fn jack_strip(b: &mut Builder<'_>, panel: NodeId) {
    let (x, y, w, h) = (PLATE.0, PLATE.1 + PLATE.3 - 58.0, PLATE.2, 58.0);
    parts::plate(
        b,
        panel,
        "Jack strip",
        (x, y, w, h),
        draw::linear(180.0, &[(0.0, "#d8d9d6"), (1.0, "#b3b4b1")]),
        2.0,
    );
    let boxes: [(f64, f64, &str); 6] = [
        (52.0, 82.0, "Mic"),
        (150.0, 82.0, "Mic"),
        (248.0, 176.0, "Instrument"),
        (440.0, 86.0, "Return"),
        (610.0, 150.0, "Output"),
        (776.0, 128.0, "Foot Sw"),
    ];
    let ink = Ink::new(7.0, "#1c1d1f").weight(800.0).track(0.08);
    for (bx, bw, s) in boxes {
        let r = draw::stroked(
            draw::rounded(
                draw::rect(
                    "jack box",
                    bx,
                    y + 6.0,
                    bw,
                    h - 12.0,
                    draw::solid("#00000000"),
                ),
                3.0,
            ),
            "#3f6a3a",
            1.2,
        );
        b.add(panel, r);
        parts::silk(b, panel, s, bx + bw / 2.0, y + 14.0, ink);
        let (jx, jy) = (bx + 26.0, y + 36.0);
        let ring = draw::circle(
            "jack",
            jx,
            jy,
            13.0,
            draw::radial_at(
                0.38,
                0.32,
                0.85,
                &[(0.0, "#ffffff"), (0.5, "#b9bbbe"), (1.0, "#55575a")],
            ),
        );
        b.add(
            panel,
            draw::with_effects(ring, vec![draw::drop_shadow("#00000080", 0.0, 1.0, 1.5)]),
        );
        b.add(
            panel,
            draw::circle(
                "jack hole",
                jx,
                jy,
                7.0,
                draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#000000"), (1.0, "#2a2b2d")]),
            ),
        );
    }
}

pub fn face() -> Face {
    let scene = canvas("Tape Echo", |b, root| {
        parts::fit_root(b, root, W, H);
        let panel = parts::panel(b, root, "panel.painted", "black", W, H, "none");
        parts::repaint(
            b,
            panel,
            &[(0.0, "#232323"), (0.5, "#1a1a1a"), (1.0, "#121212")],
        );
        case(b, panel);

        // ── the plate and the name strip ──────────────────────────────
        let (px, py, pw, ph) = PLATE;
        let plate = draw::rect(
            "Plate",
            px,
            py,
            pw,
            ph,
            draw::linear(180.0, &[(0.0, "#1f2320"), (1.0, "#171a18")]),
        );
        b.add(
            panel,
            draw::with_effects(
                draw::stroked(plate, "#000000", 1.5),
                vec![draw::inner_shadow("#000000b3", 0.0, 2.0, 5.0)],
            ),
        );
        parts::plate(
            b,
            panel,
            "Name strip",
            (px, py, pw, STRIP_H),
            draw::linear(
                180.0,
                &[
                    (0.0, "#eceeed"),
                    (0.35, "#d2d4d3"),
                    (0.7, "#bfc1c0"),
                    (1.0, "#a9abaa"),
                ],
            ),
            1.0,
        );
        parts::rule(
            b,
            panel,
            px + 6.0,
            px + pw - 6.0,
            py + STRIP_H - 6.0,
            2.0,
            "#1a1b1c",
        );
        parts::silk_left(
            b,
            panel,
            "Tape Echo",
            82.0,
            py + 28.0,
            Ink::new(24.0, "#161718").weight(900.0).track(0.02),
        );
        parts::silk_left(
            b,
            panel,
            "Multi-head",
            300.0,
            py + 30.0,
            Ink::new(12.0, "#161718").weight(800.0).track(0.06),
        );

        // ── input section: peak lamp, VU, three knobs ─────────────────
        let peak = b.add(panel, draw::frame("Peak lamp", 78.0, 141.0, 28.0, 28.0));
        parts::lamp(b, peak, (14.0, 14.0), 12.0, LAMP, false);
        let glow = b.add(peak, parts::lens(14.0, 14.0, 6.0, LAMP));
        let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Glow, glow.0);
        c.binding = Some(parts::stream(&a("in")));
        b.controls.insert(peak, c);
        parts::silk(b, panel, "Peak", 92.0, 177.0, label());
        parts::silk(b, panel, "Level", 92.0, 187.0, label());
        let vu = Place::new("Record level")
            .size(124.0)
            .variant("green")
            .legend("VU")
            .bind(parts::stream(&a("in")));
        let vu = parts::put(b, panel, "meter.vu_backlit", &vu, 190.0, 160.0);
        parts::glass(b, vu);
        for x in [141.0, 239.0] {
            parts::vrule(b, panel, x, 214.0, 318.0, 1.2, RULE);
        }
        knob(b, panel, "drive", "Rec Level", (92.0, 280.0));
        knob(b, panel, "wow", "Wow", (191.0, 280.0));
        knob(b, panel, "flutter", "Flutter", (289.0, 280.0));

        // ── centre: the heads ─────────────────────────────────────────
        heads(b, panel);

        // ── upper field: tone, tape age, the time window ──────────────
        field(
            b,
            panel,
            "Upper field",
            (492.0, 126.0, 408.0, 96.0),
            "#1b1e1c",
        );
        knob(b, panel, "tone", "Tone", (541.0, 187.0));
        knob(b, panel, "age", "Tape Age", (625.0, 187.0));
        let win = parts::well(
            b,
            panel,
            "Time window",
            (690.0, 170.0, 196.0, 34.0),
            draw::linear(180.0, &[(0.0, "#150d06"), (1.0, "#22160b")]),
            4.0,
        );
        parts::readout(
            b,
            win,
            "Repeat time",
            bind(&a("time")),
            (6.0, 17.0, 96.0),
            17.0,
            TIME_INK,
            TextAlign::Right,
        );
        parts::note_picker(
            b,
            win,
            "Note",
            &a("div"),
            (112.0, 17.0, 76.0),
            12.0,
            TIME_INK,
            "#7a5320",
        );
        parts::silk(b, panel, "Repeat Time", 788.0, 150.0, label());

        // ── lower field: repeat rate, intensity, echo volume, sync ────
        field(b, panel, "Lower field", (492.0, 225.0, 408.0, 90.0), GREEN);
        knob(b, panel, "time", "Repeat Rate", (541.0, 280.0));
        knob(b, panel, "feedback", "Intensity", (625.0, 280.0));
        knob(b, panel, "mix", "Echo Volume", (720.0, 280.0));
        let small = Ink::new(6.0, INK).weight(700.0).track(0.08);
        parts::silk(b, panel, "Straight", 694.0, 309.0, small);
        parts::silk(b, panel, "Echo", 750.0, 309.0, small);
        parts::lamp(b, panel, (802.0, 280.0), 10.0, LAMP, true);
        let sync = Place::new("Sync")
            .size(30.0)
            .variant("chrome")
            .legend("Time")
            .options(&["Sync", "Free"])
            .bind(bind(&a("sync")))
            .ink(KitInk::Color(INK.into()));
        let sw = parts::put(b, panel, "switch.toggle_bat", &sync, 866.0, 278.0);
        parts::caption_style(b, sw, label());

        jack_strip(b, panel);
    });

    let params = vec![
        parts::time_ms(&a("time"), "Repeat Rate", T_MIN, T_MAX, 375.0).featured(),
        parts::toggle(&a("sync"), "Sync", false),
        parts::choice(&a("div"), "Note", parts::DIVISIONS, parts::DEFAULT_DIV),
        parts::feedback(&a("feedback"), "Intensity", 35.0).featured(),
        parts::percent(&a("drive"), "Rec Level", 35.0).featured(),
        parts::percent(&a("wow"), "Wow", 20.0),
        parts::percent(&a("flutter"), "Flutter", 15.0),
        parts::percent(&a("tone"), "Tone", 50.0),
        parts::percent(&a("age"), "Tape Age", 50.0),
        parts::percent(&a("mix"), "Echo Volume", 35.0).featured(),
        parts::toggle(&a("head_1"), "Head 1", true),
        parts::toggle(&a("head_2"), "Head 2", false),
        parts::toggle(&a("head_3"), "Head 3", false),
    ];
    let streams = vec![parts::level_stream(&a("in"), "Record Level", 1)];
    Face {
        file: "02-tape",
        plugin: "delay",
        scene,
        params,
        streams,
    }
}
