//! `07-cab-two-twelve` — the cab page as the plugin draws it (1500 × 860):
//! the 2×12 in the middle, a mic on each speaker, and a column of the
//! plugin's own flat controls either side — each mic's model dropdown and
//! on switch, its position, distance, level and pan on flat arc knobs, its
//! phase button, and the room send under the right-hand mic.
//!
//! The cab comes in the three finishes the heads wear (a `cab/finish`
//! choice repaints it): white, pink and clear acrylic. Drag a mic across
//! its speaker (position: from the dust cap out to the edge) and down away
//! from it (distance).

use std::fmt::Write as _;

use frame::document::proto::{CornerRadii, NodeId, TextAlign};
use frame::surface::kit::draw;
use frame::surface::kit::materials::texture::Relief;
use frame::surface::proto::{
    Axis, Binding, ButtonMode, Condition, Control, ControlKind, HandleAxis, Range, SlotRole, Unit,
    ValueFormat,
};
use frame::surface::widgets::Builder;

use super::PLUGIN;
use super::parts::{self, Hand, knob};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 1500.0;
const H: f64 = 860.0;
/// The cab's box.
const CX: f64 = 290.0;
const CY: f64 = 30.0;
const CW: f64 = 920.0;
const CH: f64 = 700.0;
const MICS: [&str; 6] = [
    "Dynamic A",
    "Dynamic B",
    "Condenser A",
    "Condenser B",
    "Ribbon A",
    "Ribbon B",
];
const FINISH: &str = "cab/finish";
const TEXT: &str = "#ececee";
const DIM: &str = "#8c8c92";

/// A finish: the covering, the cloth and its weave, the speakers' ink.
struct Finish {
    name: &'static str,
    tolex: &'static str,
    cloth: [&'static str; 2],
    thread: &'static str,
    ink: &'static str,
}

const FINISHES: [Finish; 3] = [
    Finish {
        name: "white",
        tolex: "#ebebe9",
        cloth: ["#d9d9d7", "#c6c6c4"],
        thread: "#ffffff1c",
        ink: "#0000001f",
    },
    Finish {
        name: "pink",
        tolex: "#e98aa6",
        cloth: ["#8e7c91", "#76647a"],
        thread: "#ffffff12",
        ink: "#00000024",
    },
    Finish {
        name: "clear",
        tolex: "#3a4046",
        cloth: ["#2a2d31", "#1d1f22"],
        thread: "#ffffff0c",
        ink: "#ffffff14",
    },
];

#[allow(clippy::too_many_arguments)]
fn text(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
) -> NodeId {
    parts::print(b, parent, s, cx, cy, size, color, weight, 0.02)
}

/// A speaker behind the cloth: the basket ring, the cone's rolls, the cap.
fn speaker(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, ink: &str) {
    b.add(
        parent,
        draw::circle(
            "cone",
            cx,
            cy,
            r,
            draw::radial_at(
                0.5,
                0.5,
                0.5,
                &[
                    (0.0, "#00000008"),
                    (0.7, "#0000000f"),
                    (0.92, "#0000001c"),
                    (1.0, "#ffffff0d"),
                ],
            ),
        ),
    );
    for f in [1.0, 0.95, 0.72, 0.52, 0.34] {
        b.add(
            parent,
            draw::stroked(
                draw::circle("roll", cx, cy, r * f, draw::solid("#00000000")),
                ink,
                1.6,
            ),
        );
        b.add(
            parent,
            draw::stroked(
                draw::circle(
                    "roll light",
                    cx + 1.0,
                    cy + 1.5,
                    r * f,
                    draw::solid("#00000000"),
                ),
                "#ffffff14",
                1.0,
            ),
        );
    }
    b.add(
        parent,
        draw::circle(
            "dust cap",
            cx,
            cy,
            r * 0.2,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff22"), (1.0, "#0000001c")]),
        ),
    );
}

/// One finish of the cab, filling `parent` (cab coordinates): shown while
/// `cab/finish` is `i` — a readout of the finish itself, its value slot
/// left hidden.
fn cab_body(
    b: &mut Builder<'_>,
    parent: NodeId,
    i: usize,
    f: &Finish,
    speakers: &[(f64, f64)],
    r: f64,
) {
    let root = b.add(
        parent,
        draw::frame(&format!("Cab {}", f.name), 0.0, 0.0, CW, CH),
    );
    let mut hidden = ui::text(
        "finish value",
        "",
        0.0,
        6.0,
        1.0,
        1.0,
        TEXT,
        TextAlign::Left,
        400.0,
        false,
    );
    hidden.opacity = 0.0;
    let hv = b.add(root, hidden);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, hv.0);
    c.binding = Some(ui::bind(FINISH));
    c.show_if = Some(Condition::any_of(FINISH, &[i as f64]));
    b.controls.insert(root, c);

