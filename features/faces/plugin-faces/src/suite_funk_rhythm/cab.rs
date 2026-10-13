//! `09-cab-900x900`: the cab page (900 × 900) — a wood-framed 4×12 with
//! four black speakers whose baffle follows the cab choice (white, blue or
//! green: each look a layer shown for its option), the three cabs as glyph
//! buttons under it (a select bank on `cab/model`), and the rig's shared
//! mic controls either side: for each of two mics a dropdown of six mics
//! and an on switch, position, distance, level and pan knobs with their
//! readouts, and a phase button; between them the room send and its level.
//!
//! Each mic also sits on the cab as a handle: drag it across its speaker
//! for position (the dust cap out to the edge) and up for distance.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::materials::{Dir, Material, fill};
use frame::surface::kit::{Place, TOUCH_MIN, draw};
use frame::surface::proto::{ButtonMode, Control, ControlKind, Range, SlotRole, Unit, ValueFormat};
use frame::surface::widgets::Builder;

use super::parts::{self, CAB, Type};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

pub(crate) const MICS: &[&str] = &[
    "Dynamic 1",
    "Dynamic 2",
    "Condenser",
    "Ribbon 1",
    "Ribbon 2",
    "Small Condenser",
];
const CABS: &[&str] = &["White 4x12", "Blue 4x12", "Green 4x12"];
/// Each cab's baffle.
const BAFFLES: [&str; 3] = ["#e9e9e7", "#a9bcc8", "#a7cbb9"];
const INK: &str = "#2b2624";
const DIM: &str = "#7a6e69";
/// The cab on the page: x, y, side.
const BOX: (f64, f64, f64) = (250.0, 36.0, 400.0);

/// A maple-framed 4×12 at (`x`, `y`), `s` square, baffle `baffle`.
fn cabinet(b: &mut Builder<'_>, parent: NodeId, (x, y, s): (f64, f64, f64), baffle: &str) {
    // Feet.
    for (fx, lean) in [(x + s * 0.08, -1.0), (x + s * 0.8, 1.0)] {
        let foot = draw::polygon(&[
            (fx, y + s - 4.0),
            (fx + s * 0.12, y + s - 4.0),
            (fx + s * 0.12 + 5.0 * lean, y + s + 16.0),
            (fx + 5.0 * lean, y + s + 16.0),
        ]);
        b.add(
            parent,
            draw::path(
                "foot",
                0.0,
                0.0,
                CAB,
                CAB,
                foot,
                draw::linear(
                    90.0,
                    &[(0.0, "#a8764a"), (0.5, "#d3a171"), (1.0, "#96643c")],
                ),
            ),
        );
    }
    let mut frame = draw::rounded(draw::frame("cab", x, y, s, s), 10.0);
    frame.effects = vec![draw::drop_shadow("#00000070", 0.0, 8.0, 16.0)];
    let frame = b.add(parent, frame);
    fill(
        b,
        frame,
        (0.0, 0.0, s, s),
        10.0,
        &Material::Wood {
            color: "#c89462".into(),
            dir: Dir::Horizontal,
        },
    );
    for n in frame::surface::kit::materials::light::edge(
        "frame edge",
        (0.0, 0.0, s, s),
        10.0,
        6.0,
        1.0,
        true,
    ) {
        b.add(frame, n);
    }
    let m = s * 0.07;
    // The inner lip of the frame, then the baffle.
    b.add(
        frame,
        draw::stroked(
            draw::rect(
                "lip",
                m - 5.0,
                m - 5.0,
                s - 2.0 * m + 10.0,
                s - 2.0 * m + 10.0,
                draw::solid("#00000000"),
            ),
            "#7a4f2c66",
            3.0,
        ),
    );
    let mut panel = draw::frame("baffle", m, m, s - 2.0 * m, s - 2.0 * m);
    panel.fills = vec![draw::radial_at(
        0.45,
        0.4,
        0.8,
        &[
            (0.0, &ui::blend(baffle, "#ffffff", 0.25)),
            (1.0, &ui::blend(baffle, "#000000", 0.1)),
        ],
    )];
    let panel = b.add(frame, panel);
    for n in frame::surface::kit::materials::light::edge(
        "baffle recess",
        (0.0, 0.0, s - 2.0 * m, s - 2.0 * m),
        0.0,
        6.0,
        1.0,
        false,
    ) {
        b.add(panel, n);
    }
    let q = (s - 2.0 * m) / 4.0;
    let r = q * 0.9;
    for (i, j) in [(1.0, 1.0), (3.0, 1.0), (1.0, 3.0), (3.0, 3.0)] {
        let (cx, cy) = (x + m + q * i, y + m + q * j);
        // The baffle dishes in round each speaker.
        b.add(
            parent,
            draw::circle(
                "dish",
                cx + 2.0,
                cy + 3.0,
                r * 1.12,
                draw::radial(&[(0.8, "#00000000"), (0.93, "#00000026"), (1.0, "#00000000")]),
            ),
        );
        b.add(
            parent,
            draw::circle(
                "dish light",
                cx - 2.0,
                cy - 3.0,
                r * 1.12,
                draw::radial(&[(0.85, "#ffffff00"), (0.95, "#ffffff40"), (1.0, "#ffffff00")]),
            ),
        );
        b.add(
            parent,
            draw::with_effects(
                draw::circle("basket", cx, cy, r, draw::solid("#141415")),
                vec![draw::drop_shadow("#00000066", 0.0, 2.0, 5.0)],
            ),
        );
        let cone = draw::circle(
            "cone",
            cx,
            cy,
            r * 0.84,
            draw::conic(
                0.0,
                &[
                    (0.0, "#2c2c2e"),
                    (0.25, "#141415"),
                    (0.5, "#303033"),
                    (0.75, "#121213"),
                    (1.0, "#2c2c2e"),
                ],
            ),
        );
        b.add(parent, cone);
        let rings: Vec<Vec<(f64, f64)>> = (1..9)
            .map(|k| parts::ring(cx, cy, r * (0.3 + 0.065 * f64::from(k))))
            .collect();
        b.add(
            parent,
            ui::strokes("ridges", &rings, 1.0, draw::solid("#4a4a4e66")),
        );
        b.add(
            parent,
            draw::circle(
                "dust cap",
                cx,
                cy,
                r * 0.22,
                draw::radial_at(
                    0.38,
                    0.32,
                    0.8,
                    &[(0.0, "#9a9a9e"), (0.4, "#303033"), (1.0, "#0a0a0b")],
                ),
            ),
        );
        for a in 0..8 {
            let (sx, sy) = draw::polar(cx, cy, r * 0.93, 22.5 + 45.0 * f64::from(a));
            b.add(
                parent,
                draw::circle("bolt", sx, sy, 2.2, draw::solid("#3a3a3c")),
            );
        }
    }
}