    let rr = 22.0;
    let shadow = draw::rounded(
        draw::rect("cab shadow", 0.0, 0.0, CW, CH, draw::solid(f.tolex)),
        rr,
    );
    b.add(
        root,
        draw::with_effects(
            shadow,
            vec![draw::drop_shadow("#000000b0", 0.0, 14.0, 26.0)],
        ),
    );
    parts::covering(
        b,
        root,
        (0.0, 0.0, CW, CH),
        rr,
        f.tolex,
        Relief::Leather,
        0.8,
        1.0,
    );
    if f.name == "clear" {
        // Acrylic: a glassy sheen and bright cut edges.
        b.add(
            root,
            draw::rounded(
                draw::rect(
                    "acrylic sheen",
                    0.0,
                    0.0,
                    CW,
                    CH,
                    draw::linear(
                        120.0,
                        &[
                            (0.0, "#ffffff30"),
                            (0.35, "#ffffff08"),
                            (0.5, "#ffffff00"),
                            (0.8, "#ffffff10"),
                            (1.0, "#ffffff00"),
                        ],
                    ),
                ),
                rr,
            ),
        );
        b.add(
            root,
            draw::stroked(
                draw::rounded(
                    draw::rect(
                        "acrylic edge",
                        1.0,
                        1.0,
                        CW - 2.0,
                        CH - 2.0,
                        draw::solid("#00000000"),
                    ),
                    rr,
                ),
                "#dfe8f0aa",
                2.0,
            ),
        );
    }
    // The box's rolled edge: lit along the top, shaded along the bottom.
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "edge light",
                0.0,
                0.0,
                CW,
                22.0,
                draw::linear(180.0, &[(0.0, "#ffffff40"), (1.0, "#ffffff00")]),
            ),
            rr,
        ),
    );
    b.add(
        root,
        draw::rect(
            "edge shade",
            0.0,
            CH - 26.0,
            CW,
            26.0,
            draw::linear(180.0, &[(0.0, "#00000000"), (1.0, "#00000038")]),
        ),
    );
    // The baffle: the cloth, framed by the covering folding in.
    let m = 38.0;
    let (gw, gh) = (CW - 2.0 * m, CH - 2.0 * m);
    let mut grille = draw::frame("Grille", m, m, gw, gh);
    grille.fills = vec![draw::linear(180.0, &[(0.0, f.cloth[0]), (1.0, f.cloth[1])])];
    grille.clips_content = true;
    grille.corner_radii = CornerRadii::uniform(6.0);
    let grille = b.add(root, grille);
    let mut weave = String::new();
    let mut x = 0.0;
    while x < gw {
        let _ = write!(weave, "M {x:.1} 0 L {x:.1} {gh:.1} ");
        x += 3.0;
    }
    let mut y = 0.0;
    while y < gh {
        let _ = write!(weave, "M 0 {y:.1} L {gw:.1} {y:.1} ");
        y += 3.0;
    }
    b.add(
        grille,
        draw::stroked(
            draw::path("weave", 0.0, 0.0, gw, gh, weave, draw::solid("#00000000")),
            f.thread,
            0.8,
        ),
    );
    for (sx, sy) in speakers {
        speaker(b, grille, sx - m, sy - m, r, f.ink);
    }
    b.add(
        grille,
        draw::rect(
            "cloth sheen",
            0.0,
            0.0,
            gw,
            gh,
            draw::linear(
                120.0,
                &[(0.0, "#ffffff14"), (0.5, "#ffffff00"), (1.0, "#0000001a")],
            ),
        ),
    );
    b.add(
        grille,
        draw::rect(
            "recess top",
            0.0,
            0.0,
            gw,
            12.0,
            draw::linear(180.0, &[(0.0, "#00000050"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        grille,
        draw::rect(
            "recess left",
            0.0,
            0.0,
            10.0,
            gh,
            draw::linear(90.0, &[(0.0, "#00000040"), (1.0, "#00000000")]),
        ),
    );
    // Corner screws on the baffle's frame.
    for (sx, sy) in [
        (m * 0.5, m * 0.5),
        (CW - m * 0.5, m * 0.5),
        (m * 0.5, CH - m * 0.5),
        (CW - m * 0.5, CH - m * 0.5),
    ] {
        b.add(
            root,
            draw::circle(
                "screw",
                sx,
                sy,
                3.2,
                draw::radial_at(
                    0.35,
                    0.3,
                    0.9,
                    &[(0.0, "#fafafa"), (0.6, "#a4a4a8"), (1.0, "#56565a")],
                ),
            ),
        );
    }
}

/// A dynamic mic seen from the front at an angle, its grille head at the
/// handle's centre and its body running back along `lean` degrees (0 = up).
/// A `Handle`: across is its position on the cone, down its distance.
fn mic(b: &mut Builder<'_>, lane: NodeId, ns: &str, lean: f64) {
    let touch = 56.0;
    let handle = b.add(
        lane,
        draw::frame(&format!("{ns} placement"), 0.0, 0.0, touch, touch),
    );
    let (cx, cy) = (touch / 2.0, touch / 2.0);
    let a = lean.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    let (nx, ny) = (-uy, ux);
    let body = |from: f64, to: f64, w0: f64, w1: f64| {
        let (p0, p1) = (
            (cx + ux * from, cy + uy * from),
            (cx + ux * to, cy + uy * to),
        );
        [
            (p0.0 + nx * w0 / 2.0, p0.1 + ny * w0 / 2.0),
            (p1.0 + nx * w1 / 2.0, p1.1 + ny * w1 / 2.0),
            (p1.0 - nx * w1 / 2.0, p1.1 - ny * w1 / 2.0),
            (p0.0 - nx * w0 / 2.0, p0.1 - ny * w0 / 2.0),
        ]
    };
    let shadow: Vec<(f64, f64)> = body(0.0, 84.0, 28.0, 20.0)
        .iter()
        .map(|(x, y)| (x + 10.0, y + 16.0))
        .collect();
    b.add(
        handle,
        draw::with_effects(
            draw::path(
                "mic shadow",
                0.0,
                0.0,
                touch,
                touch,
                draw::polygon(&shadow),
                draw::solid("#00000040"),
            ),
            vec![draw::drop_shadow("#00000030", 0.0, 0.0, 8.0)],
        ),
    );
    let side = |dark: &str, lit: &str| {
        draw::linear(
            lean + 90.0,
            &[(0.0, dark), (0.35, lit), (0.55, dark), (1.0, "#050506")],
        )
    };
    // The shaft, the neck ring, then the grille head.
    b.add(
        handle,
        draw::path(
            "mic body",
            0.0,
            0.0,
            touch,
            touch,
            draw::polygon(&body(14.0, 84.0, 22.0, 18.0)),
            side("#0d0d0e", "#4a4a4e"),
        ),
    );
    b.add(
        handle,
        draw::path(
            "mic neck",
            0.0,
            0.0,
            touch,
            touch,
            draw::polygon(&body(12.0, 20.0, 26.0, 23.0)),
            side("#1a1a1c", "#6a6a6e"),
        ),
    );
    b.add(
        handle,
        draw::path(
            "mic head",
            0.0,
            0.0,
            touch,
            touch,
            draw::polygon(&body(-7.0, 13.0, 22.0, 28.0)),
            side("#141416", "#58585c"),
        ),
    );
    let mut mesh = String::new();
    for i in 0..6 {
        let t = -2.0 + f64::from(i) * 2.6;
        let half = 11.0 + t * 0.25;
        let (x, y) = (cx + ux * t, cy + uy * t);
        let _ = write!(
            mesh,
            "M {:.2} {:.2} L {:.2} {:.2} ",
            x + nx * half,
            y + ny * half,
            x - nx * half,
            y - ny * half
        );
    }
    b.add(
        handle,
        draw::stroked(
            draw::path(
                "mesh",
                0.0,
                0.0,
                touch,
                touch,
                mesh,
                draw::solid("#00000000"),
            ),
            "#7a7a7e",
            0.7,
        ),
    );
    b.add(
        handle,
        draw::circle(
            "capsule",
            cx - ux * 5.0,
            cy - uy * 5.0,
            10.5,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#58585c"), (1.0, "#0a0a0b")]),
        ),
    );
    let kind = ControlKind::Handle {
        x: Some(Binding::param(format!("{ns}/position"))),
        y: Some(Binding::param(format!("{ns}/distance"))),
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 1.0, max: 0.0 }),
    };
    b.controls.insert(handle, Control::new(kind));
}

/// A flat arc knob with its value over its caption.
#[allow(clippy::too_many_arguments)]
fn arc_knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    caption: &str,
    cx: f64,
    cy: f64,
    format: Option<ValueFormat>,
) {
    knob(b, parent, &parts::SCREEN, addr, caption, 64.0, cx, cy, TEXT);
    let r = b.add(
        parent,
        draw::frame(
            &format!("{caption} value"),
            cx - 50.0,
            cy + 40.0,
            100.0,
            18.0,
        ),
    );
    let t = b.add(
        r,
        draw::lettered(
            ui::text(
                "value",
                "",
                0.0,
                9.0,
                100.0,
                15.0,
                TEXT,
                TextAlign::Center,
                500.0,
                false,
            ),
            &Hand::Ui.lettering(500.0),
        ),
    );
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(addr));
    c.format = format;
    b.controls.insert(r, c);
    text(b, parent, caption, cx, cy + 70.0, 13.0, TEXT, 600.0);
}

/// A mic's model dropdown and its on switch.
fn mic_select(b: &mut Builder<'_>, parent: NodeId, ns: &str, x: f64, y: f64) {
    let (w, h) = (176.0, 44.0);
    let root = b.add(parent, draw::frame(&format!("{ns} model"), x, y, w, h));
    b.add(
        root,
        draw::rounded(
            draw::rect("box", 0.0, 0.0, w, h, draw::solid("#2b2b2e")),
            5.0,
        ),
    );
    let t = b.add(
        root,
        draw::lettered(
            ui::text(
                "value",
                "",
                16.0,
                h / 2.0,
                w - 40.0,
                16.0,
                TEXT,
                TextAlign::Left,
                400.0,
                false,
            ),
            &Hand::Ui.lettering(400.0),
        ),
    );
    ui::line(
        b,
        root,
        "chevron",
        &[
            (w - 26.0, h / 2.0 - 3.0),
            (w - 20.0, h / 2.0 + 3.0),
            (w - 14.0, h / 2.0 - 3.0),
        ],
        1.4,
        TEXT,
    );
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, t.0);
    c.binding = Some(ui::bind(&format!("{ns}/model")));
    b.controls.insert(root, c);
    // The on switch: a power glyph, lit while the mic is on.
    let p = b.add(
        parent,
        draw::frame(&format!("{ns} on"), x + w + 2.0, y, 62.0, h),
    );
    b.add(
        p,
        draw::rounded(
            draw::rect("box", 0.0, 0.0, 62.0, h, draw::solid("#2b2b2e")),
            5.0,
        ),
    );
    let glyph = |b: &mut Builder<'_>, node: NodeId, color: &str| {
        let d = "M 24.5 15 A 10 10 0 1 0 37.5 15";
        b.add(
            node,
            draw::stroked(
                draw::path(
                    "power ring",
                    0.0,
                    0.0,
                    62.0,
                    h,
                    d.into(),
                    draw::solid("#00000000"),
                ),
                color,
                2.0,
            ),
        );
        ui::line(
            b,
            node,
            "power bar",
            &[(31.0, 9.0), (31.0, 21.0)],
            2.0,
            color,
        );
    };
    let off = b.add(p, draw::frame("off", 0.0, 0.0, 62.0, h));
    glyph(b, off, DIM);
    let on = b.add(p, draw::frame("on", 0.0, 0.0, 62.0, h));
    glyph(b, on, TEXT);
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Off, off.0)
    .slot(SlotRole::On, on.0);
    c.binding = Some(ui::bind(&format!("{ns}/on")));
    b.controls.insert(p, c);
}