/// The centre and radius of the cab's speaker `i` (0 top-left, 1 top-right).
fn speaker(i: usize) -> (f64, f64, f64) {
    let (x, y, s) = BOX;
    let m = s * 0.07;
    let q = (s - 2.0 * m) / 4.0;
    (
        x + m + q * if i == 0 { 1.0 } else { 3.0 },
        y + m + q,
        q * 0.9,
    )
}

/// Mic `m` on a handle over speaker `i`.
fn mic(b: &mut Builder<'_>, parent: NodeId, i: usize, m: &str) {
    parts::mic_handle(
        b,
        parent,
        speaker(i),
        &format!("Mic {}", m.to_uppercase()),
        &format!("cab/{m}_pos"),
        &format!("cab/{m}_dist"),
    );
}

/// A select-bank button: a glyph of an amp over its cab, lit when chosen.
fn cab_glyph(b: &mut Builder<'_>, parent: NodeId, i: usize, (x, y): (f64, f64)) {
    let (w, h) = (54.0, 50.0);
    let root = b.add(parent, draw::frame(&format!("Cab {}", i + 1), x, y, w, h));
    let on = b.add(
        root,
        draw::rounded(
            draw::rect("chosen", 0.0, 0.0, w, h, draw::solid("#fbf7f4")),
            3.0,
        ),
    );
    let lines = vec![
        parts::round_rect(6.0, 5.0, w - 12.0, 13.0, 1.0),
        parts::round_rect(4.0, 22.0, w - 8.0, 24.0, 1.0),
        parts::ring(17.0, 34.0, 7.0),
        parts::ring(w - 17.0, 34.0, 7.0),
    ];
    b.add(root, ui::strokes("glyph", &lines, 2.0, draw::solid(INK)));
    let c = Control::new(ControlKind::Button {
        mode: ButtonMode::Select {
            index: i as u32,
            count: 3,
        },
    })
    .slot(SlotRole::On, on.0);
    let mut c = c;
    c.binding = Some(ui::bind("cab/model"));
    b.controls.insert(root, c);
}