/// An outlined pill toggle (PHASE Ø).
fn phase(b: &mut Builder<'_>, parent: NodeId, ns: &str, cx: f64, cy: f64) {
    let (w, h) = (150.0, 46.0);
    let root = b.add(
        parent,
        draw::frame(&format!("{ns} phase"), cx - w / 2.0, cy - h / 2.0, w, h),
    );
    let off = b.add(root, draw::frame("off", 0.0, 0.0, w, h));
    b.add(
        off,
        draw::stroked(
            draw::rounded(
                draw::rect("pill", 1.0, 1.0, w - 2.0, h - 2.0, draw::solid("#00000000")),
                4.0,
            ),
            "#7a7a80",
            1.4,
        ),
    );
    let on = b.add(root, draw::frame("on", 0.0, 0.0, w, h));
    b.add(
        on,
        draw::rounded(
            draw::rect("pill", 0.0, 0.0, w, h, draw::solid("#e8e8ea")),
            4.0,
        ),
    );
    b.add(
        root,
        draw::lettered(
            ui::text(
                "label",
                "PHASE  Ø",
                0.0,
                h / 2.0,
                w,
                17.0,
                "#b8b8bd",
                TextAlign::Center,
                500.0,
                false,
            ),
            &Hand::Ui.lettering(500.0),
        ),
    );
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Off, off.0)
    .slot(SlotRole::On, on.0);
    c.binding = Some(ui::bind(&format!("{ns}/phase")));
    b.controls.insert(root, c);
}