/// A dropdown of `options` on `addr`: a dark pill whose text is the
/// choice, stepped by wheel or arrows.
fn dropdown(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    options: &[&str],
) {
    let root = b.add(parent, draw::rounded(draw::frame(name, x, y, w, h), 6.0));
    if let Some(n) = b.scene.node_mut(root) {
        n.fills = vec![draw::solid("#141414")];
    }
    let value = b.add(
        root,
        ui::text(
            "value",
            "",
            14.0,
            h / 2.0,
            w - 44.0,
            15.0,
            "#f4f4f2",
            TextAlign::Left,
            500.0,
            false,
        ),
    );
    b.add(
        root,
        ui::strokes(
            "chevron",
            &[vec![
                (w - 26.0, h / 2.0 + 3.0),
                (w - 20.0, h / 2.0 - 3.0),
                (w - 14.0, h / 2.0 + 3.0),
            ]],
            1.8,
            draw::solid("#f4f4f2"),
        ),
    );
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(ui::bind(addr));
    c.format = Some(ValueFormat::labels(options));
    b.controls.insert(root, c);
}

/// A latching square toggle with a legend: dark when on.
fn toggle_button(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    legend: &str,
    (x, y, w, h): (f64, f64, f64, f64),
) {
    let root = b.add(parent, draw::frame(legend, x, y, w, h));
    let off = b.add(
        root,
        draw::stroked(
            draw::rounded(
                draw::rect("off", 0.0, 0.0, w, h, draw::solid("#fbf8f6")),
                6.0,
            ),
            "#b9aeaa",
            1.2,
        ),
    );
    let on = b.add(
        root,
        draw::rounded(
            draw::rect("on", 0.0, 0.0, w, h, draw::solid("#2b2624")),
            6.0,
        ),
    );
    let t_off = b.add(
        root,
        ui::text(
            "legend",
            legend,
            0.0,
            h / 2.0,
            w,
            13.0,
            INK,
            TextAlign::Center,
            600.0,
            true,
        ),
    );
    let t_on = b.add(
        root,
        ui::text(
            "legend",
            legend,
            0.0,
            h / 2.0,
            w,
            13.0,
            "#fbf8f6",
            TextAlign::Center,
            600.0,
            true,
        ),
    );
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::On, on.0)
    .slot(SlotRole::On, t_on.0)
    .slot(SlotRole::Off, t_off.0)
    .slot(SlotRole::Off, off.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

/// A flat knob with its value under it and its caption under that.
fn flat(b: &mut Builder<'_>, parent: NodeId, addr: &str, caption: &str, (cx, cy): (f64, f64)) {
    let p = Place::new(caption)
        .size(46.0)
        .variant("light")
        .bind(ui::bind(addr))
        .touch(TOUCH_MIN)
        .options(&["track=#00000024"]);
    let k = ui::put(b, parent, "knob.flat_line", &p, cx, cy);
    ui::strip_legend(b, k);
    ui::add_value(b, k, 13.0, INK);
    // A pan reads C at its centre; a level signed.
    if addr.ends_with("pan") {
        parts::format(b, k, ValueFormat::decimals(0).mark(0.0, "C"));
    } else if addr.ends_with("level") {
        parts::format(b, k, ValueFormat::decimals(1).signed());
    }
    parts::centred(
        b,
        parent,
        caption,
        (cx, cy + 58.0),
        Type::new(10.5, DIM, 600.0).upper().track(0.04),
    );
}

fn mic_panel(b: &mut Builder<'_>, parent: NodeId, m: &str, x0: f64) {
    let pre = format!("cab/{m}_");
    let a = |s: &str| format!("{pre}{s}");
    dropdown(
        b,
        parent,
        &a("mic"),
        &format!("Mic {}", m.to_uppercase()),
        (x0, 560.0, 250.0, 44.0),
        MICS,
    );
    toggle_button(b, parent, &a("on"), "On", (x0 + 262.0, 560.0, 60.0, 44.0));
    for (i, (id, cap)) in [
        ("pos", "Position"),
        ("dist", "Distance"),
        ("level", "Mic Level"),
        ("pan", "Pan"),
    ]
    .iter()
    .enumerate()
    {
        flat(b, parent, &a(id), cap, (x0 + 40.0 + 82.0 * i as f64, 676.0));
    }
    toggle_button(
        b,
        parent,
        &a("phase"),
        "Phase Ø",
        (x0 + 100.0, 790.0, 120.0, 44.0),
    );
}

pub(super) fn face() -> Face {
    let scene = canvas("Funk Cab", |b, root| {
        parts::page(b, root, CAB, CAB);
        let mut bg = draw::frame("room", 0.0, 0.0, CAB, CAB);
        bg.fills = vec![draw::linear(
            180.0,
            &[
                (0.0, "#f6ddd4"),
                (0.56, "#f3d6cc"),
                (0.561, "#e8c9bd"),
                (1.0, "#f3d9cf"),
            ],
        )];
        let bg = b.add(root, bg);
        // The cab, in the chosen look.
        for (i, baffle) in BAFFLES.iter().enumerate() {
            let layer = parts::variant_layer(b, bg, "cab/model", i, (0.0, 0.0, CAB, 470.0));
            cabinet(b, layer, BOX, baffle);
        }
        mic(b, bg, 0, "a");
        mic(b, bg, 1, "b");
        for i in 0..3 {
            cab_glyph(b, bg, i, (365.0 + 58.0 * i as f64, 474.0));
        }
        parts::centred(
            b,
            bg,
            "MIC A",
            (192.0, 540.0),
            Type::new(11.0, DIM, 700.0).track(0.08),
        );
        parts::centred(
            b,
            bg,
            "MIC B",
            (707.0, 540.0),
            Type::new(11.0, DIM, 700.0).track(0.08),
        );
        mic_panel(b, bg, "a", 30.0);
        mic_panel(b, bg, "b", 548.0);
        // The room send, between the mics.
        b.add(
            bg,
            draw::rect(
                "rule",
                400.0,
                870.0 - 40.0,
                100.0,
                1.0,
                draw::solid("#0000001f"),
            ),
        );
        parts::centred(
            b,
            bg,
            "ROOM SEND",
            (450.0, 842.0),
            Type::new(10.5, DIM, 700.0).track(0.06),
        );
        toggle_button(b, bg, "cab/room_on", "Room", (412.0, 850.0, 76.0, 44.0));
        let p = Place::new("Room level")
            .size(40.0)
            .variant("light")
            .bind(ui::bind("cab/room_level"))
            .touch(TOUCH_MIN);
        let k = ui::put(b, bg, "knob.flat_line", &p, 450.0, 780.0);
        ui::strip_legend(b, k);
        ui::add_value(b, k, 12.0, INK);
        parts::format(b, k, ValueFormat::decimals(1).signed());
    });
    let mut params = vec![parts::pick("cab/model", "Cab", CABS, 0)];
    for m in ["a", "b"] {
        let a = |s: &str| format!("cab/{m}_{s}");
        let up = m.to_uppercase();
        params.extend([
            parts::pick(
                &a("mic"),
                &format!("Mic {up}"),
                MICS,
                if m == "a" { 0 } else { 3 },
            ),
            parts::switch(&a("on"), &format!("Mic {up} On"), true),
            ui::param(
                &a("pos"),
                &format!("Mic {up} Position"),
                Range::linear(0.0, 1.0, Unit::None),
                0.35,
            )
            .featured(),
            ui::param(
                &a("dist"),
                &format!("Mic {up} Distance"),
                Range::linear(0.0, 1.0, Unit::None),
                0.15,
            )
            .featured(),
            parts::db(&a("level"), &format!("Mic {up} Level"), -24.0, 12.0, 0.0),
            ui::param(
                &a("pan"),
                &format!("Mic {up} Pan"),
                Range::linear(-100.0, 100.0, Unit::Percent),
                0.0,
            )
            .featured()
            .bipolar(),
            parts::switch(&a("phase"), &format!("Mic {up} Phase"), false),
        ]);
    }
    params.push(parts::switch("cab/room_on", "Room Send", true));
    params.push(parts::db("cab/room_level", "Room Level", -24.0, 12.0, -4.0));
    ui::formatted(Face {
        file: "09-cab-900x900",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    })
}