/// A slide toggle (the room send's switch).
fn slide_toggle(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, cx: f64, cy: f64) {
    let (w, h) = (44.0, 44.0);
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h));
    let (px, py, pw, ph) = (6.0, h / 2.0 - 9.0, 32.0, 18.0);
    let off = b.add(root, draw::frame("off", 0.0, 0.0, w, h));
    b.add(
        off,
        draw::stroked(
            draw::rounded(
                draw::rect("pill", px, py, pw, ph, draw::solid("#00000000")),
                9.0,
            ),
            TEXT,
            1.6,
        ),
    );
    b.add(
        off,
        draw::circle("dot", px + 9.0, h / 2.0, 5.0, draw::solid(TEXT)),
    );
    let on = b.add(root, draw::frame("on", 0.0, 0.0, w, h));
    b.add(
        on,
        draw::rounded(draw::rect("pill", px, py, pw, ph, draw::solid(TEXT)), 9.0),
    );
    b.add(
        on,
        draw::circle("dot", px + pw - 9.0, h / 2.0, 5.5, draw::solid("#1b1b1d")),
    );
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Off, off.0)
    .slot(SlotRole::On, on.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// One mic's column of controls, `x0` its left edge.
fn mic_column(b: &mut Builder<'_>, root: NodeId, ns: &str, x0: f64) {
    mic_select(b, root, ns, x0, 60.0);
    arc_knob(
        b,
        root,
        &format!("{ns}/position"),
        "POSITION",
        x0 + 60.0,
        190.0,
        Some(ValueFormat::decimals(3)),
    );
    arc_knob(
        b,
        root,
        &format!("{ns}/distance"),
        "DISTANCE",
        x0 + 180.0,
        190.0,
        Some(ValueFormat::decimals(3)),
    );
    arc_knob(
        b,
        root,
        &format!("{ns}/level"),
        "MIC LEVEL",
        x0 + 60.0,
        360.0,
        Some(ValueFormat::default().signed()),
    );
    arc_knob(
        b,
        root,
        &format!("{ns}/pan"),
        "PAN",
        x0 + 180.0,
        360.0,
        Some(ValueFormat::decimals(0).mark(0.0, "C")),
    );
    ui::rule(b, root, x0, 482.0, 240.0, 1.0, "#ffffff1a");
    phase(b, root, ns, x0 + 120.0, 790.0);
}

/// The finish picker: three lettered chips under the cab.
fn finish_picker(b: &mut Builder<'_>, root: NodeId) {
    let (w, h) = (96.0, 44.0);
    for (i, f) in FINISHES.iter().enumerate() {
        let cx = W / 2.0 + (i as f64 - 1.0) * (w + 16.0);
        let chip = b.add(
            root,
            draw::frame(
                &format!("{} finish", f.name),
                cx - w / 2.0,
                H - h - 36.0,
                w,
                h,
            ),
        );
        b.add(
            chip,
            draw::rounded(
                draw::rect("chip", 0.0, 0.0, w, h, draw::solid("#161618cc")),
                h / 2.0,
            ),
        );
        b.add(
            chip,
            draw::circle("swatch", 20.0, h / 2.0, 8.0, draw::solid(f.tolex)),
        );
        b.add(
            chip,
            draw::lettered(
                ui::text(
                    "name",
                    f.name,
                    32.0,
                    h / 2.0,
                    w - 36.0,
                    14.0,
                    DIM,
                    TextAlign::Left,
                    500.0,
                    false,
                ),
                &Hand::Geo.lettering(500.0),
            ),
        );
        let lit = b.add(chip, draw::frame("lit", 0.0, 0.0, w, h));
        b.add(
            lit,
            draw::stroked(
                draw::rounded(
                    draw::rect("ring", 1.0, 1.0, w - 2.0, h - 2.0, draw::solid("#00000000")),
                    h / 2.0,
                ),
                TEXT,
                1.5,
            ),
        );
        b.add(
            lit,
            draw::lettered(
                ui::text(
                    "name",
                    f.name,
                    32.0,
                    h / 2.0,
                    w - 36.0,
                    14.0,
                    TEXT,
                    TextAlign::Left,
                    500.0,
                    false,
                ),
                &Hand::Geo.lettering(500.0),
            ),
        );
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: FINISHES.len() as u32,
            },
        })
        .slot(SlotRole::Led, lit.0);
        c.binding = Some(ui::bind(FINISH));
        b.controls.insert(chip, c);
    }
}

pub fn face() -> Face {
    let scene = canvas("Two Twelve Cab (cab page, 1500×860)", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut ground = draw::rect("page", 0.0, 0.0, W, H, draw::solid("#1b1b1d"));
        ground.corner_radii = CornerRadii::uniform(10.0);
        b.add(root, ground);
        // Feet under the cab, then the cab in its finishes.
        for fx in [0.1, 0.9] {
            b.add(
                root,
                draw::rounded(
                    draw::rect(
                        "foot",
                        CX + CW * fx - 40.0,
                        CY + CH - 12.0,
                        80.0,
                        30.0,
                        draw::linear(
                            90.0,
                            &[(0.0, "#0a0a0b"), (0.4, "#39393c"), (1.0, "#0a0a0b")],
                        ),
                    ),
                    5.0,
                ),
            );
        }
        let cab = b.add(root, draw::frame("Cab", CX, CY, CW, CH));
        let r = 190.0;
        let speakers = [(CW * 0.27, CH * 0.56), (CW * 0.73, CH * 0.56)];
        for (i, f) in FINISHES.iter().enumerate() {
            cab_body(b, cab, i, f, &speakers, r);
        }
        // The mics ride over every finish: a lane across each speaker (the
        // dust cap at its left end for mic 1, its right end for mic 2).
        for (k, (sx, sy)) in speakers.iter().enumerate() {
            let (lx, lean) = if k == 0 {
                (*sx - 20.0, -50.0)
            } else {
                (*sx - r + 20.0, 50.0)
            };
            let lane = b.add(
                cab,
                draw::frame(&format!("Mic {} lane", k + 1), lx, *sy - 30.0, r, 150.0),
            );
            mic(b, lane, &format!("mic{}", k + 1), lean);
        }

        mic_column(b, root, "mic1", 24.0);
        mic_column(b, root, "mic2", W - 24.0 - 240.0);
        // The room send, under the right-hand mic.
        let rx = W - 24.0 - 120.0;
        text(b, root, "ROOM SEND", rx - 18.0, 520.0, 15.0, TEXT, 600.0);
        slide_toggle(b, root, "room/on", "Room send on", rx + 70.0, 520.0);
        arc_knob(
            b,
            root,
            "room/send",
            "LEVEL",
            rx,
            600.0,
            Some(ValueFormat::default().signed()),
        );
        finish_picker(b, root);
    });
    let mut params = vec![parts::pick(
        "cab",
        "finish",
        "Cab Finish",
        &["White", "Pink", "Clear"],
        0,
    )];
    for (i, pos, dist) in [(1, 0.5, 0.0), (2, 0.5, 0.0)] {
        let ns = format!("mic{i}");
        params.push(
            ui::param(
                &format!("{ns}/position"),
                &format!("Mic {i} Position"),
                Range::linear(0.0, 1.0, Unit::None),
                pos,
            )
            .featured(),
        );
        params.push(ui::param(
            &format!("{ns}/distance"),
            &format!("Mic {i} Distance"),
            Range::linear(0.0, 1.0, Unit::None),
            dist,
        ));
        params.push(parts::pick(
            &ns,
            "model",
            &format!("Mic {i} Model"),
            &MICS,
            if i == 1 { 5 } else { 0 },
        ));
        params.push(parts::on(&ns, "on", &format!("Mic {i} On"), true));
        params.push(parts::db(&ns, "level", &format!("Mic {i} Level"), -24.0, 12.0, 0.0).bipolar());
        params.push(
            ui::param(
                &format!("{ns}/pan"),
                &format!("Mic {i} Pan"),
                Range::linear(-100.0, 100.0, Unit::None),
                0.0,
            )
            .bipolar(),
        );
        params.push(parts::on(&ns, "phase", &format!("Mic {i} Phase"), false));
    }
    params.push(parts::on("room", "on", "Room Send", false));
    params.push(parts::db(
        "room",
        "send",
        "Room Send Level",
        -60.0,
        12.0,
        0.0,
    ));
    Face {
        file: "07-cab-two-twelve",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}
